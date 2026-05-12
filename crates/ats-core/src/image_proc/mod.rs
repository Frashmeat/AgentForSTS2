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
mod simple;

pub use chain::BgRemoverChain;
#[cfg(feature = "ml-rembg")]
pub use ml::{MlBgRemover, MlBgRemoverError};
pub use simple::{SimpleBgRemover, remove_white_background};

use async_trait::async_trait;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImageProcError {
    #[error("image decode: {0}")]
    Decode(String),
    #[error("image encode: {0}")]
    Encode(String),
    #[error("unsupported format: {0}")]
    Unsupported(String),
}

/// 图像后处理客户端：把输入 png bytes 处理成带 alpha 通道的 png bytes。
#[async_trait]
pub trait ImageProcClient: Send + Sync {
    /// 输入 PNG 字节序列 → 输出 PNG 字节序列（带 alpha）。
    async fn remove_background(&self, input_png: &[u8]) -> Result<Vec<u8>, ImageProcError>;
}
