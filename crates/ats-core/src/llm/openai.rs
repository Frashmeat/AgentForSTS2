//! OpenAI Chat Completions API 客户端（非流式 + 流式）。
//!
//! 文档：<https://platform.openai.com/docs/api-reference/chat>
//!
//! 设计取向：
//! - **协议兼容**：用于 OpenAI 官方 + 任意"OpenAI 兼容"代理（new-api / one-api /
//!   litellm proxy / vLLM 等）。`base_url` 可以指向第三方端点。
//! - 与 Anthropic 不同：`system` 作为 messages 数组的第一个元素 `role=system`
//! - 流式事件：单一 `data: {...}` 帧，含 `choices[0].delta.content` 增量
//! - 结束标记：`data: [DONE]` 字面字符串
//! - 错误体：`{error: {message, type, code}}`

use std::time::Duration;

use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures_util::{StreamExt, TryStreamExt};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, RETRY_AFTER};
use serde::Deserialize;
use serde_json::Value;

use super::client::{
    CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmClient, LlmError,
    Message, MessageRole, StreamEvent, Usage,
};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const DEFAULT_TIMEOUT_SECS: u64 = 300;

pub struct OpenAiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    default_model: String,
}

impl OpenAiClient {
    /// `base_url` 留空（""）走 OpenAI 官方。指向第三方代理时填代理的 base
    /// （例 `https://your-proxy.com`，下面会拼 `/v1/chat/completions`）。
    pub fn new(
        api_key: impl Into<String>,
        default_model: impl Into<String>,
        base_url: Option<String>,
    ) -> Result<Self, LlmError> {
        let api_key: String = api_key.into();
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err(LlmError::Config("OpenAI api_key is empty".into()));
        }
        // 提前校验 Bearer 头能否构造，避免 headers() 里 .expect() panic。
        if HeaderValue::from_str(&format!("Bearer {api_key}")).is_err() {
            return Err(LlmError::Config(
                "OpenAI api_key contains characters invalid for an HTTP header".into(),
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
        let bearer = format!("Bearer {}", self.api_key);
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&bearer).expect("invalid auth header"),
        );
        h
    }

    fn build_payload(&self, request: &CompletionRequest, stream: bool) -> Value {
        let model = request
            .model
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.default_model.clone());

        let mut messages: Vec<Value> = Vec::new();
        // OpenAI 把 system 放进 messages 数组的开头，不像 Anthropic 独立字段。
        if let Some(system) = &request.system_prompt
            && !system.is_empty()
        {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system,
            }));
        }
        for m in &request.messages {
            messages.push(message_to_json(m));
        }

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens,
            "stream": stream,
        });
        if stream {
            // 让 OpenAI 在流式末尾返回 usage（chat completions 默认不带）
            body["stream_options"] = serde_json::json!({ "include_usage": true });
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
            MessageRole::System => "system",
        },
        "content": m.content,
    })
}

