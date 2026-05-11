//! ProjectFolder 错误类型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project path is not a directory: {0}")]
    NotADirectory(String),
    #[error("project name is empty or contains invalid characters: {0}")]
    InvalidName(String),
    #[error("project directory already exists: {0}")]
    AlreadyExists(String),
    #[error("project file missing: {0}")]
    Missing(String),
    #[error("project is locked by another process (.ats/lock exists): {0}")]
    Locked(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type ProjectResult<T> = Result<T, ProjectError>;
