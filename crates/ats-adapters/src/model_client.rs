use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use ats_runtime::{
    CancellationToken, FinishReason, ModelClient, ModelError, ModelMessageRole, ModelRequest,
    ModelRequestSnapshot, ModelResponse, ModelStream, ModelStreamEvent, TokenUsage,
};
use futures_util::{Stream, StreamExt, stream};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::{LlmConfig, OpenAiResponseFormat};

const RESPONSE_IDLE_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_SSE_LINE_BYTES: usize = 4 * 1024 * 1024;
const MAX_STREAM_CONTENT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Protocol {
    Anthropic,
    OpenAi,
}

#[derive(Debug, Clone)]
pub struct HttpModelClient {
    client: Client,
    protocol: Protocol,
    base_url: String,
    api_key: String,
    default_model: String,
    openai_response_format: OpenAiResponseFormat,
    queue: Arc<ModelRequestQueue>,
    retry_policy: RetryPolicy,
}

#[derive(Debug, Default)]
pub struct ModelRequestQueue {
    slot: Arc<Mutex<()>>,
}

impl ModelRequestQueue {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    async fn acquire(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<OwnedMutexGuard<()>, ModelError> {
        if cancellation.is_cancelled() {
            return Err(ModelError::Cancelled);
        }
        tokio::select! {
            biased;
            () = wait_cancelled(cancellation) => Err(ModelError::Cancelled),
            guard = Arc::clone(&self.slot).lock_owned() => {
                if cancellation.is_cancelled() {
                    Err(ModelError::Cancelled)
                } else {
                    Ok(guard)
                }
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RetryPolicy {
    initial_delay: Duration,
    followup_delay: Duration,
}

impl HttpModelClient {
    pub fn new(config: &LlmConfig) -> Result<Self, ModelError> {
        Self::new_with_queue(config, Arc::new(ModelRequestQueue::new()))
    }

    pub fn new_with_queue(
        config: &LlmConfig,
        queue: Arc<ModelRequestQueue>,
    ) -> Result<Self, ModelError> {
        if config.api_key.is_empty()
            || config.api_key.contains(['\r', '\n'])
            || config.base_url.contains(['\r', '\n'])
            || config.retry_initial_delay_ms == 0
            || config.retry_followup_delay_ms == 0
            || config.retry_initial_delay_ms > 3_600_000
            || config.retry_followup_delay_ms > 3_600_000
            || config.model_request_limits().is_err()
        {
            return Err(ModelError::Configuration);
        }
        let provider = config
            .provider
            .trim()
            .to_ascii_lowercase()
            .replace('-', "_");
        let protocol = match provider.as_str() {
            "openai" | "openai_compatible" | "new_api" | "one_api" => Protocol::OpenAi,
            _ => Protocol::Anthropic,
        };
        let (base_url, default_model) = match protocol {
            Protocol::OpenAi => (
                value_or(&config.base_url, "https://api.openai.com/v1"),
                value_or(&config.model, "gpt-4o-mini"),
            ),
            Protocol::Anthropic => (
                value_or(&config.base_url, "https://api.anthropic.com/v1"),
                value_or(&config.model, "claude-sonnet-4-6"),
            ),
        };
        reqwest::Url::parse(&base_url).map_err(|_| ModelError::Configuration)?;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| ModelError::Configuration)?;
        Ok(Self {
            client,
            protocol,
            base_url: base_url.trim_end_matches('/').into(),
            api_key: config.api_key.clone(),
            default_model,
            openai_response_format: config.openai_response_format,
            queue,
            retry_policy: RetryPolicy {
                initial_delay: Duration::from_millis(config.retry_initial_delay_ms),
                followup_delay: Duration::from_millis(config.retry_followup_delay_ms),
            },
        })
    }

    async fn complete_once(
        &self,
        snapshot: &ModelRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        if cancellation.is_cancelled() {
            return Err(ModelError::Cancelled);
        }
        let request = snapshot.request();
        let model = request
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let future = async {
            match self.protocol {
                Protocol::OpenAi => {
                    let body = openai_request_body(request, &model, self.openai_response_format);
                    let send = self
                        .client
                        .post(format!("{}/chat/completions", self.base_url))
                        .bearer_auth(&self.api_key)
                        .json(&body)
                        .send();
                    let response = tokio::time::timeout(RESPONSE_IDLE_TIMEOUT, send)
                        .await
                        .map_err(|_| ModelError::Transport)?
                        .map_err(|_| ModelError::Transport)?;
                    parse_openai_stream(response, cancellation).await
                }
                Protocol::Anthropic => {
                    let body = anthropic_request_body(request, &model);
                    let response = self
                        .client
                        .post(format!("{}/messages", self.base_url))
                        .header("x-api-key", &self.api_key)
                        .header("anthropic-version", "2023-06-01")
                        .json(&body)
                        .timeout(RESPONSE_IDLE_TIMEOUT)
                        .send()
                        .await
                        .map_err(|_| ModelError::Transport)?;
                    parse_anthropic(response).await
                }
            }
        };
        tokio::select! {
            result = future => result,
            () = wait_cancelled(cancellation) => Err(ModelError::Cancelled),
        }
    }
}

#[async_trait]
impl ModelClient for HttpModelClient {
    async fn complete(
        &self,
        request: ModelRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        request.verify().map_err(|_| ModelError::Configuration)?;
        let _queue_guard = self.queue.acquire(cancellation).await?;
        let mut attempt = 0_u32;
        loop {
            attempt = attempt.saturating_add(1);
            match self.complete_once(&request, cancellation).await {
                Err(error) if error.is_retryable() && attempt < 3 => {
                    let delay = retry_delay(&error, attempt, self.retry_policy);
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {},
                        () = wait_cancelled(cancellation) => return Err(ModelError::Cancelled),
                    }
                }
                result => return result,
            }
        }
    }

    async fn stream(
        &self,
        request: ModelRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<ModelStream, ModelError> {
        let response = self.complete(request, cancellation).await?;
        let events = vec![
            Ok(ModelStreamEvent::Start {
                model: response.model.clone(),
            }),
            Ok(ModelStreamEvent::Delta {
                text: response.content,
            }),
            Ok(ModelStreamEvent::End {
                finish_reason: response.finish_reason,
                usage: response.usage,
            }),
        ];
        Ok(Box::pin(stream::iter(events)))
    }
}

async fn parse_openai_stream(
    response: reqwest::Response,
    cancellation: &CancellationToken,
) -> Result<ModelResponse, ModelError> {
    check_status(&response)?;
    let chunks = response
        .bytes_stream()
        .map(|chunk| chunk.map_err(|_| ModelError::Transport));
    aggregate_openai_sse(chunks, cancellation).await
}

async fn aggregate_openai_sse<S, B>(
    chunks: S,
    cancellation: &CancellationToken,
) -> Result<ModelResponse, ModelError>
where
    S: Stream<Item = Result<B, ModelError>>,
    B: AsRef<[u8]>,
{
    futures_util::pin_mut!(chunks);
    let mut decoder = SseDecoder::default();
    let mut response = OpenAiStreamResponse::default();

    loop {
        let next = tokio::select! {
            biased;
            () = wait_cancelled(cancellation) => return Err(ModelError::Cancelled),
            result = tokio::time::timeout(RESPONSE_IDLE_TIMEOUT, chunks.next()) => {
                result.map_err(|_| ModelError::Transport)?
            },
        };
        let Some(chunk) = next else {
            for data in decoder.finish()? {
                if response.apply(&data)? {
                    return response.finish();
                }
            }
            return Err(ModelError::Transport);
        };
        for data in decoder.push(chunk?.as_ref())? {
            if response.apply(&data)? {
                return response.finish();
            }
        }
    }
}

#[derive(Debug, Default)]
struct SseDecoder {
    line: Vec<u8>,
    data_lines: Vec<String>,
    data_bytes: usize,
}

impl SseDecoder {
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, ModelError> {
        let mut events = Vec::new();
        for &byte in bytes {
            if byte == b'\n' {
                let mut line = std::mem::take(&mut self.line);
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.process_line(&line, &mut events)?;
            } else {
                if self.line.len() >= MAX_SSE_LINE_BYTES {
                    return Err(ModelError::InvalidResponse);
                }
                self.line.push(byte);
            }
        }
        Ok(events)
    }

    fn finish(&mut self) -> Result<Vec<String>, ModelError> {
        let mut events = Vec::new();
        if !self.line.is_empty() {
            let mut line = std::mem::take(&mut self.line);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.process_line(&line, &mut events)?;
        }
        self.dispatch(&mut events);
        Ok(events)
    }

    fn process_line(&mut self, line: &[u8], events: &mut Vec<String>) -> Result<(), ModelError> {
        if line.is_empty() {
            self.dispatch(events);
            return Ok(());
        }
        if line.first() == Some(&b':') {
            return Ok(());
        }

        let separator = line.iter().position(|byte| *byte == b':');
        let (field, mut value) = match separator {
            Some(index) => (&line[..index], &line[index + 1..]),
            None => (line, &[][..]),
        };
        if field != b"data" {
            return Ok(());
        }
        if value.first() == Some(&b' ') {
            value = &value[1..];
        }
        let value = std::str::from_utf8(value).map_err(|_| ModelError::InvalidResponse)?;
        self.data_bytes = self.data_bytes.saturating_add(value.len());
        if self.data_bytes > MAX_SSE_LINE_BYTES {
            return Err(ModelError::InvalidResponse);
        }
        self.data_lines.push(value.to_owned());
        Ok(())
    }

    fn dispatch(&mut self, events: &mut Vec<String>) {
        if !self.data_lines.is_empty() {
            events.push(self.data_lines.join("\n"));
            self.data_lines.clear();
            self.data_bytes = 0;
        }
    }
}

#[derive(Debug, Default)]
struct OpenAiStreamResponse {
    model: Option<String>,
    content: String,
    reasoning_content: String,
    finish_reason: FinishReason,
    usage: TokenUsage,
}

impl OpenAiStreamResponse {
    fn apply(&mut self, data: &str) -> Result<bool, ModelError> {
        if data == "[DONE]" {
            return Ok(true);
        }

        let value: Value = serde_json::from_str(data).map_err(|_| ModelError::InvalidResponse)?;
        if value.get("error").is_some() {
            return Err(ModelError::Rejected);
        }
        if let Some(model) = value.get("model") {
            let model = model
                .as_str()
                .filter(|model| !model.is_empty() && model.len() <= 256)
                .ok_or(ModelError::InvalidResponse)?;
            if self
                .model
                .as_deref()
                .is_some_and(|current| current != model)
            {
                return Err(ModelError::InvalidResponse);
            }
            self.model.get_or_insert_with(|| model.to_owned());
        }
        if value.get("usage").is_some_and(|usage| !usage.is_null()) {
            self.usage = usage(&value);
        }

        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .ok_or(ModelError::InvalidResponse)?;
        if let Some(choice) = choices.first() {
            let delta = choice
                .get("delta")
                .and_then(Value::as_object)
                .ok_or(ModelError::InvalidResponse)?;
            append_optional_delta(&mut self.content, delta.get("content"))?;
            append_optional_delta(&mut self.reasoning_content, delta.get("reasoning_content"))?;
            match choice.get("finish_reason") {
                None | Some(Value::Null) => {}
                Some(Value::String(reason)) => {
                    self.finish_reason = finish_reason(Some(reason));
                }
                Some(_) => return Err(ModelError::InvalidResponse),
            }
        }
        Ok(false)
    }

    fn finish(self) -> Result<ModelResponse, ModelError> {
        let content = if self.content.is_empty() {
            self.reasoning_content
        } else {
            self.content
        };
        if content.is_empty() {
            return Err(ModelError::InvalidResponse);
        }
        Ok(ModelResponse {
            model: self.model.ok_or(ModelError::InvalidResponse)?,
            content,
            finish_reason: self.finish_reason,
            usage: self.usage,
        })
    }
}

fn append_optional_delta(target: &mut String, value: Option<&Value>) -> Result<(), ModelError> {
    let Some(value) = value else {
        return Ok(());
    };
    let text = match value {
        Value::Null => return Ok(()),
        Value::String(text) => text,
        _ => return Err(ModelError::InvalidResponse),
    };
    if target.len().saturating_add(text.len()) > MAX_STREAM_CONTENT_BYTES {
        return Err(ModelError::InvalidResponse);
    }
    target.push_str(text);
    Ok(())
}

async fn parse_anthropic(response: reqwest::Response) -> Result<ModelResponse, ModelError> {
    check_status(&response)?;
    let value: Value = response
        .json()
        .await
        .map_err(|_| ModelError::InvalidResponse)?;
    let content = value
        .get("content")
        .and_then(Value::as_array)
        .ok_or(ModelError::InvalidResponse)?
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .collect::<String>();
    if content.is_empty() {
        return Err(ModelError::InvalidResponse);
    }
    Ok(ModelResponse {
        model: text(&value, "model")?.into(),
        content,
        finish_reason: finish_reason(value.get("stop_reason").and_then(Value::as_str)),
        usage: usage(&value),
    })
}

fn check_status(response: &reqwest::Response) -> Result<(), ModelError> {
    let retry_after_ms = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1_000));
    classify_status(response.status(), retry_after_ms)
}

fn classify_status(status: StatusCode, retry_after_ms: Option<u64>) -> Result<(), ModelError> {
    match status {
        status if status.is_success() => Ok(()),
        StatusCode::UNAUTHORIZED => Err(ModelError::Authentication),
        StatusCode::FORBIDDEN => Err(ModelError::Rejected),
        StatusCode::TOO_MANY_REQUESTS => Err(ModelError::RateLimited { retry_after_ms }),
        status if status.is_server_error() => Err(ModelError::Transport),
        _ => Err(ModelError::Rejected),
    }
}

fn retry_delay(error: &ModelError, attempt: u32, policy: RetryPolicy) -> Duration {
    const MIN_RETRY_AFTER_MS: u64 = 1_000;
    const MAX_RETRY_AFTER_MS: u64 = 120_000;

    if let ModelError::RateLimited {
        retry_after_ms: Some(retry_after_ms),
    } = error
    {
        return Duration::from_millis(
            (*retry_after_ms).clamp(MIN_RETRY_AFTER_MS, MAX_RETRY_AFTER_MS),
        );
    }

    match attempt {
        0 | 1 => policy.initial_delay,
        _ => policy.followup_delay,
    }
}

fn role(role: ModelMessageRole) -> &'static str {
    match role {
        ModelMessageRole::System => "system",
        ModelMessageRole::User => "user",
        ModelMessageRole::Assistant => "assistant",
    }
}

fn openai_request_body(
    request: &ModelRequest,
    model: &str,
    response_format: OpenAiResponseFormat,
) -> Value {
    let messages = request
        .messages
        .iter()
        .map(|message| {
            json!({
                "role": role(message.role),
                "content": message.content,
            })
        })
        .collect::<Vec<_>>();
    let response_format = match response_format {
        OpenAiResponseFormat::JsonSchema => json!({
            "type": "json_schema",
            "json_schema": {
                "name": "agentthespire_output",
                "strict": true,
                "schema": request.output_contract.json_schema,
            },
        }),
        OpenAiResponseFormat::JsonObject => json!({
            "type": "json_object",
        }),
    };
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": request.max_output_tokens,
        "response_format": response_format,
        "stream": true,
        "stream_options": {
            "include_usage": true,
        },
    });
    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    body
}