fn finish_reason_from_str(s: &str) -> FinishReason {
    match s {
        "stop" => FinishReason::EndTurn,
        "length" => FinishReason::MaxTokens,
        "tool_calls" | "function_call" => FinishReason::ToolUse,
        _ => FinishReason::Other,
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
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

        let parsed: OpenAiChatResponse = response
            .json()
            .await
            .map_err(|e| LlmError::Parse(format!("response json: {e}")))?;
        Ok(parsed.into_completion())
    }

    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream, LlmError> {
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
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
    // OpenAI 错误体 {error: {message, type, code}}
    let parsed: Option<OpenAiErrorBody> = serde_json::from_str(body).ok();
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

// -------- 非流式响应类型 --------

#[derive(Debug, Deserialize)]
struct OpenAiChatResponse {
    model: String,
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: OpenAiUsage,
}

impl OpenAiChatResponse {
    fn into_completion(self) -> CompletionResponse {
        let (content, finish_reason) = self
            .choices
            .into_iter()
            .next()
            .map(|c| {
                let text = c.message.content.unwrap_or_default();
                let reason = c
                    .finish_reason
                    .as_deref()
                    .map(finish_reason_from_str)
                    .unwrap_or(FinishReason::Other);
                (text, reason)
            })
            .unwrap_or_else(|| (String::new(), FinishReason::Other));
        CompletionResponse {
            model: self.model,
            content,
            finish_reason,
            usage: Usage {
                input_tokens: self.usage.prompt_tokens,
                output_tokens: self.usage.completion_tokens,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiChoiceMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoiceMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorBody {
    error: OpenAiErrorInner,
}

#[derive(Debug, Deserialize)]
struct OpenAiErrorInner {
    message: String,
}

// -------- 流式解析状态机 --------

#[derive(Debug, Default)]
struct StreamState {
    start_emitted: bool,
    model: String,
    /// 累积的 input/output tokens（OpenAI 在 stream 末尾给 usage，包含 stream_options 才有）
    input_tokens: u32,
    output_tokens: u32,
    finish_reason: FinishReason,
    /// 是否已经收到 finish_reason（非空 chunk）
    finished: bool,
}

impl StreamState {
    /// 处理一个 SSE 数据帧。
    ///
    /// - `Some(Ok(event))`：产出一个事件（Start / Delta / End）。
    /// - `Some(Err(_))`：流级错误（部分代理把错误包成 SSE 帧），作为终止性失败上抛。
    /// - `None`：该帧无需产出，继续读下一帧。
    ///
    /// OpenAI 流式协议：
    /// - 普通帧：`data: {"id":..., "model":..., "choices":[{"delta":{"content":"..."}, "finish_reason": null}], "usage": null}`
    /// - 末帧（含 stream_options 时）：`data: {"choices":[], "usage": {prompt_tokens, completion_tokens, ...}}`
    /// - 结束哨兵：`data: [DONE]`
    fn apply(&mut self, data: &str) -> Option<Result<StreamEvent, LlmError>> {
        let data = data.trim();
        if data.is_empty() {
            return None;
        }
        if data == "[DONE]" {
            // 兜底：如果之前没有 finish_reason，仍然产出 End
            return Some(Ok(StreamEvent::End {
                finish_reason: self.finish_reason,
                usage: Usage {
                    input_tokens: self.input_tokens,
                    output_tokens: self.output_tokens,
                },
            }));
        }
        let parsed: Value = serde_json::from_str(data).ok()?;

        // 错误事件（部分代理把错误包成 SSE 帧）：作为终止性 Err 上抛，
        // 让消费者把 run 标记 Failed，而非把错误文本当成功输出保存。
        if let Some(err) = parsed.get("error") {
            let message = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("stream error")
                .to_string();
            return Some(Err(LlmError::Stream(message)));
        }

        // 抓 model（首帧）
        if !self.start_emitted
            && let Some(model) = parsed.get("model").and_then(Value::as_str)
        {
            self.model = model.to_string();
            self.start_emitted = true;
            // start 帧不消费 delta；先产 Start，下次进来再产 delta
            return Some(Ok(StreamEvent::Start {
                model: self.model.clone(),
            }));
        }

        // 抓 usage（含 stream_options 时的末帧）
        if let Some(usage) = parsed.get("usage").and_then(|u| u.as_object()) {
            if let Some(pt) = usage.get("prompt_tokens").and_then(Value::as_u64) {
                self.input_tokens = pt as u32;
            }
            if let Some(ct) = usage.get("completion_tokens").and_then(Value::as_u64) {
                self.output_tokens = ct as u32;
            }
        }

        // 抓 choices[0].delta.content + finish_reason
        let choices = parsed.get("choices")?.as_array()?;
        if let Some(choice) = choices.first() {
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str)
                && !reason.is_empty()
            {
                self.finish_reason = finish_reason_from_str(reason);
                self.finished = true;
            }
            if let Some(delta) = choice.get("delta")
                && let Some(text) = delta.get("content").and_then(Value::as_str)
                && !text.is_empty()
            {
                return Some(Ok(StreamEvent::Delta {
                    text: text.to_string(),
                }));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_start_then_delta_then_done() {
        let mut state = StreamState::default();

        let chunk1 = serde_json::json!({
            "id": "chat-1",
            "model": "gpt-4o-mini",
            "choices": [{
                "index": 0,
                "delta": { "role": "assistant", "content": "Hello" },
                "finish_reason": null
            }]
        })
        .to_string();
        let ev = state.apply(&chunk1).expect("yields an item").unwrap();
        assert!(matches!(ev, StreamEvent::Start { model } if model == "gpt-4o-mini"));

        let chunk2 = serde_json::json!({
            "choices": [{
                "delta": { "content": " world" },
                "finish_reason": null
            }]
        })
        .to_string();
        let ev = state.apply(&chunk2).expect("yields an item").unwrap();
        match ev {
            StreamEvent::Delta { text } => assert_eq!(text, " world"),
            _ => panic!("expected Delta"),
        }

        // usage 末帧（stream_options.include_usage=true 时收到）
        let chunk3 = serde_json::json!({
            "choices": [{
                "delta": {},
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 12, "completion_tokens": 3 }
        })
        .to_string();
        let _ = state.apply(&chunk3); // 无 content 不产 Delta；状态机记录 finish_reason + usage

        let done = state.apply("[DONE]").expect("yields an item").unwrap();
        match done {
            StreamEvent::End {
                finish_reason,
                usage,
            } => {
                assert!(matches!(finish_reason, FinishReason::EndTurn));
                assert_eq!(usage.input_tokens, 12);
                assert_eq!(usage.output_tokens, 3);
            }
            _ => panic!("expected End"),
        }
    }

    #[test]
    fn skips_empty_data_frames() {
        let mut state = StreamState::default();
        assert!(state.apply("").is_none());
        assert!(state.apply("   ").is_none());
    }

    #[test]
    fn inline_error_event_yields_terminal_stream_error() {
        // 代理把错误包成 SSE 帧时，必须变成终止性 Err，而非被降级成 Delta 当成功保存。
        let mut state = StreamState::default();
        let err = serde_json::json!({
            "error": { "message": "context_length_exceeded", "type": "bad_request" }
        })
        .to_string();
        match state.apply(&err) {
            Some(Err(LlmError::Stream(msg))) => assert!(msg.contains("context_length_exceeded")),
            other => panic!("expected terminal Stream error, got {other:?}"),
        }
    }

    #[test]
    fn done_without_explicit_finish_still_emits_end() {
        let mut state = StreamState::default();
        let done = state.apply("[DONE]").expect("yields an item").unwrap();
        assert!(matches!(done, StreamEvent::End { .. }));
    }

    #[test]
    fn map_http_error_classifies_status_codes() {
        let auth = map_http_error(401, None, r#"{"error":{"message":"bad key"}}"#);
        assert!(matches!(auth, LlmError::Auth(msg) if msg.contains("bad key")));

        let rate = map_http_error(429, Some(7), r#"{"error":{"message":"slow down"}}"#);
        match rate {
            LlmError::RateLimit {
                retry_after_secs,
                message,
            } => {
                assert_eq!(retry_after_secs, Some(7));
                assert!(message.contains("slow down"));
            }
            other => panic!("expected RateLimit, got {other:?}"),
        }

        let server = map_http_error(500, None, "boom");
        assert!(matches!(server, LlmError::Http { status: 500, .. }));
    }

    #[test]
    fn map_http_error_falls_back_when_body_not_json() {
        let err = map_http_error(502, None, "Bad Gateway plain text");
        match err {
            LlmError::Http { message, .. } => assert!(message.contains("Bad Gateway")),
            _ => panic!("expected Http"),
        }
    }

    #[test]
    fn build_payload_puts_system_in_messages_for_openai() {
        let client = OpenAiClient::new("k", "gpt-4o", None).unwrap();
        let req = CompletionRequest {
            messages: vec![Message {
                role: MessageRole::User,
                content: "hi".into(),
            }],
            system_prompt: Some("be terse".into()),
            max_tokens: 100,
            temperature: Some(0.5),
            model: None,
        };
        let body = client.build_payload(&req, false);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "be terse");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn build_payload_stream_includes_usage_option() {
        let client = OpenAiClient::new("k", "gpt-4o", None).unwrap();
        let req = CompletionRequest {
            messages: vec![Message {
                role: MessageRole::User,
                content: "hi".into(),
            }],
            system_prompt: None,
            max_tokens: 100,
            temperature: None,
            model: None,
        };
        let body = client.build_payload(&req, true);
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn new_rejects_empty_api_key() {
        match OpenAiClient::new("", "gpt-4o", None) {
            Err(LlmError::Config(_)) => {}
            Err(other) => panic!("expected Config error, got {other:?}"),
            Ok(_) => panic!("expected error for empty api_key"),
        }
    }

    #[test]
    fn rejects_api_key_with_invalid_header_char() {
        match OpenAiClient::new("sk-with\nnewline", "gpt-4o", None) {
            Err(LlmError::Config(_)) => {}
            Err(other) => panic!("expected Config error, got {other:?}"),
            Ok(_) => panic!("expected error for invalid api_key"),
        }
    }
}
