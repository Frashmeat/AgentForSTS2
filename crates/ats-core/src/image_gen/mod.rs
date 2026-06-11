//! 文生图客户端模块。
//!
//! 多协议支持：
//! - `images_api`（默认）：OpenAI Images API (DALL-E 系列) + 兼容代理
//! - `chat_completions`：通过 Chat Completions 生图（Nano Banana / Gemini 等）
//! - `auto`：从模型名推断协议
//!
//! 与 LLM 模块对称：trait + factory + retry 包装。

mod chat_image;
mod client;
mod openai_images;

use std::sync::Arc;

pub use chat_image::ChatImageClient;
pub use client::{
    GeneratedImage, ImageGenClient, ImageGenError, ImageGenRequest, ImageGenResponse,
};
pub use openai_images::OpenAiImagesClient;

/// 已解析的协议选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protocol {
    ImagesApi,
    ChatCompletions,
}

/// 将 config 中的 protocol 字符串 + model 名解析为具体协议。
///
/// - `""` / `"images_api"` / `"images-api"` → ImagesApi
/// - `"chat_completions"` / `"chat-completions"` → ChatCompletions
/// - `"auto"` → 从模型名推断
fn resolve_protocol(raw: &str, model: &str) -> Protocol {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "images_api" | "images-api" => Protocol::ImagesApi,
        "chat_completions" | "chat-completions" => Protocol::ChatCompletions,
        "auto" => infer_protocol_from_model(model),
        _ => Protocol::ImagesApi,
    }
}

/// 从模型名推断协议：chat 类关键词 → ChatCompletions，其余 → ImagesApi。
fn infer_protocol_from_model(model: &str) -> Protocol {
    let lower = model.to_ascii_lowercase();
    const CHAT_KEYWORDS: &[&str] = &["banana", "gemini", "imagen", "flux", "midjourney"];
    if CHAT_KEYWORDS.iter().any(|k| lower.contains(k)) {
        Protocol::ChatCompletions
    } else {
        Protocol::ImagesApi
    }
}

/// 根据 `config::ImageGenConfig` 构造 ImageGenClient。
///
/// # Errors
/// - `ImageGenError::Config`：api_key 为空或 reqwest 客户端构建失败
pub fn build_from_config(
    cfg: &crate::config::ImageGenConfig,
) -> Result<Arc<dyn ImageGenClient>, ImageGenError> {
    if cfg.api_key.is_empty() {
        return Err(ImageGenError::Config("image_gen.api_key is empty".into()));
    }
    let base_url = if cfg.base_url.is_empty() {
        None
    } else {
        Some(cfg.base_url.clone())
    };
    let model = if cfg.model.is_empty() {
        "dall-e-3".to_string()
    } else {
        cfg.model.clone()
    };

    match resolve_protocol(&cfg.protocol, &model) {
        Protocol::ChatCompletions => Ok(Arc::new(ChatImageClient::new(
            cfg.api_key.clone(),
            model,
            base_url,
        )?)),
        Protocol::ImagesApi => {
            let default_size = if cfg.size.is_empty() {
                None
            } else {
                Some(cfg.size.clone())
            };
            Ok(Arc::new(OpenAiImagesClient::new(
                cfg.api_key.clone(),
                model,
                base_url,
                default_size,
            )?))
        }
    }
}

#[cfg(test)]
mod factory_tests {
    use super::*;
    use crate::config::ImageGenConfig;

    fn cfg(provider: &str, key: &str, model: &str, base_url: &str) -> ImageGenConfig {
        ImageGenConfig {
            provider: provider.into(),
            model: model.into(),
            api_key: key.into(),
            base_url: base_url.into(),
            size: String::new(),
            protocol: String::new(),
        }
    }

    #[test]
    fn factory_rejects_empty_key() {
        match build_from_config(&cfg("openai_images", "", "dall-e-3", "")) {
            Err(ImageGenError::Config(_)) => {}
            Err(other) => panic!("expected Config error, got {other:?}"),
            Ok(_) => panic!("expected error for empty api_key"),
        }
    }

    #[test]
    fn factory_builds_openai_images() {
        for p in ["", "openai", "openai-images", "new-api", "dalle"] {
            assert!(
                build_from_config(&cfg(p, "k", "dall-e-3", "https://example.com")).is_ok(),
                "provider {p:?} should produce a client"
            );
        }
    }

    #[test]
    fn factory_builds_chat_completions_explicit() {
        let c = ImageGenConfig {
            provider: "openai".into(),
            model: "nano-banana-2".into(),
            api_key: "k".into(),
            base_url: "https://e-flowcode.cc".into(),
            size: "1920x1080".into(),
            protocol: "chat_completions".into(),
        };
        assert!(build_from_config(&c).is_ok());
    }

    #[test]
    fn factory_auto_infers_chat_from_banana() {
        let c = ImageGenConfig {
            provider: "openai".into(),
            model: "nano-banana-pro".into(),
            api_key: "k".into(),
            base_url: "https://e-flowcode.cc".into(),
            size: String::new(),
            protocol: "auto".into(),
        };
        assert!(build_from_config(&c).is_ok());
    }

    #[test]
    fn factory_auto_infers_chat_from_gemini() {
        let c = ImageGenConfig {
            provider: "openai".into(),
            model: "gemini-2.0-flash".into(),
            api_key: "k".into(),
            base_url: "".into(),
            size: String::new(),
            protocol: "auto".into(),
        };
        assert!(build_from_config(&c).is_ok());
    }

    #[test]
    fn factory_auto_defaults_to_images_for_dalle() {
        // DALL-E 不匹配任何 chat 关键词 → ImagesApi
        assert!(
            build_from_config(&ImageGenConfig {
                provider: "openai".into(),
                model: "dall-e-3".into(),
                api_key: "k".into(),
                base_url: "".into(),
                size: String::new(),
                protocol: "auto".into(),
            })
            .is_ok()
        );
    }

    #[test]
    fn factory_uses_default_model_when_empty() {
        assert!(build_from_config(&cfg("openai", "k", "", "")).is_ok());
    }
}
