//! ImageGenClient trait + 通用类型。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenRequest {
    /// 文生图的自然语言描述。
    pub prompt: String,
    /// 出图数量。OpenAI Images 上限 10（dall-e-2）/ 1（dall-e-3）。
    #[serde(default = "default_n")]
    pub n: u32,
    /// 图像尺寸，例 "1024x1024" / "1792x1024"。
    /// 不提供时走 client 的默认值（构造时设置）。
    pub size: Option<String>,
    /// 覆盖 client 的默认模型；不提供走默认。
    pub model: Option<String>,
}

const fn default_n() -> u32 {
    1
}

/// 单张图像的返回结果。`bytes` 是已经 base64 解码后的二进制（PNG/WEBP/JPG 由服务端定）。
#[derive(Debug, Clone)]
pub struct GeneratedImage {
    pub bytes: Vec<u8>,
    /// 服务端返回的格式提示（用于猜测文件扩展名），默认 "png"。
    pub format_hint: String,
}

#[derive(Debug, Clone)]
pub struct ImageGenResponse {
    pub model: String,
    pub images: Vec<GeneratedImage>,
    /// 服务端给的 revised prompt（dall-e-3 会重写），方便 LLM 后续看真实输入。
    pub revised_prompt: Option<String>,
}

#[derive(Debug, Error)]
pub enum ImageGenError {
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
    #[error("configuration error: {0}")]
    Config(String),
    #[error("server returned no usable image data")]
    Empty,
}

#[async_trait]
pub trait ImageGenClient: Send + Sync {
    async fn generate(&self, request: ImageGenRequest) -> Result<ImageGenResponse, ImageGenError>;
}
