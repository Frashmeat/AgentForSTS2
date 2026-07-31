//! batch_custom_code handler：批量跑 custom_code 生成。
//!
//! 输入是 N 个 CustomCodegenRequest，每个独立装 prompt → LLM → 解 fence → 落
//! `<artifacts_dir>/<sanitized name>/<name>.cs`。单 item 失败不影响其它，除非
//! `fail_fast = true`。每个 item 的结果（成功路径或失败原因）汇总到 run.result.items。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::code_generate::{
    GenerateError, PublishedRunArtifact, WrittenArtifact, generate_and_write_code_artifact,
    publish_generated_artifact, sanitize_entity_name,
};
use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, finalize_with_error, finalize_with_success,
    transition_to_running,
};
use crate::codegen::{CustomCodegenRequest, PromptAssembler};
use crate::game_pack::VerifiedGameContext;
use crate::llm::LlmClient;
use crate::platform::artifact::sha256_bytes;
use crate::platform::contracts::SubmitBatchCustomCodeRequest;
use crate::platform::domain::{
    BatchArtifactItemResult, RunId, RunRepository, RunResult, RunStatus, TokenUsage,
};

struct ItemOutcome {
    success: bool,
    result: BatchArtifactItemResult,
    error: Option<String>,
    pending_commit: Option<PendingItemCommit>,
}

struct PendingItemCommit {
    artifact: WrittenArtifact,
    published: PublishedRunArtifact,
}

pub async fn run_batch_custom_code(
    repo: Arc<dyn RunRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    run_id: RunId,
    request: SubmitBatchCustomCodeRequest,
    game_context: VerifiedGameContext,
    artifacts_dir: PathBuf,
) {
    if transition_to_running(&repo, &run_id, &sink).await.is_err() {
        return;
    }
    if request.items.is_empty() {
        finalize_with_error(&repo, &run_id, &sink, "items list is empty").await;
        return;
    }

    let assembler = PromptAssembler::built_in();
    let total = request.items.len();
    let mut outcomes: Vec<ItemOutcome> = Vec::with_capacity(total);
    let mut succeeded: u32 = 0;
    let mut failed: u32 = 0;

    for (idx, item) in request.items.into_iter().enumerate() {
        // 中途检查 cancel：若被取消则停止后续 item 但保留已完成结果到 run.result。
        if let Ok(j) = repo.get(&run_id).await
            && matches!(j.status, RunStatus::Cancelled)
        {
            break;
        }

        let entity_name = sanitize_entity_name(&item.name);
        sink.emit(ProgressEvent {
            run_id: run_id.clone(),
            stage: "item-start".into(),
            percent: Some((idx as f32) / (total as f32)),
            message: Some(format!("{}/{}: {}", idx + 1, total, entity_name)),
            delta: None,
        })
        .await;

        let outcome = process_one_item(
            &assembler,
            &repo,
            &llm,
            &sink,
            &run_id,
            &game_context,
            &artifacts_dir,
            &item,
            &entity_name,
        )
        .await;

        let item_success = outcome.success;
        let item_message = outcome
            .error
            .clone()
            .unwrap_or_else(|| format!("{}/{} done", idx + 1, total));
        if item_success {
            succeeded += 1;
        } else {
            failed += 1;
        }
        outcomes.push(outcome);

        sink.emit(ProgressEvent {
            run_id: run_id.clone(),
            stage: if item_success {
                "item-completed".into()
            } else {
                "item-failed".into()
            },
            percent: Some(((idx + 1) as f32) / (total as f32)),
            message: Some(item_message),
            delta: None,
        })
        .await;

        if !item_success && request.fail_fast {
            break;
        }
    }

    if matches!(
        repo.get(&run_id).await.map(|run| run.status),
        Ok(RunStatus::Cancelled)
    ) {
        rollback_pending_items(&mut outcomes).await;
        return;
    }
    let overall_ok = succeeded > 0;
    if !overall_ok {
        let first_error = outcomes
            .iter()
            .find_map(|outcome| outcome.error.as_deref())
            .unwrap_or("unknown item failure");
        finalize_with_error(
            &repo,
            &run_id,
            &sink,
            &format!("all {failed} items failed: {first_error}"),
        )
        .await;
        return;
    }
    let result = RunResult::BatchArtifactProduction {
        total,
        succeeded: succeeded as usize,
        failed: failed as usize,
        items: outcomes
            .iter()
            .map(|outcome| outcome.result.clone())
            .collect(),
    };
    if !matches!(
        finalize_with_success(&repo, &run_id, result).await,
        FinalizeOutcome::Succeeded
    ) {
        rollback_pending_items(&mut outcomes).await;
        return;
    }
    commit_pending_items(&mut outcomes).await;

    sink.emit(ProgressEvent {
        run_id,
        stage: if overall_ok {
            "completed".into()
        } else {
            "failed".into()
        },
        percent: Some(1.0),
        message: Some(format!("succeeded={succeeded}, failed={failed}")),
        delta: None,
    })
    .await;
}

