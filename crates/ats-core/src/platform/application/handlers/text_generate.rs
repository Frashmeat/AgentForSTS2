//! text_generate handler：纯 LLM 流式生成。
//!
//! 不写文件、不依赖工程目录。把 prompt 转给 LlmClient::stream，把 delta 实时
//! 推 sink，结束后把累积内容写入 run.result。

use std::sync::Arc;

use futures_util::StreamExt;

use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, finalize_if_cancelled,
    finalize_with_cancellation, finalize_with_failure, finalize_with_success,
    transition_to_running,
};
use crate::failure::FailureNormalizer;
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::application::CancellationToken;
use crate::platform::contracts::SubmitTextGenerateRequest;
use crate::platform::domain::{RunId, RunRepository, RunResult, TokenUsage};

pub async fn run_text_generate(
    repo: Arc<dyn RunRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    run_id: RunId,
    request: SubmitTextGenerateRequest,
    cancellation: CancellationToken,
) {
    let started = transition_to_running(&repo, &run_id, &sink, &cancellation).await;
    if !matches!(started, Ok(true)) {
        if let Err(err) = started {
            tracing::warn!(error = %err, "failed to mark run running");
        }
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

    let stream_result = tokio::select! {
        reason = cancellation.cancelled() => {
            finalize_with_cancellation(&repo, &run_id, &sink, reason).await;
            return;
        }
        result = llm.stream(completion_request) => result,
    };
    let mut stream = match stream_result {
        Ok(s) => s,
        Err(err) => {
            finalize_with_failure(
                &repo,
                &run_id,
                &sink,
                FailureNormalizer::llm("text_generate.stream_start", &err),
            )
            .await;
            return;
        }
    };

    let mut accumulated = String::new();
    let mut model = String::new();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut finish: Option<String> = None;
    loop {
        let item = tokio::select! {
            reason = cancellation.cancelled() => {
                drop(stream);
                finalize_with_cancellation(&repo, &run_id, &sink, reason).await;
                return;
            }
            item = stream.next() => item,
        };
        let Some(item) = item else { break };
        match item {
            Ok(StreamEvent::Start { model: m }) => {
                model = m.clone();
                sink.emit(ProgressEvent {
                    run_id: run_id.clone(),
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
                    run_id: run_id.clone(),
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
                finalize_with_failure(
                    &repo,
                    &run_id,
                    &sink,
                    FailureNormalizer::llm("text_generate.stream", &err),
                )
                .await;
                return;
            }
        }
    }

    if finalize_if_cancelled(&repo, &run_id, &sink, &cancellation).await {
        return;
    }

    let result = RunResult::TextGeneration {
        model,
        content: accumulated,
        finish_reason: finish.unwrap_or_else(|| "unknown".into()),
        usage: TokenUsage {
            input_tokens,
            output_tokens,
        },
    };
    // 原子收尾：仅当未被并发 cancel 时才落 Completed，避免覆盖用户的取消。
    match finalize_with_success(&repo, &run_id, result).await {
        FinalizeOutcome::Succeeded => {
            sink.emit(ProgressEvent {
                run_id: run_id.clone(),
                stage: "completed".into(),
                percent: Some(1.0),
                message: None,
                delta: None,
            })
            .await;
        }
        FinalizeOutcome::Cancelled => {
            sink.emit(ProgressEvent {
                run_id: run_id.clone(),
                stage: "cancelled-after-stream".into(),
                percent: None,
                message: Some("run was cancelled while running".into()),
                delta: None,
            })
            .await;
        }
        FinalizeOutcome::Vanished => {
            tracing::warn!(run_id = %run_id.0, "run vanished mid-run");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{CompletionResponse, CompletionStream, FinishReason, LlmError, Usage};
    use crate::platform::application::RunApplicationService;
    use crate::platform::domain::{RunKind, RunRecord, RunRepository, RunStatus};
    use crate::platform::infra::FileRunRepository;
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

    async fn wait_terminal(service: &RunApplicationService, id: &RunId) {
        for _ in 0..50 {
            let run = service.get(id).await.unwrap();
            if run.status.is_terminal() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn text_generate_flow_completes_and_persists_result() {
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn RunRepository> =
            Arc::new(FileRunRepository::new(td.path().to_path_buf()));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(happy_events()),
        });
        let sink = Arc::new(CapturingSink {
            events: tokio::sync::Mutex::new(Vec::new()),
        });
        let service = RunApplicationService::new(repo, llm);

        let req = SubmitTextGenerateRequest {
            prompt: "say hi".into(),
            ..Default::default()
        };
        let spawned = service
            .submit_text_generate(req, sink.clone())
            .await
            .unwrap();
        wait_terminal(&service, &spawned).await;

        let run = service.get(&spawned).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        let result = serde_json::to_value(run.result.expect("result should be set")).unwrap();
        assert_eq!(result["content"], "hello world");
        assert_eq!(result["model"], "test-model");
        assert_eq!(result["usage"]["outputTokens"], 2);

        let events = sink.events.lock().await;
        assert!(events.iter().any(|e| e.stage == "running"));
        assert!(events.iter().any(|e| e.stage == "stream-delta"));
        assert!(events.iter().any(|e| e.stage == "completed"));
    }

    #[tokio::test]
    async fn cancel_mid_stream_drops_remaining_events() {
        // 用一个慢速 stream（每帧 50ms）模拟真实 LLM；中途调 cancel，
        // handler 应在下一次 5-event-tick 检查时发现 Cancelled 并 return，
        // 剩余事件不再被处理（accumulated 远小于"完整跑完"应有的长度）。
        use futures_util::StreamExt as _;
        use std::time::Duration;

        // 构造 100 个 delta + 1 个 End，每帧 yield 前 sleep 30ms
        let mut events: Vec<Result<StreamEvent, LlmError>> = vec![Ok(StreamEvent::Start {
            model: "slow-model".into(),
        })];
        for _ in 0..100 {
            events.push(Ok(StreamEvent::Delta { text: "x".into() }));
        }
        events.push(Ok(StreamEvent::End {
            finish_reason: FinishReason::EndTurn,
            usage: Usage {
                input_tokens: 1,
                output_tokens: 100,
            },
        }));

        struct SlowLlm {
            events: Mutex<Option<Vec<Result<StreamEvent, LlmError>>>>,
        }
        #[async_trait]
        impl LlmClient for SlowLlm {
            async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
                unimplemented!()
            }
            async fn stream(&self, _: CompletionRequest) -> Result<CompletionStream, LlmError> {
                let evs = self.events.lock().unwrap().take().unwrap_or_default();
                let s = stream::iter(evs).then(|item| async move {
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    item
                });
                Ok(Box::pin(s))
            }
        }

        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn RunRepository> =
            Arc::new(FileRunRepository::new(td.path().to_path_buf()));
        let llm: Arc<dyn LlmClient> = Arc::new(SlowLlm {
            events: Mutex::new(Some(events)),
        });
        let sink = Arc::new(CapturingSink {
            events: tokio::sync::Mutex::new(Vec::new()),
        });
        let service = RunApplicationService::new(repo.clone(), llm);

        let spawned = service
            .submit_text_generate(
                SubmitTextGenerateRequest {
                    prompt: "long output".into(),
                    ..Default::default()
                },
                sink.clone(),
            )
            .await
            .unwrap();

        // 让 handler 处理几帧后再 cancel
        tokio::time::sleep(Duration::from_millis(150)).await;
        spawned
            .cancellation
            .cancel(crate::platform::domain::CancellationReason::User);

        // cancel 是同步写 status；handler 是异步轮询。等 handler 看到 Cancelled
        // → 下一个 tick%5==0 时 break → emit "cancelled-mid-stream"。轮询 sink
        // 直到该事件出现，最长 2s（实际 30ms*5 ≈ 150ms 就该到）。
        let mut cancelled_emitted = false;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if sink
                .events
                .lock()
                .await
                .iter()
                .any(|e| e.stage == "cancelled-mid-stream")
            {
                cancelled_emitted = true;
                break;
            }
        }

        let run = service.get(&spawned).await.unwrap();
        assert_eq!(run.status, RunStatus::Cancelled);
        assert!(
            run.result.is_none(),
            "result should NOT be written on cancel"
        );
        assert!(
            cancelled_emitted,
            "expected cancelled-mid-stream event within 2s; stages={:?}",
            sink.events
                .lock()
                .await
                .iter()
                .map(|e| e.stage.clone())
                .collect::<Vec<_>>()
        );
        let delta_count = sink
            .events
            .lock()
            .await
            .iter()
            .filter(|e| e.stage == "stream-delta")
            .count();
        assert!(
            delta_count < 100,
            "expected fewer than 100 deltas (got {delta_count}); stream should be cut short"
        );
    }

    #[tokio::test]
    async fn cancel_marks_pending_run_cancelled() {
        // 直接操纵 repo 绕开 spawn，专注测 cancel 状态机本身。
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn RunRepository> =
            Arc::new(FileRunRepository::new(td.path().to_path_buf()));
        let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        repo.create(&run).await.unwrap();

        repo.transition(
            &run.id,
            crate::platform::domain::RunTransition::Cancel {
                reason: crate::platform::domain::CancellationReason::User,
            },
        )
        .await
        .unwrap();
        let reloaded = repo.get(&run.id).await.unwrap();
        assert_eq!(reloaded.status, RunStatus::Cancelled);

        let err = repo
            .transition(
                &run.id,
                crate::platform::domain::RunTransition::Cancel {
                    reason: crate::platform::domain::CancellationReason::User,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            crate::platform::domain::RunError::InvalidTransition { .. }
        ));
    }
}
