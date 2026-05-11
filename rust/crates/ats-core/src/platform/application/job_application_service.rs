//! JobApplicationService —— Job 的 submit / get / list / cancel 入口，
//! 兼内嵌 text_generate handler。
//!
//! Handler 后续会拆出独立 trait + 注册表，本阶段先内嵌一类，证明端到端通路。

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::contracts::SubmitTextGenerateRequest;
use crate::platform::domain::{
    Job, JobError, JobId, JobKind, JobProgress, JobRepository, JobResult, JobStatus, JobSummary,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub job_id: JobId,
    pub stage: String,
    pub percent: Option<f32>,
    pub message: Option<String>,
    /// 部分流式 handler 会随事件附带增量文本（如 LLM token）。
    pub delta: Option<String>,
}

#[async_trait]
pub trait ProgressSink: Send + Sync {
    async fn emit(&self, event: ProgressEvent);
}

/// 无操作 sink，方便测试 / 不需要进度回传的场景。
pub struct NoopProgressSink;

#[async_trait]
impl ProgressSink for NoopProgressSink {
    async fn emit(&self, _: ProgressEvent) {}
}

pub struct JobApplicationService {
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
}

impl JobApplicationService {
    #[must_use]
    pub fn new(repo: Arc<dyn JobRepository>, llm: Arc<dyn LlmClient>) -> Self {
        Self { repo, llm }
    }

    pub async fn get(&self, id: &JobId) -> JobResult<Job> {
        self.repo.get(id).await
    }

    pub async fn list(&self) -> JobResult<Vec<JobSummary>> {
        self.repo.list().await
    }

    /// 标记 Cancelled 并保存。任务实际执行中如已经发起 LLM 请求，本 stage 不
    /// 中断网络层；handler 结束时会发现 status=Cancelled 而跳过最终结果覆写。
    pub async fn cancel(&self, id: &JobId) -> JobResult<()> {
        let mut job = self.repo.get(id).await?;
        if job.status.is_terminal() {
            return Err(JobError::Terminal {
                id: job.id.0.clone(),
                status: format!("{:?}", job.status),
            });
        }
        job.status = JobStatus::Cancelled;
        job.completed_at = Some(chrono::Utc::now());
        self.repo.update(&job).await
    }

    /// 提交 text_generate 任务：保存 Pending → spawn 后台 tokio 任务跑 LLM。
    /// 立刻返回 JobId；调用方通过 `get` / `list` / progress 事件观察进度。
    pub async fn submit_text_generate(
        &self,
        request: SubmitTextGenerateRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::TextGenerate, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_text_generate(repo, llm, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }
}

async fn run_text_generate(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitTextGenerateRequest,
) {
    // 转 Running 状态
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

    // 流结束后检查 job 是否已被 cancel
    let mut job = match repo.get(&job_id).await {
        Ok(j) => j,
        Err(err) => {
            tracing::warn!(error = %err, "job vanished mid-run");
            return;
        }
    };
    if matches!(job.status, JobStatus::Cancelled) {
        // 用户已取消——保留结果到 result 但不改回 Completed
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

async fn transition_to_running(
    repo: &Arc<dyn JobRepository>,
    id: &JobId,
    sink: &Arc<dyn ProgressSink>,
) -> JobResult<()> {
    let mut job = repo.get(id).await?;
    // 用户可能在 spawn 任务被调度前就 cancel 了——尊重该状态，不要回写 Running。
    if matches!(job.status, JobStatus::Cancelled) {
        return Err(JobError::Terminal {
            id: job.id.0.clone(),
            status: format!("{:?}", job.status),
        });
    }
    job.status = JobStatus::Running;
    job.started_at = Some(chrono::Utc::now());
    job.attempts += 1;
    job.progress = Some(JobProgress {
        stage: "running".into(),
        percent: Some(0.0),
        message: None,
    });
    repo.update(&job).await?;
    sink.emit(ProgressEvent {
        job_id: id.clone(),
        stage: "running".into(),
        percent: Some(0.0),
        message: None,
        delta: None,
    })
    .await;
    Ok(())
}

async fn finalize_with_error(repo: &Arc<dyn JobRepository>, id: &JobId, message: &str) {
    if let Ok(mut job) = repo.get(id).await {
        if matches!(job.status, JobStatus::Cancelled) {
            // 已取消优先于失败标记，保留 Cancelled
            return;
        }
        job.status = JobStatus::Failed;
        job.completed_at = Some(chrono::Utc::now());
        job.error = Some(message.to_string());
        let _ = repo.update(&job).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionResponse, CompletionStream, FinishReason, LlmError, Usage,
    };
    use crate::platform::infra::FileJobRepository;
    use futures_util::stream;
    use std::sync::Mutex;

    /// 单测用 LLM 客户端：按预设的事件序列返回流；非流式接口未用。
    struct ScriptedLlm {
        events: Mutex<Vec<Result<StreamEvent, LlmError>>>,
    }

    #[async_trait]
    impl LlmClient for ScriptedLlm {
        async fn complete(
            &self,
            _: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            unimplemented!()
        }
        async fn stream(
            &self,
            _: CompletionRequest,
        ) -> Result<CompletionStream, LlmError> {
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

    async fn make_service() -> (
        tempfile::TempDir,
        JobApplicationService,
        Arc<CapturingSink>,
    ) {
        let td = tempfile::TempDir::new().unwrap();
        let repo = Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm = Arc::new(ScriptedLlm {
            events: Mutex::new(vec![
                Ok(StreamEvent::Start { model: "test-model".into() }),
                Ok(StreamEvent::Delta { text: "hello ".into() }),
                Ok(StreamEvent::Delta { text: "world".into() }),
                Ok(StreamEvent::End {
                    finish_reason: FinishReason::EndTurn,
                    usage: Usage { input_tokens: 4, output_tokens: 2 },
                }),
            ]),
        });
        let sink = Arc::new(CapturingSink {
            events: tokio::sync::Mutex::new(Vec::new()),
        });
        let service = JobApplicationService::new(repo, llm);
        (td, service, sink)
    }

    #[tokio::test]
    async fn text_generate_flow_completes_and_persists_result() {
        let (_td, service, sink) = make_service().await;
        let req = SubmitTextGenerateRequest {
            prompt: "say hi".into(),
            ..Default::default()
        };
        let id = service
            .submit_text_generate(req, sink.clone())
            .await
            .unwrap();
        // spawn 后等任务跑完。最长 500ms 应足够，因 ScriptedLlm 是同步内存事件。
        for _ in 0..50 {
            let job = service.get(&id).await.unwrap();
            if job.status.is_terminal() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
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
    async fn cancel_marks_job_terminal_and_blocks_result_overwrite() {
        // 简单验证 cancel 后 status=Cancelled；不验证 race（handler 已结束才取消的场景）
        let (_td, service, sink) = make_service().await;
        let id = service
            .submit_text_generate(
                SubmitTextGenerateRequest {
                    prompt: "x".into(),
                    ..Default::default()
                },
                sink,
            )
            .await
            .unwrap();
        service.cancel(&id).await.unwrap();
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        // 再次 cancel 应当报 Terminal
        let err = service.cancel(&id).await.unwrap_err();
        assert!(matches!(err, JobError::Terminal { .. }));
    }
}
