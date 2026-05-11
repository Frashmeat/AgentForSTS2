//! LLM 客户端核心抽象：trait + 公共消息类型 + 错误。

use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CompletionRequest {
    /// 用户消息序列，至少 1 条；system 单独走 `system_prompt`。
    pub messages: Vec<Message>,
    /// 可选 system prompt（Anthropic 顶层 system 字段，OpenAI 转为 role=system 消息）。
    pub system_prompt: Option<String>,
    /// 期望生成的最大 token 数。
    pub max_tokens: u32,
    /// 0.0-1.0，温度参数；None 用提供商默认。
    pub temperature: Option<f32>,
    /// 模型 ID，None 用 client 配置默认。
    pub model: Option<String>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: None,
            max_tokens: 4096,
            temperature: None,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    /// 自然结束（模型主动 stop）。
    EndTurn,
    /// 达到 max_tokens 截断。
    MaxTokens,
    /// 命中 stop sequence。
    StopSequence,
    /// 工具调用（tool use），本阶段未实现。
    ToolUse,
    /// 未知/其它。
    #[default]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionResponse {
    pub model: String,
    pub content: String,
    pub finish_reason: FinishReason,
    pub usage: Usage,
}

/// 流式响应中的一帧。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// 模型开始响应（含 model id）。
    Start { model: String },
    /// 一段文本增量（流式累积）。
    Delta { text: String },
    /// 模型结束（含 finish reason + usage）。
    End {
        finish_reason: FinishReason,
        usage: Usage,
    },
}

#[derive(Debug, Error)]
pub enum LlmError {
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("rate limited; retry after {retry_after_secs:?} seconds: {message}")]
    RateLimit {
        retry_after_secs: Option<u64>,
        message: String,
    },
    #[error("HTTP error {status}: {message}")]
    Http { status: u16, message: String },
    #[error("transport error: {0}")]
    Transport(String),
    #[error("response parse error: {0}")]
    Parse(String),
    #[error("stream interrupted: {0}")]
    Stream(String),
    #[error("request cancelled")]
    Cancelled,
    #[error("configuration error: {0}")]
    Config(String),
}

impl LlmError {
    /// 该错误是否值得重试（429 + 5xx + transport 临时故障）。
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            LlmError::RateLimit { .. }
                | LlmError::Transport(_)
                | LlmError::Http {
                    status: 500..=599,
                    ..
                }
        )
    }

    /// 若错误自带 retry-after 提示，返回建议等待秒数。
    #[must_use]
    pub fn retry_after_secs(&self) -> Option<u64> {
        match self {
            LlmError::RateLimit { retry_after_secs, .. } => *retry_after_secs,
            _ => None,
        }
    }
}

pub type CompletionStream =
    Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send + 'static>>;

#[async_trait]
pub trait LlmClient: Send + Sync {
    /// 非流式补全：阻塞直到模型完整响应或报错。
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// 流式补全：返回 chunk 流；调用方 drop 流即视为取消。
    async fn stream(&self, request: CompletionRequest) -> Result<CompletionStream, LlmError>;
}
