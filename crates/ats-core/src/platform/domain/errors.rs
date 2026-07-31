//! Run repository and lifecycle errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RunError {
    #[error("run not found: {0}")]
    NotFound(String),
    #[error("run already exists: {0}")]
    AlreadyExists(String),
    #[error("invalid run transition for {id}: {message}")]
    InvalidTransition { id: String, message: String },
    #[error("invalid run record {id}: {message}")]
    InvalidRecord { id: String, message: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("storage backend error: {0}")]
    Storage(String),
}

pub type RunRepositoryResult<T> = Result<T, RunError>;
