//! Platform job 操作的错误类型。

use thiserror::Error;

#[derive(Debug, Error)]
pub enum JobError {
    #[error("job not found: {0}")]
    NotFound(String),
    #[error("job already exists: {0}")]
    AlreadyExists(String),
    #[error("job is in terminal state ({status:?}) and cannot transition: {id}")]
    Terminal { id: String, status: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("storage backend error: {0}")]
    Storage(String),
}

pub type JobResult<T> = Result<T, JobError>;
