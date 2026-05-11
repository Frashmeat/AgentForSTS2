//! batch_custom_code handler：批量跑 custom_code 生成。
//!
//! 输入是 N 个 CustomCodegenRequest，每个独立装 prompt → LLM → 解 fence → 落
//! `<artifacts_dir>/<sanitized name>/<name>.cs`。单 item 失败不影响其它，除非
//! `fail_fast = true`。每个 item 的结果（成功路径或失败原因）汇总到 job.result.items。

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use super::code_generate::{
    GenerateError, generate_and_write_code_artifact, sanitize_entity_name,
};
use super::common::{ProgressEvent, ProgressSink, finalize_with_error, transition_to_running};
use crate::codegen::{CustomCodegenRequest, PromptAssembler};
use crate::knowledge::{KnowledgePaths, SourceMode};
use crate::llm::LlmClient;
use crate::platform::contracts::SubmitBatchCustomCodeRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};

#[derive(Debug, Clone, Serialize)]
struct ItemOutcome {
    name: String,
    entity_name: String,
    success: bool,
    cs_path: Option<String>,
    extracted_chars: Option<usize>,
    raw_chars: Option<usize>,
    error: Option<String>,
}

pub async fn run_batch_custom_code(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitBatchCustomCodeRequest,
    knowledge_paths: KnowledgePaths,
    artifacts_dir: PathBuf,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }
    if request.items.is_empty() {
        finalize_with_error(&repo, &job_id, "items list is empty").await;
        return;
    }

    let assembler = PromptAssembler::built_in();
    let total = request.items.len();
    let mut outcomes: Vec<ItemOutcome> = Vec::with_capacity(total);
    let mut succeeded: u32 = 0;
    let mut failed: u32 = 0;

    for (idx, item) in request.items.into_iter().enumerate() {
        // 中途检查 cancel：若被取消则停止后续 item 但保留已完成结果到 job.result。
        if let Ok(j) = repo.get(&job_id).await {
            if matches!(j.status, JobStatus::Cancelled) {
                break;
            }
        }

        let entity_name = sanitize_entity_name(&item.name);
        sink.emit(ProgressEvent {
            job_id: job_id.clone(),
            stage: "item-start".into(),
            percent: Some((idx as f32) / (total as f32)),
            message: Some(format!("{}/{}: {}", idx + 1, total, entity_name)),
            delta: None,
        })
        .await;

        let outcome =
            process_one_item(&assembler, &llm, &sink, &job_id, &knowledge_paths, &artifacts_dir,
                &item, &entity_name)
                .await;

        let item_success = outcome.success;
        if item_success {
            succeeded += 1;
        } else {
            failed += 1;
        }
        outcomes.push(outcome);

        sink.emit(ProgressEvent {
            job_id: job_id.clone(),
            stage: if item_success {
                "item-completed".into()
            } else {
                "item-failed".into()
            },
            percent: Some(((idx + 1) as f32) / (total as f32)),
            message: Some(format!("{}/{} done", idx + 1, total)),
            delta: None,
        })
        .await;

        if !item_success && request.fail_fast {
            break;
        }
    }

    let job = match repo.get(&job_id).await {
        Ok(j) => j,
        Err(_) => return,
    };
    if matches!(job.status, JobStatus::Cancelled) {
        return;
    }
    let mut job = job;
    // 任一 item 成功就算 batch 跑通；全失败才置 Failed。
    let overall_ok = succeeded > 0;
    job.status = if overall_ok {
        JobStatus::Completed
    } else {
        JobStatus::Failed
    };
    job.completed_at = Some(chrono::Utc::now());
    if !overall_ok {
        job.error = Some(format!("all {failed} items failed"));
    }
    job.result = Some(serde_json::json!({
        "total": total,
        "succeeded": succeeded,
        "failed": failed,
        "items": outcomes,
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id,
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

async fn process_one_item(
    assembler: &PromptAssembler,
    llm: &Arc<dyn LlmClient>,
    sink: &Arc<dyn ProgressSink>,
    job_id: &JobId,
    knowledge_paths: &KnowledgePaths,
    artifacts_dir: &PathBuf,
    item: &CustomCodegenRequest,
    entity_name: &str,
) -> ItemOutcome {
    let prompt = match assembler.assemble_custom_code_prompt(item, knowledge_paths, SourceMode::Missing) {
        Ok(p) => p,
        Err(err) => {
            return ItemOutcome {
                name: item.name.clone(),
                entity_name: entity_name.to_string(),
                success: false,
                cs_path: None,
                extracted_chars: None,
                raw_chars: None,
                error: Some(format!("prompt assembly: {err}")),
            };
        }
    };
    match generate_and_write_code_artifact(
        Arc::clone(llm),
        Arc::clone(sink),
        job_id,
        prompt,
        entity_name,
        artifacts_dir,
    )
    .await
    {
        Ok(art) => ItemOutcome {
            name: item.name.clone(),
            entity_name: art.entity_name,
            success: true,
            cs_path: Some(art.cs_path.display().to_string()),
            extracted_chars: Some(art.extracted_chars),
            raw_chars: Some(art.raw_chars),
            error: None,
        },
        Err(GenerateError::Stream(msg)) => ItemOutcome {
            name: item.name.clone(),
            entity_name: entity_name.to_string(),
            success: false,
            cs_path: None,
            extracted_chars: None,
            raw_chars: None,
            error: Some(format!("stream: {msg}")),
        },
        Err(GenerateError::Write(msg)) => ItemOutcome {
            name: item.name.clone(),
            entity_name: entity_name.to_string(),
            success: false,
            cs_path: None,
            extracted_chars: None,
            raw_chars: None,
            error: Some(format!("write: {msg}")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::KnowledgePaths;
    use crate::llm::{
        CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmError,
        StreamEvent, Usage,
    };
    use crate::platform::application::JobApplicationService;
    use crate::platform::domain::JobRepository;
    use crate::platform::infra::FileJobRepository;
    use async_trait::async_trait;
    use futures_util::stream;
    use std::sync::Mutex;

    /// 按调用次数轮转预设响应：第 N 次 stream() 返回第 N 组事件。
    struct RotatingLlm {
        responses: Mutex<Vec<Vec<Result<StreamEvent, LlmError>>>>,
    }

    #[async_trait]
    impl LlmClient for RotatingLlm {
        async fn complete(
            &self,
            _: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
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
            Ok(StreamEvent::Start { model: model.into() }),
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

    async fn wait_terminal(service: &JobApplicationService, id: &JobId) {
        for _ in 0..200 {
            let job = service.get(id).await.unwrap();
            if job.status.is_terminal() {
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

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![
                ok_code_response("m1", "public class A {}"),
                ok_code_response("m1", "public class B {}"),
            ]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let kp = KnowledgePaths::from_runtime_dir(td.path());
        let req = make_request(&["AlphaHook", "BetaHook"]);
        let id = service
            .submit_batch_custom_code(req, kp, artifacts.clone(), sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.expect("result");
        assert_eq!(res["total"], 2);
        assert_eq!(res["succeeded"], 2);
        assert_eq!(res["failed"], 0);

        assert!(artifacts.join("AlphaHook/AlphaHook.cs").exists());
        assert!(artifacts.join("BetaHook/BetaHook.cs").exists());
    }

    #[tokio::test]
    async fn batch_custom_code_partial_failure_continues_by_default() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![
                ok_code_response("m", "class Good {}"),
                vec![Err(LlmError::Stream("forced failure".into()))],
                ok_code_response("m", "class AlsoGood {}"),
            ]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let kp = KnowledgePaths::from_runtime_dir(td.path());
        let req = make_request(&["one", "two", "three"]);
        let id = service
            .submit_batch_custom_code(req, kp, artifacts.clone(), sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        // 部分成功 batch 整体 Completed
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.expect("result");
        assert_eq!(res["total"], 3);
        assert_eq!(res["succeeded"], 2);
        assert_eq!(res["failed"], 1);
        let items = res["items"].as_array().unwrap();
        assert!(items[1]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("forced failure"));
    }

    #[tokio::test]
    async fn batch_custom_code_fail_fast_stops_on_first_error() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![
                vec![Err(LlmError::Stream("boom".into()))],
                ok_code_response("m", "never reached"),
            ]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let kp = KnowledgePaths::from_runtime_dir(td.path());
        let mut req = make_request(&["one", "two"]);
        req.fail_fast = true;
        let id = service
            .submit_batch_custom_code(req, kp, artifacts, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        // 第 1 个失败 + fail_fast → 不再跑第 2 个；零成功 = Failed
        assert_eq!(job.status, JobStatus::Failed);
        let res = job.result.expect("result");
        assert_eq!(res["total"], 2);
        assert_eq!(res["succeeded"], 0);
        assert_eq!(res["failed"], 1);
        assert_eq!(
            res["items"].as_array().unwrap().len(),
            1,
            "fail_fast 应在第 1 个 item 后停止"
        );
    }

    #[tokio::test]
    async fn batch_custom_code_empty_items_fails() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(RotatingLlm {
            responses: Mutex::new(vec![]),
        });
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let kp = KnowledgePaths::from_runtime_dir(td.path());
        let req = SubmitBatchCustomCodeRequest::default();
        let id = service
            .submit_batch_custom_code(req, kp, artifacts, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error
                .as_deref()
                .unwrap_or("")
                .contains("items list is empty")
        );
    }
}
