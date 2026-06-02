//! 文生图客户端模块。
//!
//! 当前实现：
//! - `OpenAiImagesClient`：OpenAI Images API (DALL-E 系列)。同时覆盖大多数
//!   "OpenAI 兼容代理"（new-api / one-api / litellm 等），含火山方舟通过这些
//!   代理暴露的图像端点。
//!
//! 未来如需直连火山方舟原生 SDK 协议，可加 `VolcengineClient` 实现同 trait。
//!
//! 与 LLM 模块对称：trait + factory + retry 包装。

mod client;
mod openai_images;

use std::sync::Arc;

pub use client::{
    GeneratedImage, ImageGenClient, ImageGenError, ImageGenRequest, ImageGenResponse,
};
pub use openai_images::OpenAiImagesClient;

/// 根据 `config::ImageGenConfig` 构造 ImageGenClient。
///
/// `provider` 字段（容错：trim + lowercase + hyphen-to-underscore）：
/// - `""` / `"openai"` / `"openai_images"` / `"new_api"` / `"one_api"` / `"dalle"`：OpenAI Images
/// - 其它（未来追加）→ 暂时也兜底走 OpenAI Images
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
    let default_size = if cfg.size.is_empty() {
        None
    } else {
        Some(cfg.size.clone())
    };
    // provider 全部走 OpenAI Images 兼容路径；保留 normalized 供未来分流
    let _normalized = cfg.provider.trim().to_ascii_lowercase().replace('-', "_");

    let model = if cfg.model.is_empty() {
        "dall-e-3".to_string()
    } else {
        cfg.model.clone()
    };
    let client = OpenAiImagesClient::new(cfg.api_key.clone(), model, base_url, default_size)?;
    Ok(Arc::new(client))
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
    fn factory_uses_default_model_when_empty() {
        // 不直接观察 model；只需要 factory 构造不爆
        assert!(build_from_config(&cfg("openai", "k", "", "")).is_ok());
    }
}
