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
    #[error(
        "project metadata is missing required `game_id`; legacy projects must be recreated or migrated explicitly"
    )]
    MissingGameId,
    #[error("project references unknown game pack `{0}`")]
    UnknownGameId(String),
    #[error("game pack registry is unavailable: {0}")]
    GamePackRegistry(String),
    #[error("project scaffold does not satisfy the game pack contract: {0}")]
    ScaffoldContract(String),
    #[error("invalid project schema version in `{path}`: `{value}`")]
    InvalidSchemaVersion { path: String, value: String },
    #[error("unsupported project schema version {found}; expected {expected}")]
    UnsupportedSchemaVersion { found: u32, expected: u32 },
    #[error("invalid run history schema marker in `{path}`: `{value}`")]
    InvalidHistorySchema { path: String, value: String },
    #[error("project is locked by another process (.ats/lock is held): {0}")]
    Locked(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type ProjectResult<T> = Result<T, ProjectError>;
