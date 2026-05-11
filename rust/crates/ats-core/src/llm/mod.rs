//! LLM 客户端模块。
//!
//! 提供 `LlmClient` trait + Anthropic 实现，覆盖非流式和流式两种调用。
//! 配置从 `config::LlmConfig` 取，重试层包在 client 之上（指数退避 + jitter）。
//!
//! 后续 stage 加 OpenAI / 其它厂商时实现同一 trait 即可。

mod anthropic;
mod client;
mod retry;

pub use anthropic::AnthropicClient;
pub use client::{
    CompletionRequest, CompletionResponse, FinishReason, LlmClient, LlmError, Message,
    MessageRole, StreamEvent, Usage,
};
pub use retry::{RetryConfig, RetryingClient};
