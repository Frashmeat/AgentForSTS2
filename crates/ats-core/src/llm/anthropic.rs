//! Anthropic Messages API 客户端（非流式 + 流式）。
//!
//! 文档：<https://docs.anthropic.com/en/api/messages>
//!
//! 关键约定：
//! - `system` 单独走顶层字段，**不**作为 message 元素（与 OpenAI 不同）
//! - 流式走 SSE，事件类型 `message_start` / `content_block_delta` / `message_delta` / `message_stop`
//! - 错误用顶层 `{type:"error", error:{type, message}}` 格式

use std::time::Duration;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::{StreamExt, TryStreamExt};
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use serde::Deserialize;
use serde_json::Value;

use super::client::{
    CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmClient, LlmError,
    Message, MessageRole, StreamEvent, Usage,
};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const DEFAULT_TIMEOUT_SECS: u64 = 300;

pub struct AnthropicClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    default_model: String,
}

impl AnthropicClient {
    /// `base_url` 留空（""）使用 `https://api.anthropic.com`。
    /// `default_model` 为 request.model 缺省时的兜底（例 `claude-opus-4-1-20250805`）。
    pub fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<Self, LlmError> {
        let api_key: String = api_key.into();
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(LlmError::Config("Anthropic api_key is empty".into()));
        }
        // 提前校验能否作为 HTTP header 值，避免 headers() 里 .expect() 在用户
        // 粘贴了含换行/控制字符的 key 时 panic 掉 async 任务。
        if HeaderValue::from_str(&api_key).is_err() {
            return Err(LlmError::Config(
                "Anthropic api_key contains characters invalid for an HTTP header".into(),
            ));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|e| LlmError::Config(format!("build http client: {e}")))?;
        Ok(Self {
            http,
            base_url: base_url
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| DEFAULT_BASE_URL.into()),
            api_key,
            default_model: default_model.into(),
        })
    }

    fn headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        h.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(&self.api_key).expect("api key invalid for header"),
        );
        h.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        h
    }

    fn build_payload(&self, request: &CompletionRequest, stream: bool) -> Value {
        let model = request
            .model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.default_model.clone());
        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": request.max_tokens,
            "stream": stream,
            "messages": request.messages.iter().map(message_to_json).collect::<Vec<_>>(),
        });
        if let Some(system) = &request.system_prompt
            && !system.is_empty()
        {
            body["system"] = Value::String(system.clone());
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        body
    }
}

fn message_to_json(m: &Message) -> Value {
    serde_json::json!({
        "role": match m.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            // Anthropic 不接受 role=system 在 messages 中；调用方应把 system
            // 内容放进 CompletionRequest::system_prompt。此处仍按 user 兜底以免 400。
            MessageRole::System => "user",
        },
        "content": m.content,
    })
}

fn finish_reason_from_str(s: &str) -> FinishReason {
    match s {
        "end_turn" => FinishReason::EndTurn,
        "max_tokens" => FinishReason::MaxTokens,
        "stop_sequence" => FinishReason::StopSequence,
        "tool_use" => FinishReason::ToolUse,
        _ => FinishReason::Other,
    }
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let body = self.build_payload(&request, false);
        let response = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let body = response.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), retry_after, &body));
        }

        let parsed: AnthropicMessageResponse = response
            .json()
            .await
            .map_err(|e| LlmError::Parse(format!("response json: {e}")))?;

        Ok(parsed.into_completion())
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream, LlmError> {
        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let body = self.build_payload(&request, true);
        let response = self
            .http
            .post(&url)
            .headers(self.headers())
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Transport(e.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let body = response.text().await.unwrap_or_default();
            return Err(map_http_error(status.as_u16(), retry_after, &body));
        }

        let byte_stream = response.bytes_stream();
        let sse_stream = byte_stream.map_err(std::io::Error::other).eventsource();

        // 在 stream 处理过程中需要保留累积的 usage 与 finish_reason；
        // 用 unfold 模式把这个状态藏在闭包里。
        let state = StreamState::default();
        let mapped = futures_util::stream::unfold(
            (sse_stream, state, false),
            |(mut sse, mut state, mut done)| async move {
                if done {
                    return None;
                }
                while let Some(item) = sse.next().await {
                    match item {
                        Ok(event) => match state.apply(&event.data) {
                            Some(Ok(out)) => {
                                if matches!(out, StreamEvent::End { .. }) {
                                    done = true;
                                }
                                return Some((Ok(out), (sse, state, done)));
                            }
                            Some(Err(err)) => {
                                return Some((Err(err), (sse, state, true)));
                            }
                            // 没产出，继续读下一帧
                            None => {}
                        },
                        Err(e) => {
                            return Some((
                                Err(LlmError::Stream(e.to_string())),
                                (sse, state, true),
                            ));
                        }
                    }
                }
                None
            },
        );

        Ok(Box::pin(mapped))
    }
}

