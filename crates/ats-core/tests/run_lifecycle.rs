//! 集成测试：RunApplicationService 全生命周期 + 文件 IO 一致性。
//!
//! 只用 pub API（不能 import 私有 modules）。覆盖：
//! 1. submit_text_generate → 流式 LLM → 结果写入 FileRunRepository → 用 list/get 验证
//! 2. submit_code_generate (custom_code) → 写 Generated/<name>.cs，
//!    并发布不可变 ArtifactManifest
//! 3. cancel_run → status 终态 + 结果不写
//! 4. 跨 service 实例：写一个 run 后，新实例 list 时仍能看到（验证 FileRunRepository 持久化）

use std::sync::Arc;
use std::sync::Mutex;
use std::{collections::BTreeMap, fs, path::Path};

use async_trait::async_trait;
use ats_core::codegen::CustomCodegenRequest;
use ats_core::game_pack::{
    GamePackLoadPolicy, GamePackLoader, GamePackRegistry, TruthSnapshotStore, VerifiedGameContext,
};
use ats_core::llm::{
    CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmClient, LlmError,
    StreamEvent, Usage,
};
use ats_core::platform::{
    FileRunRepository, NoopProgressSink, ProgressSink, RunApplicationService, RunId, RunRepository,
    RunStatus, SubmitCodeGenerateRequest, SubmitTextGenerateRequest,
};
use futures_util::stream;

struct ScriptedLlm {
    events: Mutex<Vec<Result<StreamEvent, LlmError>>>,
}

#[async_trait]
impl LlmClient for ScriptedLlm {
    async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        unimplemented!()
    }
    async fn stream(&self, _: CompletionRequest) -> Result<CompletionStream, LlmError> {
        let evs: Vec<_> = self.events.lock().unwrap().drain(..).collect();
        Ok(Box::pin(stream::iter(evs)))
    }
}

fn ok_text_events() -> Vec<Result<StreamEvent, LlmError>> {
    vec![
        Ok(StreamEvent::Start {
            model: "integration-test".into(),
        }),
        Ok(StreamEvent::Delta {
            text: "hello ".into(),
        }),
        Ok(StreamEvent::Delta {
            text: "world".into(),
        }),
        Ok(StreamEvent::End {
            finish_reason: FinishReason::EndTurn,
            usage: Usage {
                input_tokens: 5,
                output_tokens: 2,
            },
        }),
    ]
}

fn ok_code_events() -> Vec<Result<StreamEvent, LlmError>> {
    vec![
        Ok(StreamEvent::Start {
            model: "integration-test".into(),
        }),
        Ok(StreamEvent::Delta {
            text: "```csharp\npublic class IntegrationDemo {}\n```".into(),
        }),
        Ok(StreamEvent::End {
            finish_reason: FinishReason::EndTurn,
            usage: Usage {
                input_tokens: 30,
                output_tokens: 10,
            },
        }),
    ]
}

fn forbidden_code_events() -> Vec<Result<StreamEvent, LlmError>> {
    vec![
        Ok(StreamEvent::Start {
            model: "integration-test".into(),
        }),
        Ok(StreamEvent::Delta {
            text: "```csharp\npublic class BadRelic { public override Task BeforeCombatStart() => PlayerCmd.GainEnergy(1m, Owner); }\n```".into(),
        }),
        Ok(StreamEvent::End {
            finish_reason: FinishReason::EndTurn,
            usage: Usage {
                input_tokens: 30,
                output_tokens: 10,
            },
        }),
    ]
}

fn make_repo(td: &tempfile::TempDir) -> Arc<dyn RunRepository> {
    Arc::new(FileRunRepository::new(td.path().to_path_buf()))
}

fn game_context(runtime_dir: &Path) -> VerifiedGameContext {
    let loader = GamePackLoader::new(GamePackLoadPolicy::new(
        ["truth_sources", "validation_rules"],
        ["dotnet_project"],
        ["sts2_code_facts"],
    ));
    let pack = loader
        .load_str(
            "fixture:sts2",
            r#"{
              "schema_version":1,
              "id":"sts2",
              "display_name":"STS2 Fixture",
              "capabilities":["truth_sources", "validation_rules"],
              "truth_sources":[{
                "id":"game",
                "kind":"local_file",
                "input_key":"game_assembly",
                "indexer":"dotnet_project",
                "provider":"sts2_code_facts"
              }],
              "validation_rules":[{
                "id":"sts2.energy.before_combat_start",
                "kind":"forbidden_call_in_method",
                "method_name":"BeforeCombatStart",
                "call_path":["PlayerCmd", "GainEnergy"],
                "message":"ResetEnergy runs afterwards"
              }]
            }"#,
        )
        .unwrap();
    let input = runtime_dir.join("fixture-game.dll");
    fs::write(&input, b"fixture-game").unwrap();
    let store = TruthSnapshotStore::new(runtime_dir, &pack);
    let mut draft = store.begin(&pack).unwrap();
    draft.stage_source("game", &input).unwrap();
    fs::write(
        draft.index_output_dir("game").unwrap().join("Fixture.cs"),
        "public class Fixture {}",
    )
    .unwrap();
    draft
        .finalize(BTreeMap::from([("fixture".into(), "1".into())]))
        .unwrap();
    let registry = GamePackRegistry::from_packs([pack]).unwrap();
    VerifiedGameContext::open_current(runtime_dir, &registry, "sts2").unwrap()
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

