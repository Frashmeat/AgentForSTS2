//! 顶层错误类型。Stage 1 补充模块级 thiserror 派生。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;
