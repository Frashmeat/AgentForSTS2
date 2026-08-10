use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use async_trait::async_trait;
use ats_kernel::PrimitiveId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CancellationReason, RunId, normalize_relative_path};

#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    state: Arc<AtomicU8>,
}

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self, reason: CancellationReason) -> bool {
        self.state
            .compare_exchange(0, encode_reason(reason), Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    #[must_use]
    pub fn reason(&self) -> Option<CancellationReason> {
        decode_reason(self.state.load(Ordering::SeqCst))
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.reason().is_some()
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProjectFileWrite {
    relative_path: String,
    bytes: Vec<u8>,
    source_path: Option<PathBuf>,
}

impl ProjectFileWrite {
    pub fn new(
        relative_path: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self, ProjectWriteError> {
        let relative_path = relative_path.into();
        if normalize_relative_path(Path::new(&relative_path))? != relative_path
            || relative_path.starts_with(".ats/")
            || bytes.is_empty()
            || bytes.len() > 16 * 1024 * 1024
        {
            return Err(ProjectWriteError::InvalidWrite);
        }
        Ok(Self {
            relative_path,
            bytes,
            source_path: None,
        })
    }

    pub fn from_source(
        relative_path: impl Into<String>,
        source_path: PathBuf,
    ) -> Result<Self, ProjectWriteError> {
        let relative_path = relative_path.into();
        if normalize_relative_path(Path::new(&relative_path))? != relative_path
            || relative_path.starts_with(".ats/")
            || source_path.as_os_str().is_empty()
        {
            return Err(ProjectWriteError::InvalidWrite);
        }
        Ok(Self {
            relative_path,
            bytes: Vec::new(),
            source_path: Some(source_path),
        })
    }

    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub fn bytes(&self) -> Option<&[u8]> {
        self.source_path.is_none().then_some(self.bytes.as_slice())
    }

    #[must_use]
    pub fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }
}

pub trait PendingProjectWrites: Send {
    fn commit(self: Box<Self>) -> Result<(), ProjectWriteError>;
    fn rollback(self: Box<Self>) -> Result<(), ProjectWriteError>;
}

pub trait ProjectFileWriter: Send + Sync {
    fn apply(
        &self,
        project_root: &Path,
        run_id: &RunId,
        writes: Vec<ProjectFileWrite>,
    ) -> Result<Box<dyn PendingProjectWrites>, ProjectWriteError>;
}

#[derive(Debug, Clone)]
pub struct ProjectStageRequest {
    pub project_root: PathBuf,
    pub run_id: RunId,
}

pub trait PendingProjectStage: Send {
    fn root(&self) -> &Path;
    fn cleanup(self: Box<Self>) -> Result<(), ProjectStageError>;
}

pub trait ProjectStager: Send + Sync {
    fn stage(
        &self,
        request: ProjectStageRequest,
    ) -> Result<Box<dyn PendingProjectStage>, ProjectStageError>;
}

#[derive(Debug, Error)]
pub enum ProjectStageError {
    #[error("project stage source is invalid")]
    InvalidSource,
    #[error("project stage exceeds the bounded copy limits")]
    LimitExceeded,
    #[error("project staging failed during {operation}")]
    Io {
        operation: &'static str,
        kind: std::io::ErrorKind,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Error)]
pub enum ProjectWriteError {
    #[error("project write path or content is invalid")]
    InvalidWrite,
    #[error("project write plan contains a duplicate path")]
    DuplicatePath,
    #[error("project write I/O failed during {operation}")]
    Io {
        operation: &'static str,
        kind: std::io::ErrorKind,
        #[source]
        source: std::io::Error,
    },
}

impl From<crate::ArtifactContractError> for ProjectWriteError {
    fn from(_: crate::ArtifactContractError) -> Self {
        Self::InvalidWrite
    }
}

pub fn validate_project_writes(writes: &[ProjectFileWrite]) -> Result<(), ProjectWriteError> {
    if writes.is_empty() || writes.len() > 256 {
        return Err(ProjectWriteError::InvalidWrite);
    }
    let mut paths = BTreeSet::new();
    for write in writes {
        if !paths.insert(write.relative_path()) {
            return Err(ProjectWriteError::DuplicatePath);
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ValidationRequest {
    pub primitive: PrimitiveId,
    pub project_root: PathBuf,
    pub run_id: RunId,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationReport {
    pub exit_code: i32,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

impl ValidationReport {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.stdout_tail.chars().count() > 8_000
            || self.stderr_tail.chars().count() > 8_000
            || self.stdout_tail.contains('\0')
            || self.stderr_tail.contains('\0')
        {
            return Err(ValidationError::InvalidReport);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("validation Primitive is not registered")]
    UnknownPrimitive,
    #[error("validation rejected the generated project")]
    Rejected(ValidationReport),
    #[error("validation execution is unavailable during {operation}")]
    Unavailable {
        operation: &'static str,
        kind: std::io::ErrorKind,
    },
    #[error("validation report is invalid")]
    InvalidReport,
    #[error("validation was cancelled")]
    Cancelled,
}

#[async_trait]
pub trait ValidationRunner: Send + Sync {
    async fn validate(
        &self,
        request: ValidationRequest,
        cancellation: &CancellationToken,
    ) -> Result<ValidationReport, ValidationError>;
}

fn encode_reason(reason: CancellationReason) -> u8 {
    match reason {
        CancellationReason::User => 1,
        CancellationReason::Pause => 5,
        CancellationReason::ProjectClose => 2,
        CancellationReason::ProjectSwitch => 3,
        CancellationReason::AppShutdown => 4,
    }
}

fn decode_reason(value: u8) -> Option<CancellationReason> {
    match value {
        1 => Some(CancellationReason::User),
        2 => Some(CancellationReason::ProjectClose),
        3 => Some(CancellationReason::ProjectSwitch),
        4 => Some(CancellationReason::AppShutdown),
        5 => Some(CancellationReason::Pause),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_first_writer_wins() {
        let token = CancellationToken::new();
        assert!(token.cancel(CancellationReason::ProjectClose));
        assert!(!token.cancel(CancellationReason::User));
        assert_eq!(token.reason(), Some(CancellationReason::ProjectClose));
    }

    #[test]
    fn project_write_plan_rejects_reserved_duplicate_and_empty_content() {
        assert!(ProjectFileWrite::new("Generated/One.cs", b"one".to_vec()).is_ok());
        assert!(ProjectFileWrite::new("../escape", b"one".to_vec()).is_err());
        assert!(ProjectFileWrite::new(".ats/internal", b"one".to_vec()).is_err());
        assert!(ProjectFileWrite::new("Generated/Empty.cs", Vec::new()).is_err());
        assert!(
            ProjectFileWrite::from_source("packages/mod.zip", PathBuf::from("stage/mod.zip"))
                .is_ok()
        );
        let same = ProjectFileWrite::new("Generated/Same.cs", b"same".to_vec()).unwrap();
        assert!(matches!(
            validate_project_writes(&[same.clone(), same]),
            Err(ProjectWriteError::DuplicatePath)
        ));
    }
}