#[tokio::test]
async fn text_generate_end_to_end_through_public_api() {
    let td = tempfile::TempDir::new().unwrap();
    let repo = make_repo(&td);
    let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
        events: Mutex::new(ok_text_events()),
    });
    let service = RunApplicationService::new(repo, llm);
    let sink: Arc<dyn ProgressSink> = Arc::new(NoopProgressSink);

    let id = service
        .submit_text_generate(
            SubmitTextGenerateRequest {
                prompt: "say hi".into(),
                ..Default::default()
            },
            sink,
        )
        .await
        .unwrap();
    wait_terminal(&service, &id).await;

    let run = service.get(&id).await.unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
    let result = serde_json::to_value(run.result.expect("result")).unwrap();
    assert_eq!(result["content"], "hello world");
    assert_eq!(result["model"], "integration-test");

    // list 也应该看到
    let summaries = service.list().await.unwrap();
    assert!(summaries.iter().any(|s| s.id == id));
}

#[tokio::test]
async fn code_generate_writes_files_via_public_api() {
    let td = tempfile::TempDir::new().unwrap();
    let history = td.path().join("history");
    std::fs::create_dir_all(&history).unwrap();
    let artifacts = td.path().join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let context = game_context(td.path());
    let snapshot_id = context.snapshot_id().to_string();

    let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
    let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
        events: Mutex::new(ok_code_events()),
    });
    let service = RunApplicationService::new(repo, llm);
    let sink: Arc<dyn ProgressSink> = Arc::new(NoopProgressSink);

    let id = service
        .submit_code_generate(
            SubmitCodeGenerateRequest::CustomCode {
                request: CustomCodegenRequest {
                    name: "IntegrationDemo".into(),
                    description: "integration smoke".into(),
                    implementation_notes: "no-op".into(),
                    project_root: ".".into(),
                    skip_build: true,
                },
            },
            context,
            artifacts.clone(),
            sink,
        )
        .await
        .unwrap();
    wait_terminal(&service, &id).await;

    let run = service.get(&id).await.unwrap();
    assert_eq!(run.status, RunStatus::Succeeded);
    assert_eq!(run.payload["_gameContext"]["gamePackId"], "sts2");
    assert_eq!(run.payload["_gameContext"]["snapshotId"], snapshot_id);
    let generated_cs = td.path().join("Generated/IntegrationDemo.cs");
    let cs = generated_cs;
    assert!(cs.exists(), "cs file should be written");
    let cs_text = std::fs::read_to_string(&cs).unwrap();
    assert!(cs_text.contains("public class IntegrationDemo"));
    assert!(!artifacts.join("IntegrationDemo/raw.md").exists());
    assert!(!artifacts.join("IntegrationDemo/evidence.md").exists());
    let result = serde_json::to_value(run.result.expect("result")).unwrap();
    assert_eq!(result["artifactId"], "IntegrationDemo");
    assert_eq!(result["manifestSha256"].as_str().unwrap().len(), 64);
    let manifest_path = td
        .path()
        .join(result["artifactManifestRef"].as_str().unwrap());
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    assert_eq!(manifest["producingRunId"], id.0);
    let snapshot = manifest_path.parent().unwrap().join(
        manifest["files"][0]["snapshotRelativePath"]
            .as_str()
            .unwrap(),
    );
    assert_eq!(std::fs::read_to_string(snapshot).unwrap(), cs_text);
}

