//! 重试装饰器：指数退避 + jitter，遵从服务端 retry-after 提示。
//!
//! 只重试**非流式** complete()——流式响应中途出错难以无副作用重试，由调用方决定。

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::client::{CompletionRequest, CompletionResponse, CompletionStream, LlmClient, LlmError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub backoff_multiplier: f32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            initial_backoff_ms: 500,
            max_backoff_ms: 10_000,
            backoff_multiplier: 2.0,
        }
    }
}

pub struct RetryingClient {
    inner: Arc<dyn LlmClient>,
    config: RetryConfig,
}

impl RetryingClient {
    pub fn new(inner: Arc<dyn LlmClient>, config: RetryConfig) -> Self {
        Self { inner, config }
    }
}

#[async_trait]
impl LlmClient for RetryingClient {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            match self.inner.complete(request.clone()).await {
                Ok(resp) => return Ok(resp),
                Err(err) => {
                    if attempt >= self.config.max_attempts || !err.is_retryable() {
                        return Err(err);
                    }
                    let wait = backoff_duration(&self.config, attempt, err.retry_after_secs());
                    tracing::warn!(
                        attempt,
                        wait_ms = wait.as_millis() as u64,
                        error = %err,
                        "llm complete retry"
                    );
                    tokio::time::sleep(wait).await;
                }
            }
        }
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream, LlmError> {
        // 流式不在本层重试——首次连接失败仍可被调用者重新发起。
        self.inner.stream(request).await
    }
}

fn backoff_duration(
    config: &RetryConfig,
    attempt: u32,
    server_retry_after_secs: Option<u64>,
) -> Duration {
    if let Some(secs) = server_retry_after_secs {
        return Duration::from_secs(secs.min(60));
    }
    let exponent = attempt.saturating_sub(1) as i32;
    let raw = (config.initial_backoff_ms as f32) * config.backoff_multiplier.powi(exponent);
    let capped = (raw as u64).min(config.max_backoff_ms);
    // 简单 jitter：在 [50%, 100%] 区间随机
    let jittered = capped / 2 + pseudo_random_in_range(capped / 2);
    Duration::from_millis(jittered)
}

/// 极简伪随机（基于纳秒时间），避免引入 `rand` 依赖。jitter 用够了。
fn pseudo_random_in_range(max: u64) -> u64 {
    if max == 0 {
        return 0;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    nanos % max
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::client::{FinishReason, Usage};
    use std::sync::Mutex;

    struct ScriptedClient {
        responses: Mutex<Vec<Result<CompletionResponse, LlmError>>>,
        calls: Mutex<u32>,
    }

    impl ScriptedClient {
        fn new(responses: Vec<Result<CompletionResponse, LlmError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                calls: Mutex::new(0),
            }
        }
        fn call_count(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    #[async_trait]
    impl LlmClient for ScriptedClient {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            *self.calls.lock().unwrap() += 1;
            self.responses.lock().unwrap().remove(0)
        }
        async fn stream(&self, _: CompletionRequest) -> Result<CompletionStream, LlmError> {
            unimplemented!()
        }
    }

    fn ok_response() -> Result<CompletionResponse, LlmError> {
        Ok(CompletionResponse {
            model: "test".into(),
            content: "ok".into(),
            finish_reason: FinishReason::EndTurn,
            usage: Usage::default(),
        })
    }

    fn fast_retry_config() -> RetryConfig {
        RetryConfig {
            max_attempts: 3,
            initial_backoff_ms: 1,
            max_backoff_ms: 4,
            backoff_multiplier: 2.0,
        }
    }

    #[tokio::test]
    async fn succeeds_first_try_no_retry() {
        let inner = Arc::new(ScriptedClient::new(vec![ok_response()]));
        let client = RetryingClient::new(inner.clone(), fast_retry_config());
        let r = client.complete(CompletionRequest::default()).await;
        assert!(r.is_ok());
        assert_eq!(inner.call_count(), 1);
    }

    #[tokio::test]
    async fn retries_on_rate_limit_then_succeeds() {
        let inner = Arc::new(ScriptedClient::new(vec![
            Err(LlmError::RateLimit {
                retry_after_secs: None,
                message: "throttled".into(),
            }),
            ok_response(),
        ]));
        let client = RetryingClient::new(inner.clone(), fast_retry_config());
        let r = client.complete(CompletionRequest::default()).await;
        assert!(r.is_ok());
        assert_eq!(inner.call_count(), 2);
    }

    #[tokio::test]
    async fn does_not_retry_auth_error() {
        let inner = Arc::new(ScriptedClient::new(vec![Err(LlmError::Auth(
            "bad key".into(),
        ))]));
        let client = RetryingClient::new(inner.clone(), fast_retry_config());
        let r = client.complete(CompletionRequest::default()).await;
        assert!(matches!(r, Err(LlmError::Auth(_))));
        assert_eq!(inner.call_count(), 1);
    }

    #[tokio::test]
    async fn gives_up_after_max_attempts() {
        let inner = Arc::new(ScriptedClient::new(vec![
            Err(LlmError::Transport("boom".into())),
            Err(LlmError::Transport("boom".into())),
            Err(LlmError::Transport("boom".into())),
        ]));
        let client = RetryingClient::new(inner.clone(), fast_retry_config());
        let r = client.complete(CompletionRequest::default()).await;
        assert!(matches!(r, Err(LlmError::Transport(_))));
        assert_eq!(inner.call_count(), 3);
    }
}