fn anthropic_request_body(request: &ModelRequest, model: &str) -> Value {
    let system = request
        .messages
        .iter()
        .filter(|message| message.role == ModelMessageRole::System)
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let messages = request
        .messages
        .iter()
        .filter(|message| message.role != ModelMessageRole::System)
        .map(|message| {
            json!({
                "role": role(message.role),
                "content": message.content,
            })
        })
        .collect::<Vec<_>>();
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": request.max_output_tokens,
        "output_config": {
            "format": {
                "type": "json_schema",
                "schema": request.output_contract.json_schema,
            },
        },
    });
    if !system.is_empty() {
        body["system"] = Value::String(system);
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    body
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, ModelError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 256)
        .ok_or(ModelError::InvalidResponse)
}

fn usage(value: &Value) -> TokenUsage {
    let usage = value.get("usage").unwrap_or(&Value::Null);
    TokenUsage {
        input_tokens: token_count(usage, &["input_tokens", "prompt_tokens"]),
        output_tokens: token_count(usage, &["output_tokens", "completion_tokens"]),
    }
}

fn token_count(value: &Value, keys: &[&str]) -> u32 {
    keys.iter()
        .find_map(|key| value.get(key).and_then(Value::as_u64))
        .and_then(|count| u32::try_from(count).ok())
        .unwrap_or(0)
}