#[tokio::test]
async fn code_generate_applies_pack_rules_before_writing_files() {
    let td = tempfile::TempDir::new().unwrap();
    let history = td.path().join("history");
    std::fs::create_dir_all(&history).unwrap();
    let artifacts = td.path().join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();

    let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
    let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
        events: Mutex::new(forbidden_code_events()),
    });
    let service = RunApplicationService::new(repo, llm);
    let id = service
        .submit_code_generate(
            SubmitCodeGenerateRequest::CustomCode {
                request: CustomCodegenRequest {
                    name: "BadRelic".into(),
                    project_root: td.path().to_path_buf(),
                    skip_build: true,
                    ..Default::default()
                },
            },
            game_context(td.path()),
            artifacts.clone(),
            Arc::new(NoopProgressSink),
        )
        .await
        .unwrap();
    wait_terminal(&service, &id).await;

    let run = service.get(&id).await.unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    let failure = run.failure.as_ref().unwrap();
    assert_eq!(failure.code, "run.input_invalid");
    assert_eq!(failure.stage, "code_generate.output");
    assert!(failure.diagnostic.is_none());
    assert!(!serde_json::to_string(&run).unwrap().contains("ResetEnergy"));
    assert!(!td.path().join("Generated/BadRelic.cs").exists());
    assert!(!artifacts.join("BadRelic/BadRelic.cs").exists());
}

#[tokio::test]
async fn manifest_publish_failure_restores_previous_generated_file() {
    let td = tempfile::TempDir::new().unwrap();
    let history = td.path().join("history");
    fs::create_dir_all(&history).unwrap();
    let artifacts = td.path().join("artifacts");
    fs::create_dir_all(artifacts.join("IntegrationDemo")).unwrap();
    fs::write(
        artifacts.join("IntegrationDemo/runs"),
        b"force directory conflict",
    )
    .unwrap();
    let generated = td.path().join("Generated/IntegrationDemo.cs");
    fs::create_dir_all(generated.parent().unwrap()).unwrap();
    fs::write(&generated, b"public class PreviousVersion {}").unwrap();

    let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
    let service = RunApplicationService::new(
        repo,
        Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events()),
        }),
    );
    let id = service
        .submit_code_generate(
            SubmitCodeGenerateRequest::CustomCode {
                request: CustomCodegenRequest {
                    name: "IntegrationDemo".into(),
                    project_root: td.path().to_path_buf(),
                    skip_build: true,
                    ..Default::default()
                },
            },
            game_context(td.path()),
            artifacts,
            Arc::new(NoopProgressSink),
        )
        .await
        .unwrap();
    wait_terminal(&service, &id).await;

    let run = service.get(&id).await.unwrap();
    assert_eq!(run.status, RunStatus::Failed);
    assert!(run.result.is_none());
    let failure = run.failure.as_ref().unwrap();
    assert_eq!(failure.code, "core.unclassified");
    assert_eq!(failure.stage, "code_generate.publish");
    assert!(failure.diagnostic.is_some());
    assert!(!serde_json::to_string(&run)
        .unwrap()
        .contains("publish artifact manifest"));
    assert_eq!(
        fs::read_to_string(generated).unwrap(),
        "public class PreviousVersion {}"
    );
}

#[tokio::test]
async fn cancel_pending_run_marks_cancelled() {
    // 不走 spawn 路径（race-y），直接创建 Pending → 调 cancel → 验证状态机。
    use ats_core::platform::{RunKind, RunRecord};

    let td = tempfile::TempDir::new().unwrap();
    let repo = make_repo(&td);
    let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
    repo.create(&run).await.unwrap();
    let id = run.id.clone();

    repo.transition(
        &id,
        ats_core::platform::domain::RunTransition::Cancel {
            reason: ats_core::platform::domain::CancellationReason::User,
        },
    )
    .await
    .unwrap();
    let reloaded = repo.get(&id).await.unwrap();
    assert_eq!(reloaded.status, RunStatus::Cancelled);

    // 二次 cancel 应该报 Terminal（已经是终态）
    let err = repo
        .transition(
            &id,
            ats_core::platform::domain::RunTransition::Cancel {
                reason: ats_core::platform::domain::CancellationReason::User,
            },
        )
        .await
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("terminal"));
}

#[tokio::test]
async fn runs_persist_across_service_instances() {
    let td = tempfile::TempDir::new().unwrap();
    let history = td.path().to_path_buf();

    // 第一个 service：提交并跑完
    {
        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history.clone()));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_text_events()),
        });
        let service = RunApplicationService::new(repo, llm);
        let sink: Arc<dyn ProgressSink> = Arc::new(NoopProgressSink);
        let id = service
            .submit_text_generate(
                SubmitTextGenerateRequest {
                    prompt: "persist me".into(),
                    ..Default::default()
                },
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
    } // service drop 后 repo 也 drop

    // 第二个 service 用同一个 history dir → 应当能 list 出之前的 run
    let repo2: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
    let llm2: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
        events: Mutex::new(vec![]),
    });
    let service2 = RunApplicationService::new(repo2, llm2);
    let summaries = service2.list().await.unwrap();
    assert_eq!(
        summaries.len(),
        1,
        "second service instance should see persisted run"
    );
    assert_eq!(summaries[0].status, RunStatus::Succeeded);
}