#[allow(clippy::too_many_arguments)] // 同 run_asset_generate，DI 注入式 handler
async fn process_one_item(
    assembler: &PromptAssembler,
    repo: &Arc<dyn RunRepository>,
    llm: &Arc<dyn LlmClient>,
    sink: &Arc<dyn ProgressSink>,
    run_id: &RunId,
    game_context: &VerifiedGameContext,
    artifacts_dir: &Path,
    item: &CustomCodegenRequest,
    entity_name: &str,
) -> ItemOutcome {
    let assembly = match assembler.assemble_custom_code_prompt_with_evidence(item, game_context) {
        Ok(assembly) => assembly,
        Err(err) => {
            return ItemOutcome {
                success: false,
                result: failed_batch_item(entity_name),
                error: Some(format!("prompt assembly: {err}")),
                pending_commit: None,
            };
        }
    };
    let prompt = assembly.prompt;
    let inputs_sha256 = sha256_bytes(prompt.as_bytes());
    match generate_and_write_code_artifact(
        Arc::clone(repo),
        Arc::clone(llm),
        Arc::clone(sink),
        run_id,
        prompt,
        entity_name,
        artifacts_dir,
        &game_context.pack().validation_rules,
    )
    .await
    {
        Ok(mut art) => {
            let published = publish_generated_artifact(
                artifacts_dir,
                run_id,
                game_context,
                "custom_code",
                art.entity_name.clone(),
                art.model.clone(),
                inputs_sha256,
                TokenUsage {
                    input_tokens: art.usage_in,
                    output_tokens: art.usage_out,
                },
                assembly.evidence,
                vec![("csharp".into(), art.cs_path.clone())],
                Vec::new(),
            )
            .await;
            match published {
                Ok(published) => {
                    let result = match &published.result {
                        RunResult::ArtifactProduction {
                            artifact_manifest_ref,
                            manifest_sha256,
                            artifact_id,
                            ..
                        } => BatchArtifactItemResult {
                            item_id: artifact_id.clone(),
                            artifact_manifest_ref: Some(artifact_manifest_ref.clone()),
                            manifest_sha256: Some(manifest_sha256.clone()),
                            diagnostic_ref: None,
                        },
                        _ => unreachable!("generation helper returns artifact production"),
                    };
                    ItemOutcome {
                        success: true,
                        result,
                        error: None,
                        pending_commit: Some(PendingItemCommit {
                            artifact: art,
                            published,
                        }),
                    }
                }
                Err(error) => {
                    let rollback = art.rollback_writes().await;
                    ItemOutcome {
                        success: false,
                        result: failed_batch_item(entity_name),
                        error: Some(match rollback {
                            Ok(()) => error,
                            Err(rollback_error) => format!(
                                "{error}; rollback generated files failed: {rollback_error}"
                            ),
                        }),
                        pending_commit: None,
                    }
                }
            }
        }
        Err(GenerateError::Cancelled) => ItemOutcome {
            success: false,
            result: failed_batch_item(entity_name),
            error: Some("cancelled".into()),
            pending_commit: None,
        },
        Err(GenerateError::Stream(msg)) => ItemOutcome {
            success: false,
            result: failed_batch_item(entity_name),
            error: Some(format!("stream: {msg}")),
            pending_commit: None,
        },
        Err(GenerateError::ModelOutput(msg)) => ItemOutcome {
            success: false,
            result: failed_batch_item(entity_name),
            error: Some(format!("invalid code model output: {msg}")),
            pending_commit: None,
        },
        Err(GenerateError::Write(msg)) => ItemOutcome {
            success: false,
            result: failed_batch_item(entity_name),
            error: Some(format!("write: {msg}")),
            pending_commit: None,
        },
    }
}

async fn rollback_pending_items(outcomes: &mut [ItemOutcome]) {
    for outcome in outcomes.iter_mut().rev() {
        if let Some(mut pending) = outcome.pending_commit.take() {
            let artifact_rollback = pending.published.rollback().await;
            let file_rollback = pending.artifact.rollback_writes().await;
            let _ = (artifact_rollback, file_rollback);
        }
    }
}

async fn commit_pending_items(outcomes: &mut [ItemOutcome]) {
    for outcome in outcomes {
        if let Some(mut pending) = outcome.pending_commit.take() {
            let _ = pending.published.commit().await;
            pending.artifact.commit_writes();
        }
    }
}

