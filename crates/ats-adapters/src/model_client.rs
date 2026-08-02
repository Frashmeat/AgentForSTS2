use std::time::Duration;

use async_trait::async_trait;
use ats_runtime::{
    CancellationToken, FinishReason, ModelClient, ModelError, ModelMessageRole,
    ModelRequestSnapshot, ModelResponse, ModelStream, ModelStreamEvent, TokenUsage,
};
use futures_util::stream;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};

use crate::LlmConfig;

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
}

impl HttpModelClient {
    pub fn new(config: &LlmConfig) -> Result<Self, ModelError> {
        if config.api_key.is_empty()
            || config.api_key.contains(['\r', '\n'])
            || config.base_url.contains(['\r', '\n'])
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
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|_| ModelError::Configuration)?;
        Ok(Self {
            client,
            protocol,
            base_url: base_url.trim_end_matches('/').into(),
            api_key: config.api_key.clone(),
            default_model,
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
                    let mut body = json!({
                        "model": model,
                        "messages": messages,
                        "max_tokens": request.max_output_tokens,
                    });
                    if let Some(temperature) = request.temperature {
                        body["temperature"] = json!(temperature);
                    }
                    let response = self
                        .client
                        .post(format!("{}/chat/completions", self.base_url))
                        .bearer_auth(&self.api_key)
                        .json(&body)
                        .send()
                        .await
                        .map_err(|_| ModelError::Transport)?;
                    parse_openai(response).await
                }
                Protocol::Anthropic => {
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
                    });
                    if !system.is_empty() {
                        body["system"] = Value::String(system);
                    }
                    if let Some(temperature) = request.temperature {
                        body["temperature"] = json!(temperature);
                    }
                    let response = self
                        .client
                        .post(format!("{}/messages", self.base_url))
                        .header("x-api-key", &self.api_key)
                        .header("anthropic-version", "2023-06-01")
                        .json(&body)
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
        let mut attempt = 0_u32;
        loop {
            attempt = attempt.saturating_add(1);
            match self.complete_once(&request, cancellation).await {
                Err(error) if error.is_retryable() && attempt < 3 => {
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_millis(u64::from(attempt) * 250)) => {},
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

async fn parse_openai(response: reqwest::Response) -> Result<ModelResponse, ModelError> {
    check_status(&response)?;
    let value: Value = response
        .json()
        .await
        .map_err(|_| ModelError::InvalidResponse)?;
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .ok_or(ModelError::InvalidResponse)?;
    Ok(ModelResponse {
        model: text(&value, "model")?.into(),
        content: choice
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or(ModelError::InvalidResponse)?
            .into(),
        finish_reason: finish_reason(choice.get("finish_reason").and_then(Value::as_str)),
        usage: usage(&value),
    })
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
    match response.status() {
        status if status.is_success() => Ok(()),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ModelError::Authentication),
        StatusCode::TOO_MANY_REQUESTS => Err(ModelError::RateLimited {
            retry_after_ms: response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(|seconds| seconds.saturating_mul(1_000)),
        }),
        status if status.is_server_error() => Err(ModelError::Transport),
        _ => Err(ModelError::Rejected),
    }
}

fn role(role: ModelMessageRole) -> &'static str {
    match role {
        ModelMessageRole::System => "system",
        ModelMessageRole::User => "user",
        ModelMessageRole::Assistant => "assistant",
    }
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
