//! Game-pack loading and registry errors.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GamePackError {
    #[error("read game pack `{path}`: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid game pack field `{field}` in `{source_name}`: {message}")]
    InvalidField {
        source_name: String,
        field: String,
        message: String,
    },
    #[error(
        "unsupported game pack schema at `schema_version` in `{source_name}`: {found}; supported: {supported}"
    )]
    UnsupportedSchema {
        source_name: String,
        found: u32,
        supported: u32,
    },
    #[error("duplicate truth source id `{id}` at `truth_sources[{index}].id` in `{source_name}`")]
    DuplicateTruthSourceId {
        source_name: String,
        id: String,
        index: usize,
    },
    #[error("duplicate game pack id `{id}`")]
    DuplicatePackId { id: String },
    #[error("unknown game pack id `{id}`")]
    UnknownPackId { id: String },
    #[error("game pack path `{relative}` escapes pack root `{root}` (resolved to `{resolved}`)")]
    PathOutsideRoot {
        root: PathBuf,
        relative: PathBuf,
        resolved: PathBuf,
    },
}

pub type GamePackResult<T> = Result<T, GamePackError>;