fn failed_batch_item(item_id: &str) -> BatchArtifactItemResult {
    BatchArtifactItemResult {
        item_id: item_id.to_string(),
        artifact_manifest_ref: None,
        manifest_sha256: None,
        diagnostic_ref: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::test_support::fixture_game_context;
    use crate::llm::{
        CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmError,
        StreamEvent, Usage,
    };
    use crate::platform::application::RunApplicationService;
    use crate::platform::domain::RunRepository;
    use crate::platform::infra::FileRunRepository;
    use async_trait::async_trait;
    use futures_util::stream;
    use std::sync::Mutex;

    /// 按调用次数轮转预设响应：第 N 次 stream() 返回第 N 组事件。
    struct RotatingLlm {
        responses: Mutex<Vec<Vec<Result<StreamEvent, LlmError>>>>,
    }

    #[async_trait]
    impl LlmClient for RotatingLlm {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            unimplemented!()
        }
        async fn stream(&self, _: CompletionRequest) -> Result<CompletionStream, LlmError> {
            let next = {
                let mut g = self.responses.lock().unwrap();
                if g.is_empty() {
                    return Err(LlmError::Stream("no scripted response left".into()));
                }
                g.remove(0)
            };
            Ok(Box::pin(stream::iter(next)))
        }
    }

    fn ok_code_response(model: &str, body: &str) -> Vec<Result<StreamEvent, LlmError>> {
        vec![
            Ok(StreamEvent::Start {
                model: model.into(),
            }),
            Ok(StreamEvent::Delta {
                text: format!("```csharp\n{body}\n```"),
            }),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::EndTurn,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            }),
        ]
    }

    async fn wait_terminal(service: &RunApplicationService, id: &RunId) {
        for _ in 0..200 {
            let run = service.get(id).await.unwrap();
            if run.status.is_terminal() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    fn make_request(names: &[&str]) -> SubmitBatchCustomCodeRequest {
        SubmitBatchCustomCodeRequest {
            items: names
                .iter()
                .map(|n| CustomCodegenRequest {
                    name: (*n).into(),
                    ..Default::default()
                })
                .collect(),
            fail_fast: false,
        }
    }

    #[tokio::test]
    async fn batch_custom_code_all_succeed() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![
                ok_code_response("m1", "public class A {}"),
                ok_code_response("m1", "public class B {}"),
            ]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let kp = fixture_game_context(td.path(), &[], &[]);
        let req = make_request(&["AlphaHook", "BetaHook"]);
        let id = service
            .submit_batch_custom_code(req, kp, artifacts.clone(), sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        let res = serde_json::to_value(run.result.expect("result")).unwrap();
        assert_eq!(res["total"], 2);
        assert_eq!(res["succeeded"], 2);
        assert_eq!(res["failed"], 0);

        assert!(td.path().join("Generated/AlphaHook.cs").exists());
        assert!(td.path().join("Generated/BetaHook.cs").exists());
        for item in res["items"].as_array().unwrap() {
            let manifest_ref = item["artifactManifestRef"].as_str().unwrap();
            assert!(td.path().join(manifest_ref).is_file());
        }
    }

    #[tokio::test]
    async fn batch_custom_code_partial_failure_continues_by_default() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![
                ok_code_response("m", "class Good {}"),
                vec![Err(LlmError::Stream("forced failure".into()))],
                ok_code_response("m", "class AlsoGood {}"),
            ]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let kp = fixture_game_context(td.path(), &[], &[]);
        let req = make_request(&["one", "two", "three"]);
        let id = service
            .submit_batch_custom_code(req, kp, artifacts.clone(), sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        // 部分成功 batch 整体 Completed
        assert_eq!(run.status, RunStatus::Succeeded);
        let res = serde_json::to_value(run.result.expect("result")).unwrap();
        assert_eq!(res["total"], 3);
        assert_eq!(res["succeeded"], 2);
        assert_eq!(res["failed"], 1);
        let items = res["items"].as_array().unwrap();
        assert!(items[1]["artifactManifestRef"].is_null());
    }

    #[tokio::test]
    async fn batch_custom_code_applies_pack_rules_before_writing_files() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![ok_code_response(
                "m",
                "public class BadRelic { public override Task BeforeCombatStart() => PlayerCmd.GainEnergy(1m, Owner); }",
            )]),
        });
        let service = RunApplicationService::new(repo, llm);
        let id = service
            .submit_batch_custom_code(
                make_request(&["BadRelic"]),
                fixture_game_context(td.path(), &[], &[]),
                artifacts.clone(),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.result.is_none());
        assert!(
            run.error_message()
                .unwrap_or_default()
                .contains("ResetEnergy")
        );
        assert!(!td.path().join("Generated/BadRelic.cs").exists());
        assert!(!artifacts.join("BadRelic/BadRelic.cs").exists());
    }

    #[tokio::test]
    async fn batch_custom_code_fail_fast_stops_on_first_error() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![
                vec![Err(LlmError::Stream("boom".into()))],
                ok_code_response("m", "never reached"),
            ]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let kp = fixture_game_context(td.path(), &[], &[]);
        let mut req = make_request(&["one", "two"]);
        req.fail_fast = true;
        let id = service
            .submit_batch_custom_code(req, kp, artifacts, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        // 第 1 个失败 + fail_fast → 不再跑第 2 个；零成功 = Failed
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.result.is_none());
        assert!(run.error_message().unwrap_or_default().contains("boom"));
    }

    #[tokio::test]
    async fn batch_custom_code_empty_items_fails() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let kp = fixture_game_context(td.path(), &[], &[]);
        let req = SubmitBatchCustomCodeRequest::default();
        let id = service
            .submit_batch_custom_code(req, kp, artifacts, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert!(
            run.error_message()
                .as_deref()
                .unwrap_or("")
                .contains("items list is empty")
        );
    }
}
