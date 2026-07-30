//! Truth snapshot staging, activation, and verification errors.

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TruthSnapshotError {
    #[error("unknown truth source `{0}`")]
    UnknownSource(String),
    #[error("truth source `{0}` was staged more than once")]
    DuplicateSource(String),
    #[error("truth source `{0}` is missing from the snapshot draft")]
    MissingSource(String),
    #[error("truth source is not a file: {0}")]
    SourceNotFile(PathBuf),
    #[error("truth source `{source_id}` checksum mismatch: expected {expected}, got {actual}")]
    SourceChecksumMismatch {
        source_id: String,
        expected: String,
        actual: String,
    },
    #[error("truth index for source `{0}` is missing")]
    MissingIndex(String),
    #[error("truth index for source `{0}` contains no C# files")]
    EmptyIndex(String),
    #[error("truth index contains an unsupported entry `{path}`: {reason}")]
    InvalidIndexEntry { path: PathBuf, reason: String },
    #[error("truth index integrity mismatch for source `{source_id}`: {detail}")]
    IndexIntegrityMismatch { source_id: String, detail: String },
    #[error("invalid tool version `{tool}`: {reason}")]
    InvalidToolVersion { tool: String, reason: String },
    #[error("invalid snapshot id `{0}`")]
    InvalidSnapshotId(String),
    #[error("truth snapshot manifest is invalid: {0}")]
    InvalidManifest(String),
    #[error("truth snapshot belongs to a different game pack: {0}")]
    PackMismatch(String),
    #[error("truth snapshot path `{relative}` escapes root `{root}`")]
    PathOutsideRoot { root: PathBuf, relative: PathBuf },
    #[error("{action} `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse truth snapshot JSON `{path}`: {message}")]
    Json { path: PathBuf, message: String },
}

pub type TruthSnapshotResult<T> = Result<T, TruthSnapshotError>;
