//! 集成测试：JobApplicationService 全生命周期 + 文件 IO 一致性。
//!
//! 只用 pub API（不能 import 私有 modules）。覆盖：
//! 1. submit_text_generate → 流式 LLM → 结果写入 FileJobRepository → 用 list/get 验证
//! 2. submit_code_generate (custom_code) → 写 artifacts/<name>/<name>.cs + raw.md
//! 3. cancel_job → status 终态 + 结果不写
//! 4. 跨 service 实例：写一个 job 后，新实例 list 时仍能看到（验证 FileJobRepository 持久化）

use std::sync::Arc;
use std::sync::Mutex;

use ats_core::codegen::CustomCodegenRequest;
use ats_core::knowledge::KnowledgePaths;
use ats_core::llm::{
    CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmClient, LlmError,
    StreamEvent, Usage,
};
use ats_core::platform::{
    FileJobRepository, JobApplicationService, JobId, JobRepository, JobStatus, NoopProgressSink,
    ProgressSink, SubmitCodeGenerateRequest, SubmitTextGenerateRequest,
};
use async_trait::async_trait;
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

fn make_repo(td: &tempfile::TempDir) -> Arc<dyn JobRepository> {
    Arc::new(FileJobRepository::new(td.path().to_path_buf()))
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

#[tokio::test]
async fn text_generate_end_to_end_through_public_api() {
    let td = tempfile::TempDir::new().unwrap();
    let repo = make_repo(&td);
    let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
        events: Mutex::new(ok_text_events()),
    });
    let service = JobApplicationService::new(repo, llm);
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

    let job = service.get(&id).await.unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    let result = job.result.expect("result");
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
    let knowledge = KnowledgePaths::from_runtime_dir(td.path());

    let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
    let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
        events: Mutex::new(ok_code_events()),
    });
    let service = JobApplicationService::new(repo, llm);
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
            knowledge,
            artifacts.clone(),
            sink,
        )
        .await
        .unwrap();
    wait_terminal(&service, &id).await;

    let job = service.get(&id).await.unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    let cs = artifacts.join("IntegrationDemo/IntegrationDemo.cs");
    assert!(cs.exists(), "cs file should be written");
    let cs_text = std::fs::read_to_string(&cs).unwrap();
    assert!(cs_text.contains("public class IntegrationDemo"));
    let raw = artifacts.join("IntegrationDemo/raw.md");
    assert!(raw.exists());
}

#[tokio::test]
async fn cancel_pending_job_marks_cancelled() {
    // 不走 spawn 路径（race-y），直接创建 Pending → 调 cancel → 验证状态机。
    use ats_core::platform::{Job, JobKind};

    let td = tempfile::TempDir::new().unwrap();
    let repo = make_repo(&td);
    let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
        events: Mutex::new(vec![]),
    });
    let service = JobApplicationService::new(Arc::clone(&repo), llm);

    let job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
    repo.create(&job).await.unwrap();
    let id = job.id.clone();

    service.cancel(&id).await.unwrap();
    let reloaded = service.get(&id).await.unwrap();
    assert_eq!(reloaded.status, JobStatus::Cancelled);

    // 二次 cancel 应该报 Terminal（已经是终态）
    let err = service.cancel(&id).await.unwrap_err();
    assert!(err.to_string().to_lowercase().contains("terminal"));
}

#[tokio::test]
async fn jobs_persist_across_service_instances() {
    let td = tempfile::TempDir::new().unwrap();
    let history = td.path().to_path_buf();

    // 第一个 service：提交并跑完
    {
        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history.clone()));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_text_events()),
        });
        let service = JobApplicationService::new(repo, llm);
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
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
    } // service drop 后 repo 也 drop

    // 第二个 service 用同一个 history dir → 应当能 list 出之前的 job
    let repo2: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
    let llm2: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
        events: Mutex::new(vec![]),
    });
    let service2 = JobApplicationService::new(repo2, llm2);
    let summaries = service2.list().await.unwrap();
    assert_eq!(
        summaries.len(),
        1,
        "second service instance should see persisted job"
    );
    assert_eq!(summaries[0].status, JobStatus::Completed);
}
