//! text_generate handler：纯 LLM 流式生成。
//!
//! 不写文件、不依赖工程目录。把 prompt 转给 LlmClient::stream，把 delta 实时
//! 推 sink，结束后把累积内容写入 job.result。

use std::sync::Arc;

use futures_util::StreamExt;

use super::common::{ProgressEvent, ProgressSink, finalize_with_error, transition_to_running};
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::contracts::SubmitTextGenerateRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};

pub async fn run_text_generate(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitTextGenerateRequest,
) {
    if let Err(err) = transition_to_running(&repo, &job_id, &sink).await {
        tracing::warn!(error = %err, "failed to mark job running");
        return;
    }

    let completion_request = CompletionRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: request.prompt.clone(),
        }],
        system_prompt: request.system_prompt.clone(),
        max_tokens: request.max_tokens.unwrap_or(2048),
        temperature: request.temperature,
        model: request.model.clone(),
    };

    let stream_result = llm.stream(completion_request).await;
    let mut stream = match stream_result {
        Ok(s) => s,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &err.to_string()).await;
            sink.emit(ProgressEvent {
                job_id: job_id.clone(),
                stage: "stream-start-error".into(),
                percent: None,
                message: Some(err.to_string()),
                delta: None,
            })
            .await;
            return;
        }
    };

    let mut accumulated = String::new();
    let mut model = String::new();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut finish: Option<String> = None;

    while let Some(item) = stream.next().await {
        match item {
            Ok(StreamEvent::Start { model: m }) => {
                model = m.clone();
                sink.emit(ProgressEvent {
                    job_id: job_id.clone(),
                    stage: "stream-start".into(),
                    percent: None,
                    message: Some(format!("model={m}")),
                    delta: None,
                })
                .await;
            }
            Ok(StreamEvent::Delta { text }) => {
                accumulated.push_str(&text);
                sink.emit(ProgressEvent {
                    job_id: job_id.clone(),
                    stage: "stream-delta".into(),
                    percent: None,
                    message: None,
                    delta: Some(text),
                })
                .await;
            }
            Ok(StreamEvent::End {
                finish_reason,
                usage,
            }) => {
                input_tokens = usage.input_tokens;
                output_tokens = usage.output_tokens;
                finish = Some(format!("{finish_reason:?}").to_lowercase());
            }
            Err(err) => {
                finalize_with_error(&repo, &job_id, &err.to_string()).await;
                sink.emit(ProgressEvent {
                    job_id: job_id.clone(),
                    stage: "stream-error".into(),
                    percent: None,
                    message: Some(err.to_string()),
                    delta: None,
                })
                .await;
                return;
            }
        }
    }

    let mut job = match repo.get(&job_id).await {
        Ok(j) => j,
        Err(err) => {
            tracing::warn!(error = %err, "job vanished mid-run");
            return;
        }
    };
    if matches!(job.status, JobStatus::Cancelled) {
        sink.emit(ProgressEvent {
            job_id: job_id.clone(),
            stage: "cancelled-after-stream".into(),
            percent: None,
            message: Some("job was cancelled while running".into()),
            delta: None,
        })
        .await;
        return;
    }

    job.status = JobStatus::Completed;
    job.completed_at = Some(chrono::Utc::now());
    job.result = Some(serde_json::json!({
        "model": model,
        "content": accumulated,
        "finishReason": finish,
        "usage": { "inputTokens": input_tokens, "outputTokens": output_tokens },
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "completed".into(),
        percent: Some(1.0),
        message: None,
        delta: None,
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionResponse, CompletionStream, FinishReason, LlmError, Usage,
    };
    use crate::platform::application::JobApplicationService;
    use crate::platform::domain::{Job, JobKind, JobRepository};
    use crate::platform::infra::FileJobRepository;
    use async_trait::async_trait;
    use futures_util::stream;
    use std::sync::Mutex;

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

    struct CapturingSink {
        events: tokio::sync::Mutex<Vec<ProgressEvent>>,
    }

    #[async_trait]
    impl ProgressSink for CapturingSink {
        async fn emit(&self, event: ProgressEvent) {
            self.events.lock().await.push(event);
        }
    }

    fn happy_events() -> Vec<Result<StreamEvent, LlmError>> {
        vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
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
                    input_tokens: 4,
                    output_tokens: 2,
                },
            }),
        ]
    }

    async fn wait_terminal(service: &JobApplicationService, id: &JobId) {
        for _ in 0..50 {
            let job = service.get(id).await.unwrap();
            if job.status.is_terminal() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn text_generate_flow_completes_and_persists_result() {
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn JobRepository> =
            Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(happy_events()),
        });
        let sink = Arc::new(CapturingSink {
            events: tokio::sync::Mutex::new(Vec::new()),
        });
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitTextGenerateRequest {
            prompt: "say hi".into(),
            ..Default::default()
        };
        let id = service
            .submit_text_generate(req, sink.clone())
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let result = job.result.expect("result should be set");
        assert_eq!(result["content"], "hello world");
        assert_eq!(result["model"], "test-model");
        assert_eq!(result["usage"]["outputTokens"], 2);

        let events = sink.events.lock().await;
        assert!(events.iter().any(|e| e.stage == "running"));
        assert!(events.iter().any(|e| e.stage == "stream-delta"));
        assert!(events.iter().any(|e| e.stage == "completed"));
    }

    #[tokio::test]
    async fn cancel_marks_pending_job_cancelled() {
        // 直接操纵 repo 绕开 spawn，专注测 cancel 状态机本身。
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn JobRepository> =
            Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(vec![]),
        });
        let service = JobApplicationService::new(repo.clone(), llm);

        let job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();

        service.cancel(&job.id).await.unwrap();
        let reloaded = service.get(&job.id).await.unwrap();
        assert_eq!(reloaded.status, JobStatus::Cancelled);

        let err = service.cancel(&job.id).await.unwrap_err();
        assert!(matches!(
            err,
            crate::platform::domain::JobError::Terminal { .. }
        ));
    }
}
