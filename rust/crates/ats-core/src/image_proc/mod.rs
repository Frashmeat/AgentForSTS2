//! 图像后处理：背景去除 + alpha 通道。
//!
//! **当前阶段**：lightweight 启发式（luminance + RGB 阈值），适合 AI 文生图常见的
//! 纯白 / 浅灰背景。处理速度 ~10ms/1024x1024。
//!
//! **后续阶段**：rembg via ort（u2net.onnx）。Q10 决议是内嵌模型 170MB；
//! 真实 ML 实现是 Stage 4 后续工作。trait 设计已经留好扩展位 ——
//! 加 `OrtRembgClient` 实现同 trait 即可无侵入升级。

mod simple;

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
