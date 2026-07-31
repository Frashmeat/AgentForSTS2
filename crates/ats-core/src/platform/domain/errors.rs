//! Run repository and lifecycle errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("package source directory is missing")]
    SourceMissing,
    #[error("package output path is invalid")]
    OutputInvalid,
    #[error("required package file is missing: {relative_path}")]
    RequiredFileMissing { relative_path: String },
    #[error("package path escapes the declared source root: {relative_path}")]
    PathEscape { relative_path: String },
    #[error("package path may not be a symbolic link: {relative_path}")]
    Symlink { relative_path: String },
    #[error("package path is not a regular file: {relative_path}")]
    NotRegularFile { relative_path: String },
    #[error("package I/O failed during {operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("ZIP encoding failed")]
    Zip,
    #[error("package worker failed")]
    Worker,
}

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
