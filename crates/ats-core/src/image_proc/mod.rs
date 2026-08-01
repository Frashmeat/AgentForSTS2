//! 图像后处理：背景去除 + alpha 通道。
//!
//! **两条实现**：
//! - `SimpleBgRemover`（always-on）：启发式 luminance + RGB 阈值，~10ms/1024px，
//!   只对纯白 / 浅灰背景有效；
//! - `MlBgRemover`（feature `ml-rembg`）：ort + u2netp 真做语义分割，
//!   ~200-400ms CPU，能处理复杂 / 深色背景。
//!
//! 调用方建议直接用 `BgRemoverChain`：自动 ML→Simple 回退，不感知 feature 状态。
//! 模型权重缓存在 `image_proc::cache::ModelSpec` + `ensure_model`，落到
//! `<app_data>/models/u2netp.onnx`。

pub mod cache;
mod chain;
#[cfg(feature = "ml-rembg")]
mod ml;
mod quality;
mod simple;
mod variants;

pub use chain::BgRemoverChain;
#[cfg(feature = "ml-rembg")]
pub use ml::{MlBgRemover, MlBgRemoverError, init_ort_from_dylib};
pub use quality::{ImageQualityIssue, ImageQualityReport, ImageQualitySpec, analyze_png_quality};
pub use simple::{SimpleBgRemover, remove_white_background};
pub use variants::{
    DerivedImageVariant, ImageVariantRole, ImageVariantSpec, ImageVariantTransform,
    derive_png_variants,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::build_info::BuildInfo;
use crate::cancellation::CancellationToken;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageProcessor {
    Simple,
    MlU2netp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ImageProcFallbackReason {
    NotReady,
    Model,
    Runtime,
    Decode,
    Encode,
    Unsupported,
    Quality,
    Unclassified,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageProcFallback {
    pub from: ImageProcessor,
    pub reason: ImageProcFallbackReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageProcessingProvenance {
    pub processor: ImageProcessor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<ImageProcFallback>,
    pub build: BuildInfo,
}

impl ImageProcessingProvenance {
    #[must_use]
    pub fn simple() -> Self {
        Self {
            processor: ImageProcessor::Simple,
            model_sha256: None,
            runtime_version: None,
            fallback: None,
            build: BuildInfo::current(),
        }
    }

    #[must_use]
    pub fn ml_u2netp(model_sha256: String, runtime_version: String) -> Self {
        Self {
            processor: ImageProcessor::MlU2netp,
            model_sha256: Some(model_sha256),
            runtime_version: Some(runtime_version),
            fallback: None,
            build: BuildInfo::current(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImageProcOutcome {
    pub png: Vec<u8>,
    pub provenance: ImageProcessingProvenance,
}

#[derive(Debug, Error)]
pub enum ImageProcError {
    #[error("image processing was cancelled")]
    Cancelled,
    #[error("image processor is not ready: {0}")]
    NotReady(String),
    #[error("image model failed: {0}")]
    Model(String),
    #[error("image runtime failed: {0}")]
    Runtime(String),
    #[error("image decode: {0}")]
    Decode(String),
    #[error("image encode: {0}")]
    Encode(String),
    #[error("unsupported format: {0}")]
    Unsupported(String),
    #[error("image quality: {0}")]
    Quality(String),
}

/// 图像后处理客户端：把输入 png bytes 处理成带 alpha 通道的 png bytes。
#[async_trait]
pub trait ImageProcClient: Send + Sync {
    fn processor(&self) -> ImageProcessor;

    /// 输入 PNG 字节序列 → 输出 PNG 字节序列（带 alpha）。
    async fn remove_background(
        &self,
        input_png: &[u8],
        cancellation: &CancellationToken,
    ) -> Result<ImageProcOutcome, ImageProcError>;
}

impl ImageProcFallbackReason {
    #[must_use]
    fn from_error(error: &ImageProcError) -> Self {
        match error {
            ImageProcError::NotReady(_) => Self::NotReady,
            ImageProcError::Model(_) => Self::Model,
            ImageProcError::Runtime(_) => Self::Runtime,
            ImageProcError::Decode(_) => Self::Decode,
            ImageProcError::Encode(_) => Self::Encode,
            ImageProcError::Unsupported(_) => Self::Unsupported,
            ImageProcError::Quality(_) => Self::Quality,
            ImageProcError::Cancelled => Self::Unclassified,
        }
    }
}

#[cfg(test)]
mod provenance_tests {
    use super::*;

    #[test]
    fn ml_provenance_keeps_verified_model_runtime_and_build_identity() {
        let model_sha256 = "a".repeat(64);
        let provenance =
            ImageProcessingProvenance::ml_u2netp(model_sha256.clone(), "1.22.0".into());

        assert_eq!(provenance.processor, ImageProcessor::MlU2netp);
        assert_eq!(
            provenance.model_sha256.as_deref(),
            Some(model_sha256.as_str())
        );
        assert_eq!(provenance.runtime_version.as_deref(), Some("1.22.0"));
        assert!(provenance.fallback.is_none());
        assert!(provenance.build.is_consistent());
    }
}
