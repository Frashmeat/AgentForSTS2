//! LLM 客户端模块。
//!
//! 提供 `LlmClient` trait + 多个具体实现：
//! - `AnthropicClient`：Anthropic Messages API（含流式）
//! - `OpenAiClient`：OpenAI Chat Completions API（含流式）——同时兼容 new-api /
//!   one-api / litellm proxy / vLLM 等"OpenAI 协议代理"，让用户用任意第三方
//!   endpoint 测试
//!
//! 配置从 `config::LlmConfig` 取（`provider` 字段决定走哪个 client）。重试层
//! 包在 client 之上（指数退避 + jitter）。

mod anthropic;
mod client;
mod openai;
mod retry;

use std::sync::Arc;

pub use anthropic::AnthropicClient;
pub use client::{
    CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmClient, LlmError,
    Message, MessageRole, StreamEvent, Usage,
};
pub use openai::OpenAiClient;
pub use retry::{RetryConfig, RetryingClient};

/// 根据 `config::LlmConfig` 构造合适的 LlmClient（含 retry 层）。
///
/// `provider` 字段（容错：trim + lowercase + `-` 视同 `_`）：
/// - `"openai"` / `"openai_compatible"` / `"new_api"` / `"one_api"`：OpenAI Chat Completions
/// - `""` / `"anthropic"` / `"claude"` / 其它：Anthropic Messages（默认）
///
/// 默认 model：Anthropic → `claude-opus-4-1-20250805`；OpenAI → `gpt-4o-mini`。
///
/// # Errors
/// - `LlmError::Config`：api_key 为空 或 reqwest 客户端构建失败
pub fn build_from_config(
    cfg: &crate::config::LlmConfig,
) -> Result<Arc<dyn LlmClient>, LlmError> {
    if cfg.api_key.is_empty() {
        return Err(LlmError::Config("llm.api_key is empty".into()));
    }
    let base_url = if cfg.base_url.is_empty() {
        None
    } else {
        Some(cfg.base_url.clone())
    };
    let normalized = cfg
        .provider
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_");

    let raw: Arc<dyn LlmClient> = match normalized.as_str() {
        "openai" | "openai_compatible" | "new_api" | "one_api" => {
            let model = if cfg.model.is_empty() {
                "gpt-4o-mini".to_string()
            } else {
                cfg.model.clone()
            };
            Arc::new(OpenAiClient::new(cfg.api_key.clone(), model, base_url)?)
        }
        // 默认 Anthropic（含 Anthropic 兼容代理）
        _ => {
            let model = if cfg.model.is_empty() {
                "claude-opus-4-1-20250805".to_string()
            } else {
                cfg.model.clone()
            };
            Arc::new(AnthropicClient::new(cfg.api_key.clone(), model, base_url)?)
        }
    };
    Ok(Arc::new(RetryingClient::new(raw, RetryConfig::default())))
}

#[cfg(test)]
mod factory_tests {
    use super::*;
    use crate::config::LlmConfig;

    fn cfg(provider: &str, key: &str, model: &str, base_url: &str) -> LlmConfig {
        LlmConfig {
            mode: String::new(),
            agent_backend: String::new(),
            provider: provider.into(),
            model: model.into(),
            api_key: key.into(),
            base_url: base_url.into(),
        }
    }

    #[test]
    fn factory_rejects_empty_key() {
        match build_from_config(&cfg("openai", "", "gpt-4o", "")) {
            Err(LlmError::Config(_)) => {}
            Err(other) => panic!("expected Config error, got {other:?}"),
            Ok(_) => panic!("expected error for empty api_key"),
        }
    }

    #[test]
    fn factory_builds_openai_for_known_aliases() {
        for p in ["openai", "OpenAI", "openai-compatible", "new-api", "one_api"] {
            assert!(
                build_from_config(&cfg(p, "k", "gpt-4o", "https://example.com")).is_ok(),
                "provider {p:?} should produce a client"
            );
        }
    }

    #[test]
    fn factory_defaults_to_anthropic_when_provider_blank() {
        assert!(
            build_from_config(&cfg("", "k", "claude-sonnet-4-6", "https://example.com")).is_ok()
        );
    }

    #[test]
    fn factory_anthropic_for_explicit_provider_value() {
        assert!(build_from_config(&cfg("anthropic", "k", "", "")).is_ok());
    }
}