fn finish_reason(value: Option<&str>) -> FinishReason {
    match value {
        Some("stop" | "end_turn") => FinishReason::EndTurn,
        Some("length" | "max_tokens") => FinishReason::MaxTokens,
        Some("stop_sequence") => FinishReason::StopSequence,
        Some("tool_calls" | "tool_use") => FinishReason::ToolUse,
        _ => FinishReason::Other,
    }
}

async fn wait_cancelled(cancellation: &CancellationToken) {
    while !cancellation.is_cancelled() {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn value_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.into()
    } else {
        value.trim().into()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use ats_kernel::{SchemaId, SchemaRef, SchemaVersion};
    use ats_runtime::{CancellationReason, ModelMessage, ModelOutputContract};

    use super::*;

    fn fixture_retry_policy() -> RetryPolicy {
        RetryPolicy {
            initial_delay: Duration::from_secs(120),
            followup_delay: Duration::from_secs(300),
        }
    }

    #[test]
    fn openai_request_enforces_the_core_output_contract() {
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["itemId"],
            "properties": {
                "itemId": {
                    "type": "string",
                    "pattern": "^[a-z][a-z0-9_-]*$"
                }
            }
        });
        let request = ModelRequest {
            messages: vec![ModelMessage {
                role: ModelMessageRole::User,
                content: "fixture".into(),
            }],
            output_contract: ModelOutputContract {
                schema: SchemaRef {
                    id: SchemaId::parse("feature.fixture-output").unwrap(),
                    version: SchemaVersion::new(1).unwrap(),
                },
                json_schema: schema.clone(),
            },
            max_output_tokens: 512,
            temperature: Some(0.25),
            model: None,
        };

        let body = openai_request_body(&request, "fixture-model", OpenAiResponseFormat::JsonSchema);

        assert_eq!(body["model"], json!("fixture-model"));
        assert_eq!(body["max_tokens"], json!(512));
        assert_eq!(body["temperature"], json!(0.25));
        assert_eq!(body["messages"][0]["role"], json!("user"));
        assert_eq!(body["stream"], json!(true));
        assert_eq!(body["stream_options"]["include_usage"], json!(true));
        assert_eq!(body["response_format"]["type"], json!("json_schema"));
        assert_eq!(
            body["response_format"]["json_schema"]["name"],
            json!("agentthespire_output")
        );
        assert_eq!(
            body["response_format"]["json_schema"]["strict"],
            json!(true)
        );
        assert_eq!(body["response_format"]["json_schema"]["schema"], schema);
    }

    #[test]
    fn openai_request_omits_an_absent_temperature() {
        let request = ModelRequest {
            messages: vec![ModelMessage {
                role: ModelMessageRole::System,
                content: "fixture".into(),
            }],
            output_contract: ModelOutputContract {
                schema: SchemaRef {
                    id: SchemaId::parse("feature.fixture-output").unwrap(),
                    version: SchemaVersion::new(1).unwrap(),
                },
                json_schema: json!({"type": "object"}),
            },
            max_output_tokens: 128,
            temperature: None,
            model: None,
        };

        let body = openai_request_body(&request, "fixture-model", OpenAiResponseFormat::JsonSchema);

        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn openai_json_object_format_is_explicit_and_keeps_the_prompt_contract() {
        let request = ModelRequest {
            messages: vec![ModelMessage {
                role: ModelMessageRole::System,
                content: "Return the exact contract shown in this prompt.".into(),
            }],
            output_contract: ModelOutputContract {
                schema: SchemaRef {
                    id: SchemaId::parse("feature.fixture-output").unwrap(),
                    version: SchemaVersion::new(1).unwrap(),
                },
                json_schema: json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["ok"],
                    "properties": { "ok": { "type": "boolean" } }
                }),
            },
            max_output_tokens: 128,
            temperature: None,
            model: None,
        };

        let body = openai_request_body(&request, "fixture-model", OpenAiResponseFormat::JsonObject);

        assert_eq!(body["response_format"], json!({ "type": "json_object" }));
        assert_eq!(
            body["messages"][0]["content"],
            json!("Return the exact contract shown in this prompt.")
        );
    }

    #[test]
    fn anthropic_request_enforces_the_core_output_contract() {
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["ok"],
            "properties": { "ok": { "type": "boolean" } }
        });
        let request = ModelRequest {
            messages: vec![
                ModelMessage {
                    role: ModelMessageRole::System,
                    content: "system one".into(),
                },
                ModelMessage {
                    role: ModelMessageRole::System,
                    content: "system two".into(),
                },
                ModelMessage {
                    role: ModelMessageRole::User,
                    content: "fixture".into(),
                },
            ],
            output_contract: ModelOutputContract {
                schema: SchemaRef {
                    id: SchemaId::parse("feature.fixture-output").unwrap(),
                    version: SchemaVersion::new(1).unwrap(),
                },
                json_schema: schema.clone(),
            },
            max_output_tokens: 256,
            temperature: Some(0.5),
            model: None,
        };

        let body = anthropic_request_body(&request, "fixture-model");

        assert_eq!(body["model"], json!("fixture-model"));
        assert_eq!(body["max_tokens"], json!(256));
        assert_eq!(body["temperature"], json!(0.5));
        assert_eq!(body["system"], json!("system one\n\nsystem two"));
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], json!("user"));
        assert_eq!(
            body["output_config"]["format"]["type"],
            json!("json_schema")
        );
        assert_eq!(body["output_config"]["format"]["schema"], schema);
        assert!(body.get("stream").is_none());
        assert!(body.get("stream_options").is_none());
    }

    fn sse_data(value: Value) -> String {
        format!("data: {}\r\n\r\n", serde_json::to_string(&value).unwrap())
    }

    #[tokio::test]
    async fn openai_sse_aggregates_split_utf8_content_finish_reason_and_usage() {
        let mut bytes = String::new();
        bytes.push_str(": keep-alive\r\n\r\n");
        bytes.push_str(&sse_data(json!({
            "model": "deepseek-v4-pro",
            "choices": [{
                "index": 0,
                "delta": {"reasoning_content": "private analysis", "content": "{\"title\":\""},
                "finish_reason": null
            }]
        })));
        bytes.push_str(&sse_data(json!({
            "model": "deepseek-v4-pro",
            "choices": [{
                "index": 0,
                "delta": {"content": "测试\"}"},
                "finish_reason": "stop"
            }]
        })));
        bytes.push_str(&sse_data(json!({
            "model": "deepseek-v4-pro",
            "choices": [],
            "usage": {"prompt_tokens": 41, "completion_tokens": 17}
        })));
        bytes.push_str("data: [DONE]\r\n\r\n");
        let bytes = bytes.into_bytes();
        let utf8_split = bytes
            .windows("测".len())
            .position(|window| window == "测".as_bytes())
            .unwrap()
            + 1;
        let chunks = vec![
            Ok(bytes[..13].to_vec()),
            Ok(bytes[13..utf8_split].to_vec()),
            Ok(bytes[utf8_split..].to_vec()),
        ];

        let response = aggregate_openai_sse(stream::iter(chunks), &CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(response.model, "deepseek-v4-pro");
        assert_eq!(response.content, "{\"title\":\"测试\"}");
        assert!(!response.content.contains("private analysis"));
        assert_eq!(response.finish_reason, FinishReason::EndTurn);
        assert_eq!(
            response.usage,
            TokenUsage {
                input_tokens: 41,
                output_tokens: 17,
            }
        );
    }

    #[tokio::test]
    async fn openai_sse_uses_reasoning_content_only_when_content_is_absent() {
        let body = format!(
            "{}data: [DONE]\n\n",
            sse_data(json!({
                "model": "compatible-reasoner",
                "choices": [{
                    "index": 0,
                    "delta": {"reasoning_content": "{\"ok\":true}"},
                    "finish_reason": "length"
                }]
            }))
        );

        let response = aggregate_openai_sse(
            stream::iter(vec![Ok(body.into_bytes())]),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(response.content, "{\"ok\":true}");
        assert_eq!(response.finish_reason, FinishReason::MaxTokens);
    }

    #[tokio::test]
    async fn openai_sse_accepts_multiline_data_and_ignores_non_data_fields() {
        let body = concat!(
            "event: message\n",
            "id: fixture\n",
            "data: {\"model\":\"fixture-model\",\n",
            "data: \"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
            "data: [DONE]\n\n"
        );

        let response = aggregate_openai_sse(
            stream::iter(vec![Ok(body.as_bytes().to_vec())]),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

        assert_eq!(response.content, "ok");
    }

    #[tokio::test]
    async fn openai_sse_rejects_malformed_empty_and_provider_error_events() {
        for (body, expected) in [
            ("data: not-json\n\n", ModelError::InvalidResponse),
            ("data: [DONE]\n\n", ModelError::InvalidResponse),
            (
                "data: {\"error\":{\"message\":\"redacted by adapter\"}}\n\n",
                ModelError::Rejected,
            ),
        ] {
            let result = aggregate_openai_sse(
                stream::iter(vec![Ok(body.as_bytes().to_vec())]),
                &CancellationToken::new(),
            )
            .await;
            assert_eq!(result, Err(expected));
        }
    }

    #[tokio::test]
    async fn openai_sse_treats_eof_before_done_as_retryable_transport_failure() {
        let body = sse_data(json!({
            "model": "fixture-model",
            "choices": [{
                "index": 0,
                "delta": {"content": "partial"},
                "finish_reason": null
            }]
        }));

        let result = aggregate_openai_sse(
            stream::iter(vec![Ok(body.into_bytes())]),
            &CancellationToken::new(),
        )
        .await;

        assert_eq!(result, Err(ModelError::Transport));
    }

    #[tokio::test]
    async fn openai_sse_observes_cancellation_while_waiting_for_a_chunk() {
        let cancellation = CancellationToken::new();
        let trigger = cancellation.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            trigger.cancel(CancellationReason::User);
        });

        let result = aggregate_openai_sse(
            stream::pending::<Result<Vec<u8>, ModelError>>(),
            &cancellation,
        )
        .await;

        assert_eq!(result, Err(ModelError::Cancelled));
    }

    #[test]
    fn retry_delay_honors_bounded_provider_guidance() {
        assert_eq!(
            retry_delay(
                &ModelError::RateLimited {
                    retry_after_ms: Some(25_000),
                },
                1,
                fixture_retry_policy(),
            ),
            Duration::from_secs(25)
        );
        assert_eq!(
            retry_delay(
                &ModelError::RateLimited {
                    retry_after_ms: Some(0),
                },
                1,
                fixture_retry_policy(),
            ),
            Duration::from_secs(1)
        );
        assert_eq!(
            retry_delay(
                &ModelError::RateLimited {
                    retry_after_ms: Some(300_000),
                },
                1,
                fixture_retry_policy(),
            ),
            Duration::from_secs(120)
        );
    }

    #[test]
    fn retry_delay_uses_spaced_fallbacks_without_provider_guidance() {
        assert_eq!(
            retry_delay(&ModelError::Transport, 1, fixture_retry_policy()),
            Duration::from_secs(120)
        );
        assert_eq!(
            retry_delay(&ModelError::Transport, 2, fixture_retry_policy()),
            Duration::from_secs(300)
        );
        assert_eq!(
            retry_delay(
                &ModelError::RateLimited {
                    retry_after_ms: None,
                },
                1,
                fixture_retry_policy(),
            ),
            Duration::from_secs(120)
        );
    }

    #[tokio::test]
    async fn model_request_queue_is_fifo_across_clients() {
        let queue = Arc::new(ModelRequestQueue::new());
        let first_token = CancellationToken::new();
        let first = queue.acquire(&first_token).await.unwrap();
        let order = Arc::new(StdMutex::new(Vec::new()));

        let second_queue = Arc::clone(&queue);
        let second_order = Arc::clone(&order);
        let second = tokio::spawn(async move {
            let token = CancellationToken::new();
            let _guard = second_queue.acquire(&token).await.unwrap();
            second_order.lock().unwrap().push(2_u8);
        });
        tokio::task::yield_now().await;

        let third_queue = Arc::clone(&queue);
        let third_order = Arc::clone(&order);
        let third = tokio::spawn(async move {
            let token = CancellationToken::new();
            let _guard = third_queue.acquire(&token).await.unwrap();
            third_order.lock().unwrap().push(3_u8);
        });
        tokio::task::yield_now().await;

        drop(first);
        second.await.unwrap();
        third.await.unwrap();
        assert_eq!(*order.lock().unwrap(), vec![2, 3]);
    }

    #[tokio::test]
    async fn queued_model_request_observes_cancellation() {
        let queue = Arc::new(ModelRequestQueue::new());
        let active = CancellationToken::new();
        let first = queue.acquire(&active).await.unwrap();
        let waiting = CancellationToken::new();
        let waiting_task = {
            let queue = Arc::clone(&queue);
            let waiting = waiting.clone();
            tokio::spawn(async move { queue.acquire(&waiting).await })
        };
        tokio::task::yield_now().await;

        assert!(waiting.cancel(CancellationReason::User));
        let result = tokio::time::timeout(Duration::from_secs(1), waiting_task)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(ModelError::Cancelled)));
        drop(first);
    }

    #[test]
    fn model_clients_can_share_one_request_queue() {
        let queue = Arc::new(ModelRequestQueue::new());
        let config = LlmConfig {
            provider: "openai".into(),
            api_key: "fixture-key".into(),
            ..LlmConfig::default()
        };
        let first = HttpModelClient::new_with_queue(&config, Arc::clone(&queue)).unwrap();
        let second = HttpModelClient::new_with_queue(&config, Arc::clone(&queue)).unwrap();

        assert!(Arc::ptr_eq(&first.queue, &second.queue));
        assert_eq!(first.retry_policy.initial_delay, Duration::from_secs(120));
        assert_eq!(first.retry_policy.followup_delay, Duration::from_secs(300));
    }

    #[test]
    fn retry_configuration_is_bounded() {
        let queue = Arc::new(ModelRequestQueue::new());
        for invalid in [0_u64, 3_600_001] {
            let config = LlmConfig {
                provider: "openai".into(),
                api_key: "fixture-key".into(),
                retry_initial_delay_ms: invalid,
                ..LlmConfig::default()
            };
            assert!(matches!(
                HttpModelClient::new_with_queue(&config, Arc::clone(&queue)),
                Err(ModelError::Configuration)
            ));
        }
    }

    #[test]
    fn http_status_classification_separates_authentication_and_rejection() {
        assert_eq!(
            classify_status(StatusCode::UNAUTHORIZED, None),
            Err(ModelError::Authentication)
        );
        assert_eq!(
            classify_status(StatusCode::FORBIDDEN, None),
            Err(ModelError::Rejected)
        );
        assert_eq!(
            classify_status(StatusCode::TOO_MANY_REQUESTS, Some(4_000)),
            Err(ModelError::RateLimited {
                retry_after_ms: Some(4_000)
            })
        );
    }
}