fn map_http_error(status: u16, retry_after: Option<u64>, body: &str) -> LlmError {
    // 优先尝试解析 Anthropic 错误体 {type:"error",error:{type,message}}
    let parsed: Option<AnthropicErrorBody> = serde_json::from_str(body).ok();
    let message = parsed
        .map(|p| p.error.message)
        .unwrap_or_else(|| body.chars().take(500).collect());
    match status {
        401 | 403 => LlmError::Auth(message),
        429 => LlmError::RateLimit {
            retry_after_secs: retry_after,
            message,
        },
        _ => LlmError::Http { status, message },
    }
}

// -------- Anthropic 响应类型 --------

#[derive(Debug, Deserialize)]
struct AnthropicMessageResponse {
    model: String,
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    usage: AnthropicUsage,
}

impl AnthropicMessageResponse {
    fn into_completion(self) -> CompletionResponse {
        let mut text = String::new();
        for block in &self.content {
            if block.r#type == "text"
                && let Some(t) = &block.text
            {
                text.push_str(t);
            }
        }
        CompletionResponse {
            model: self.model,
            content: text,
            finish_reason: self
                .stop_reason
                .as_deref()
                .map(finish_reason_from_str)
                .unwrap_or(FinishReason::Other),
            usage: Usage {
                input_tokens: self.usage.input_tokens,
                output_tokens: self.usage.output_tokens,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    r#type: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorBody {
    error: AnthropicErrorInner,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorInner {
    #[allow(dead_code)]
    #[serde(default, rename = "type")]
    error_type: String,
    message: String,
}

// -------- 流式解析状态机 --------

#[derive(Debug, Default)]
struct StreamState {
    /// 是否已经发出过 Start 帧（防重）
    start_emitted: bool,
    model: String,
    input_tokens: u32,
    output_tokens: u32,
    finish_reason: FinishReason,
}

impl StreamState {
    /// 处理一个 SSE 数据帧。
    ///
    /// - `Some(Ok(event))`：产出一个事件（Start / Delta / End）。
    /// - `Some(Err(_))`：流级错误（如 `overloaded_error` / 中途 rate limit），
    ///   作为终止性失败上抛，调用方据此把 run 标记 Failed。
    /// - `None`：该帧无需产出，继续读下一帧。
    fn apply(&mut self, data: &str) -> Option<Result<StreamEvent, LlmError>> {
        if data.is_empty() {
            return None;
        }
        let parsed: Value = serde_json::from_str(data).ok()?;
        let kind = parsed.get("type")?.as_str()?;

        match kind {
            "message_start" => {
                let msg = parsed.get("message")?;
                if let Some(model) = msg.get("model").and_then(Value::as_str) {
                    self.model = model.to_string();
                }
                if let Some(usage) = msg.get("usage") {
                    if let Some(it) = usage.get("input_tokens").and_then(Value::as_u64) {
                        self.input_tokens = it as u32;
                    }
                    if let Some(ot) = usage.get("output_tokens").and_then(Value::as_u64) {
                        self.output_tokens = ot as u32;
                    }
                }
                if !self.start_emitted {
                    self.start_emitted = true;
                    return Some(Ok(StreamEvent::Start {
                        model: self.model.clone(),
                    }));
                }
                None
            }
            "content_block_delta" => {
                let delta = parsed.get("delta")?;
                let delta_type = delta.get("type").and_then(Value::as_str)?;
                if delta_type != "text_delta" {
                    return None; // tool_use_delta 等本阶段不处理
                }
                let text = delta.get("text").and_then(Value::as_str)?.to_string();
                if text.is_empty() {
                    return None;
                }
                Some(Ok(StreamEvent::Delta { text }))
            }
            "message_delta" => {
                if let Some(delta) = parsed.get("delta")
                    && let Some(stop) = delta.get("stop_reason").and_then(Value::as_str)
                {
                    self.finish_reason = finish_reason_from_str(stop);
                }
                if let Some(usage) = parsed.get("usage")
                    && let Some(ot) = usage.get("output_tokens").and_then(Value::as_u64)
                {
                    self.output_tokens = ot as u32;
                }
                None
            }
            "message_stop" => Some(Ok(StreamEvent::End {
                finish_reason: self.finish_reason,
                usage: Usage {
                    input_tokens: self.input_tokens,
                    output_tokens: self.output_tokens,
                },
            })),
            "ping" => None,
            "error" => {
                let message = parsed
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("stream error event")
                    .to_string();
                // 流级错误必须作为终止性 Err 上抛：unfold 把它转成 yielded Err 并置 done，
                // 消费者的 Err 分支调用 finalize_with_error 把 run 标记 Failed。
                // 旧实现把它降级成一条 Delta 文本，导致 API 失败被当成功输出持久化。
                Some(Err(LlmError::Stream(message)))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_text_delta_to_stream_event() {
        let mut state = StreamState::default();
        // message_start
        let start = serde_json::json!({
            "type": "message_start",
            "message": {
                "model": "claude-opus-4-1",
                "usage": { "input_tokens": 10, "output_tokens": 0 }
            }
        })
        .to_string();
        let ev = state.apply(&start).unwrap().unwrap();
        assert!(matches!(ev, StreamEvent::Start { model } if model == "claude-opus-4-1"));

        // text_delta
        let delta = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "text_delta", "text": "Hello" }
        })
        .to_string();
        let ev = state.apply(&delta).unwrap().unwrap();
        assert!(matches!(ev, StreamEvent::Delta { text } if text == "Hello"));

        // message_delta 含 stop_reason
        let mdelta = serde_json::json!({
            "type": "message_delta",
            "delta": { "stop_reason": "end_turn" },
            "usage": { "output_tokens": 5 }
        })
        .to_string();
        assert!(state.apply(&mdelta).is_none()); // message_delta 不直接产出

        // message_stop
        let stop = serde_json::json!({ "type": "message_stop" }).to_string();
        let ev = state.apply(&stop).unwrap().unwrap();
        match ev {
            StreamEvent::End {
                finish_reason,
                usage,
            } => {
                assert_eq!(finish_reason, FinishReason::EndTurn);
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(usage.output_tokens, 5);
            }
            _ => panic!("expected End"),
        }
    }

    #[test]
    fn ignores_ping_and_unknown_events() {
        let mut state = StreamState::default();
        assert!(
            state
                .apply(&serde_json::json!({"type":"ping"}).to_string())
                .is_none()
        );
        assert!(
            state
                .apply(&serde_json::json!({"type":"unknown_xyz"}).to_string())
                .is_none()
        );
        assert!(state.apply("").is_none());
        assert!(state.apply("not json").is_none());
    }

    #[test]
    fn error_frame_yields_terminal_stream_error() {
        // 顶层 error 帧（overloaded_error / 中途 rate limit）必须变成终止性 Err，
        // 而不是被降级成 Delta 文本当成功输出保存。
        let mut state = StreamState::default();
        let frame = serde_json::json!({
            "type": "error",
            "error": { "type": "overloaded_error", "message": "Overloaded" }
        })
        .to_string();
        match state.apply(&frame) {
            Some(Err(LlmError::Stream(msg))) => assert!(msg.contains("Overloaded")),
            other => panic!("expected terminal Stream error, got {other:?}"),
        }
    }

    #[test]
    fn maps_429_to_rate_limit_with_retry_after() {
        let err = map_http_error(
            429,
            Some(30),
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"too fast"}}"#,
        );
        match err {
            LlmError::RateLimit {
                retry_after_secs,
                message,
            } => {
                assert_eq!(retry_after_secs, Some(30));
                assert_eq!(message, "too fast");
            }
            _ => panic!("expected RateLimit"),
        }
    }

    #[test]
    fn maps_401_to_auth() {
        let err = map_http_error(
            401,
            None,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
        );
        assert!(matches!(err, LlmError::Auth(m) if m == "invalid x-api-key"));
    }

    #[test]
    fn rejects_empty_api_key() {
        let r = AnthropicClient::new("", "claude-opus-4-1", None);
        assert!(matches!(r, Err(LlmError::Config(_))));
    }

    #[test]
    fn rejects_api_key_with_invalid_header_char() {
        // 内嵌换行的 key（粘贴事故）旧实现会在 headers() 里 panic；现在 new() 直接拒。
        let r = AnthropicClient::new("sk-with\nnewline", "claude-opus-4-1", None);
        assert!(matches!(r, Err(LlmError::Config(_))));
    }
}
