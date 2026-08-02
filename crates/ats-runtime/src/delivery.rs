use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use ats_kernel::PrimitiveId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CancellationToken, RunId, normalize_relative_path};

#[derive(Debug, Clone)]
pub struct BuildStepRequest {
    pub primitive: PrimitiveId,
    pub project_root: PathBuf,
    pub run_id: RunId,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildStepReport {
    pub primitive: PrimitiveId,
    pub exit_code: i32,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

impl BuildStepReport {
    pub fn validate(&self) -> Result<(), BuildError> {
        if self.stdout_tail.chars().count() > 8_000
            || self.stderr_tail.chars().count() > 8_000
            || self.stdout_tail.contains('\0')
            || self.stderr_tail.contains('\0')
        {
            return Err(BuildError::InvalidReport);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("build Primitive is not registered")]
    UnknownPrimitive,
    #[error("build step rejected the project")]
    Rejected(BuildStepReport),
    #[error("build execution is unavailable during {operation}")]
    Unavailable {
        operation: &'static str,
        kind: std::io::ErrorKind,
    },
    #[error("build report is invalid")]
    InvalidReport,
    #[error("build was cancelled")]
    Cancelled,
}

#[async_trait]
pub trait BuildRunner: Send + Sync {
    async fn run_step(
        &self,
        request: BuildStepRequest,
        cancellation: &CancellationToken,
    ) -> Result<BuildStepReport, BuildError>;
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PackageEntry {
    source_relative_path: String,
    archive_path: String,
}

impl PackageEntry {
    pub fn new(
        source_relative_path: impl Into<String>,
        archive_path: impl Into<String>,
    ) -> Result<Self, PackageError> {
        let source_relative_path = source_relative_path.into();
        let archive_path = archive_path.into();
        if normalize_relative_path(Path::new(&source_relative_path))? != source_relative_path
            || normalize_relative_path(Path::new(&archive_path))? != archive_path
            || source_relative_path.starts_with(".ats/")
            || archive_path.starts_with(".ats/")
        {
            return Err(PackageError::InvalidRequest);
        }
        Ok(Self {
            source_relative_path,
            archive_path,
        })
    }

    #[must_use]
    pub fn source_relative_path(&self) -> &str {
        &self.source_relative_path
    }

    #[must_use]
    pub fn archive_path(&self) -> &str {
        &self.archive_path
    }
}

#[derive(Debug, Clone)]
pub struct PackagePrepareRequest {
    pub project_root: PathBuf,
    pub source_relative_root: String,
    pub output_relative_path: String,
    pub run_id: RunId,
    pub entries: Vec<PackageEntry>,
    pub compression_level: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageReport {
    pub file_count: u32,
    pub uncompressed_bytes: u64,
    pub package_bytes: u64,
}

pub trait PendingPackageOutput: Send {
    fn report(&self) -> &PackageReport;
    fn output_path(&self) -> &Path;
    fn output_relative_path(&self) -> &str;
    fn commit(self: Box<Self>) -> Result<(), PackageError>;
    fn rollback(self: Box<Self>) -> Result<(), PackageError>;
}

pub trait PackageWriter: Send + Sync {
    fn prepare(
        &self,
        request: PackagePrepareRequest,
        cancellation: &CancellationToken,
    ) -> Result<Box<dyn PendingPackageOutput>, PackageError>;
}

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("package request is invalid")]
    InvalidRequest,
    #[error("package request contains duplicate source or archive paths")]
    DuplicateEntry,
    #[error("package source is missing or unsafe")]
    InvalidSource,
    #[error("package output is unsafe")]
    InvalidOutput,
    #[error("package construction failed during {operation}")]
    Io {
        operation: &'static str,
        kind: std::io::ErrorKind,
        #[source]
        source: std::io::Error,
    },
    #[error("package archive construction failed")]
    Archive,
    #[error("package construction was cancelled")]
    Cancelled,
}

impl From<crate::ArtifactContractError> for PackageError {
    fn from(_: crate::ArtifactContractError) -> Self {
        Self::InvalidRequest
    }
}

pub fn validate_package_request(request: &PackagePrepareRequest) -> Result<(), PackageError> {
    if normalize_relative_path(Path::new(&request.source_relative_root))?
        != request.source_relative_root
        || normalize_relative_path(Path::new(&request.output_relative_path))?
            != request.output_relative_path
        || request.source_relative_root.starts_with(".ats/")
        || request.output_relative_path.starts_with(".ats/")
        || request.entries.is_empty()
        || request.entries.len() > 512
        || request
            .compression_level
            .is_some_and(|level| !(0..=9).contains(&level))
    {
        return Err(PackageError::InvalidRequest);
    }
    let mut sources = BTreeSet::new();
    let mut archive_paths = BTreeSet::new();
    for entry in &request.entries {
        if !sources.insert(entry.source_relative_path())
            || !archive_paths.insert(entry.archive_path())
        {
            return Err(PackageError::DuplicateEntry);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_request_rejects_escape_reserved_and_duplicates() {
        assert!(PackageEntry::new("runtime/mod.bin", "runtime/mod.bin").is_ok());
        assert!(PackageEntry::new("../escape", "escape").is_err());
        let entry = PackageEntry::new("runtime/mod.bin", "runtime/mod.bin").unwrap();
        let request = PackagePrepareRequest {
            project_root: PathBuf::from("project"),
            source_relative_root: "delivery".into(),
            output_relative_path: "packages/mod.zip".into(),
            run_id: RunId::new(),
            entries: vec![entry.clone(), entry],
            compression_level: Some(5),
        };
        assert!(matches!(
            validate_package_request(&request),
            Err(PackageError::DuplicateEntry)
        ));
    }
}
