//! Immutable artifact snapshots and their reproducible manifest.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::codegen::GenerationEvidence;
use crate::game_pack::{TruthSnapshotIndex, TruthSnapshotSource, VerifiedGameContext};
use crate::image_proc::{ImageProcessingProvenance, ImageProcessor};
use crate::platform::domain::RunId;

pub const ARTIFACT_MANIFEST_SCHEMA_VERSION: u32 = 2;
const PUBLISH_RENAME_RETRY_DELAYS: [Duration; 5] = [
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(400),
    Duration::from_millis(800),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub artifact_id: String,
    pub artifact_kind: String,
    pub producing_run_id: RunId,
    pub created_at: DateTime<Utc>,
    pub game_context: ArtifactGameContext,
    pub evidence: Vec<ArtifactEvidence>,
    pub generation: ArtifactGeneration,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_processing: Option<ImageProcessingProvenance>,
    pub files: Vec<ArtifactFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactGameContext {
    pub game_pack_id: String,
    pub game_pack_schema_version: u32,
    pub game_pack_sha256: String,
    pub snapshot_id: String,
    pub snapshot_schema_version: u32,
    pub sources: Vec<TruthSnapshotSource>,
    pub indexes: Vec<TruthSnapshotIndex>,
    pub tool_versions: BTreeMap<String, String>,
}

impl From<&VerifiedGameContext> for ArtifactGameContext {
    fn from(context: &VerifiedGameContext) -> Self {
        let evidence = context.evidence();
        Self {
            game_pack_id: evidence.game_pack_id.to_string(),
            game_pack_schema_version: evidence.game_pack_schema_version,
            game_pack_sha256: evidence.game_pack_sha256.to_string(),
            snapshot_id: evidence.snapshot_id.to_string(),
            snapshot_schema_version: evidence.snapshot_schema_version,
            sources: evidence.sources.to_vec(),
            indexes: evidence.indexes.to_vec(),
            tool_versions: evidence.tool_versions.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactEvidence {
    pub source: String,
    pub symbol: String,
    pub purpose: String,
    pub bounded_excerpt: String,
}

impl From<GenerationEvidence> for ArtifactEvidence {
    fn from(evidence: GenerationEvidence) -> Self {
        Self {
            source: evidence.source,
            symbol: evidence.symbol,
            purpose: evidence.purpose,
            bounded_excerpt: evidence.bounded_excerpt,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactGeneration {
    pub provider: String,
    pub model: String,
    pub inputs_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactFile {
    pub role: String,
    pub snapshot_relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_relative_path: Option<String>,
    pub byte_length: u64,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct ArtifactFileInput {
    pub role: String,
    pub source_path: PathBuf,
    pub published_relative_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArtifactPublishRequest {
    pub artifact_id: String,
    pub artifact_kind: String,
    pub run_id: RunId,
    pub game_context: ArtifactGameContext,
    pub evidence: Vec<ArtifactEvidence>,
    pub generation: ArtifactGeneration,
    pub image_processing: Option<ImageProcessingProvenance>,
    pub files: Vec<ArtifactFileInput>,
}

#[derive(Debug, Clone)]
pub struct PublishedArtifact {
    pub artifact_manifest_ref: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Error)]
pub enum ArtifactError {
    #[error("unsafe artifact identifier `{0}`")]
    UnsafeId(String),
    #[error("unsafe project-relative path `{0}`")]
    UnsafeRelativePath(String),
    #[error("artifact source is not a regular file: {0}")]
    NotRegularFile(String),
    #[error("artifact source may not be a symlink: {0}")]
    Symlink(String),
    #[error("artifact snapshot already exists: {0}")]
    AlreadyExists(String),
    #[error("invalid artifact manifest: {0}")]
    InvalidManifest(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type ArtifactResult<T> = Result<T, ArtifactError>;

#[derive(Debug, Clone)]
pub struct ArtifactStore {
    project_root: PathBuf,
    root: PathBuf,
}

pub struct LegacyArtifactCleanup {
    artifact_root: PathBuf,
    backup_root: Option<PathBuf>,
    moved_names: Vec<std::ffi::OsString>,
}

impl ArtifactStore {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        let root = project_root.join("artifacts");
        Self { project_root, root }
    }

    pub fn project_relative_ref(&self, path: &Path) -> ArtifactResult<String> {
        let relative = path
            .strip_prefix(&self.project_root)
            .map_err(|_| ArtifactError::UnsafeRelativePath(path.display().to_string()))?;
        normalize_relative_path(relative)
    }

    pub fn publish(&self, request: ArtifactPublishRequest) -> ArtifactResult<PublishedArtifact> {
        validate_safe_segment(&request.artifact_id)?;
        if !request.run_id.is_safe_segment() {
            return Err(ArtifactError::UnsafeId(request.run_id.0));
        }
        if request.files.is_empty() {
            return Err(ArtifactError::InvalidManifest(
                "artifact must contain at least one file".into(),
            ));
        }

        ensure_directory(&self.root)?;
        let artifact_root = self.root.join(&request.artifact_id);
        ensure_directory(&artifact_root)?;
        let runs_root = artifact_root.join("runs");
        ensure_directory(&runs_root)?;
        let final_dir = runs_root.join(&request.run_id.0);
        if final_dir.exists() {
            return Err(ArtifactError::AlreadyExists(
                final_dir.display().to_string(),
            ));
        }
        let staging_dir = runs_root.join(format!(".staging-{}", request.run_id));
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir)?;
        }
        fs::create_dir_all(staging_dir.join("files"))?;

        let result = (|| {
            let manifest = self.build_staging(&staging_dir, &request)?;
            let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
            fs::write(staging_dir.join("artifact-manifest.json"), &manifest_bytes)?;
            let manifest_ref = normalize_relative_path(
                final_dir
                    .join("artifact-manifest.json")
                    .strip_prefix(&self.project_root)
                    .map_err(|_| {
                        ArtifactError::UnsafeRelativePath(final_dir.display().to_string())
                    })?,
            )?;
            publish_staging_directory(&staging_dir, &final_dir)?;
            Ok(PublishedArtifact {
                artifact_manifest_ref: manifest_ref,
                manifest_sha256: sha256_bytes(&manifest_bytes),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging_dir);
        }
        result
    }

    pub fn remove_published_run(&self, artifact_id: &str, run_id: &RunId) -> ArtifactResult<()> {
        validate_safe_segment(artifact_id)?;
        if !run_id.is_safe_segment() {
            return Err(ArtifactError::UnsafeId(run_id.0.clone()));
        }
        let artifact_root = self.root.join(artifact_id);
        let runs_root = artifact_root.join("runs");
        reject_symlink_if_exists(&self.root)?;
        reject_symlink_if_exists(&artifact_root)?;
        reject_symlink_if_exists(&runs_root)?;
        let run_root = runs_root.join(&run_id.0);
        match fs::symlink_metadata(&run_root) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(ArtifactError::Symlink(run_root.display().to_string()))
            }
            Ok(metadata) if metadata.is_dir() => {
                fs::remove_dir_all(run_root)?;
                Ok(())
            }
            Ok(_) => Err(ArtifactError::NotRegularFile(
                run_root.display().to_string(),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn begin_legacy_cleanup(
        &self,
        artifact_id: &str,
        run_id: &RunId,
    ) -> ArtifactResult<LegacyArtifactCleanup> {
        validate_safe_segment(artifact_id)?;
        if !run_id.is_safe_segment() {
            return Err(ArtifactError::UnsafeId(run_id.0.clone()));
        }
        let artifact_root = self.root.join(artifact_id);
        reject_symlink_if_exists(&self.root)?;
        reject_symlink_if_exists(&artifact_root)?;
        if !artifact_root.is_dir() {
            return Ok(LegacyArtifactCleanup {
                artifact_root,
                backup_root: None,
                moved_names: Vec::new(),
            });
        }

        let mut legacy_entries = Vec::new();
        for entry in fs::read_dir(&artifact_root)? {
            let entry = entry?;
            let name = entry.file_name();
            if name == "runs" {
                continue;
            }
            if name.to_string_lossy().starts_with(".legacy-backup-") {
                return Err(ArtifactError::AlreadyExists(
                    entry.path().display().to_string(),
                ));
            }
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                return Err(ArtifactError::Symlink(entry.path().display().to_string()));
            }
            legacy_entries.push(name);
        }
        if legacy_entries.is_empty() {
            return Ok(LegacyArtifactCleanup {
                artifact_root,
                backup_root: None,
                moved_names: Vec::new(),
            });
        }

        let backup_root = artifact_root.join(format!(".legacy-backup-{}", run_id.0));
        fs::create_dir(&backup_root)?;
        let mut transaction = LegacyArtifactCleanup {
            artifact_root,
            backup_root: Some(backup_root),
            moved_names: Vec::new(),
        };
        for name in legacy_entries {
            let source = transaction.artifact_root.join(&name);
            let destination = transaction
                .backup_root
                .as_ref()
                .expect("backup exists for legacy entries")
                .join(&name);
            if let Err(error) = fs::rename(&source, &destination) {
                let _ = transaction.rollback_in_place();
                return Err(error.into());
            }
            transaction.moved_names.push(name);
        }
        Ok(transaction)
    }

    pub async fn publish_async(
        &self,
        request: ArtifactPublishRequest,
    ) -> ArtifactResult<PublishedArtifact> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || store.publish(request))
            .await
            .map_err(|error| {
                ArtifactError::InvalidManifest(format!("publish task failed: {error}"))
            })?
    }

    fn build_staging(
        &self,
        staging_dir: &Path,
        request: &ArtifactPublishRequest,
    ) -> ArtifactResult<ArtifactManifest> {
        let mut files = Vec::with_capacity(request.files.len());
        for (index, input) in request.files.iter().enumerate() {
            let metadata = fs::symlink_metadata(&input.source_path)?;
            if metadata.file_type().is_symlink() {
                return Err(ArtifactError::Symlink(
                    input.source_path.display().to_string(),
                ));
            }
            if !metadata.is_file() {
                return Err(ArtifactError::NotRegularFile(
                    input.source_path.display().to_string(),
                ));
            }
            let file_name = input
                .source_path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    ArtifactError::UnsafeRelativePath(input.source_path.display().to_string())
                })?;
            let snapshot_relative_path = format!("files/{index:03}-{file_name}");
            let destination = staging_dir.join(&snapshot_relative_path);
            let bytes = fs::read(&input.source_path)?;
            fs::write(&destination, &bytes)?;
            let published_relative_path = input
                .published_relative_path
                .as_deref()
                .map(|path| normalize_relative_path(Path::new(path)))
                .transpose()?;
            files.push(ArtifactFile {
                role: input.role.clone(),
                snapshot_relative_path,
                published_relative_path,
                byte_length: bytes.len() as u64,
                sha256: sha256_bytes(&bytes),
            });
        }

        let manifest = ArtifactManifest {
            schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION,
            artifact_id: request.artifact_id.clone(),
            artifact_kind: request.artifact_kind.clone(),
            producing_run_id: request.run_id.clone(),
            created_at: Utc::now(),
            game_context: request.game_context.clone(),
            evidence: request.evidence.clone(),
            generation: request.generation.clone(),
            image_processing: request.image_processing.clone(),
            files,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

fn publish_staging_directory(staging_dir: &Path, final_dir: &Path) -> io::Result<()> {
    publish_staging_directory_with(
        staging_dir,
        final_dir,
        |from, to| fs::rename(from, to),
        thread::sleep,
        is_transient_publish_rename_error,
    )
}

fn publish_staging_directory_with<R, S, C>(
    staging_dir: &Path,
    final_dir: &Path,
    mut rename: R,
    mut sleep: S,
    is_retryable: C,
) -> io::Result<()>
where
    R: FnMut(&Path, &Path) -> io::Result<()>,
    S: FnMut(Duration),
    C: Fn(&io::Error) -> bool,
{
    let mut attempt = 1_u8;
    loop {
        match rename(staging_dir, final_dir) {
            Ok(()) => return Ok(()),
            Err(error)
                if is_retryable(&error)
                    && usize::from(attempt) <= PUBLISH_RENAME_RETRY_DELAYS.len() =>
            {
                let delay = PUBLISH_RENAME_RETRY_DELAYS[usize::from(attempt) - 1];
                tracing::warn!(
                    attempt,
                    next_attempt = attempt + 1,
                    delay_ms = delay.as_millis(),
                    io_kind = ?error.kind(),
                    "artifact directory publish was temporarily unavailable; retrying"
                );
                sleep(delay);
                attempt += 1;
            }
            Err(error) => {
                let _ = fs::remove_dir_all(staging_dir);
                return Err(error);
            }
        }
    }
}

fn is_transient_publish_rename_error(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Interrupted {
        return true;
    }
    #[cfg(windows)]
    {
        error.kind() == io::ErrorKind::PermissionDenied
            || matches!(error.raw_os_error(), Some(5 | 32 | 33))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

impl LegacyArtifactCleanup {
    pub fn rollback(mut self) -> ArtifactResult<()> {
        self.rollback_in_place()
    }

    fn rollback_in_place(&mut self) -> ArtifactResult<()> {
        let Some(backup_root) = &self.backup_root else {
            return Ok(());
        };
        for name in self.moved_names.iter().rev() {
            fs::rename(backup_root.join(name), self.artifact_root.join(name))?;
        }
        self.moved_names.clear();
        fs::remove_dir(backup_root)?;
        self.backup_root = None;
        Ok(())
    }

    pub fn commit(self) -> ArtifactResult<()> {
        if let Some(backup_root) = self.backup_root {
            fs::remove_dir_all(backup_root)?;
        }
        Ok(())
    }
}

#[must_use]
pub fn snapshot_evidence(context: &VerifiedGameContext) -> Vec<ArtifactEvidence> {
    let identity = context.evidence();
    let mut evidence = identity
        .sources
        .iter()
        .map(|source| ArtifactEvidence {
            source: source.relative_path.clone(),
            symbol: source.id.clone(),
            purpose: "verified truth source used by this generation".into(),
            bounded_excerpt: format!(
                "kind={}; sha256={}; bytes={}",
                source.kind, source.sha256, source.size_bytes
            ),
        })
        .collect::<Vec<_>>();
    evidence.extend(identity.indexes.iter().map(|index| ArtifactEvidence {
        source: index.relative_root.clone(),
        symbol: index.source_id.clone(),
        purpose: format!(
            "structured index via {} / {}",
            index.indexer, index.provider
        ),
        bounded_excerpt: format!(
            "files={}; csharpFiles={}; treeSha256={}",
            index.file_count, index.cs_file_count, index.tree_sha256
        ),
    }));
    evidence
}

impl ArtifactManifest {
    pub fn validate(&self) -> ArtifactResult<()> {
        if self.schema_version != ARTIFACT_MANIFEST_SCHEMA_VERSION {
            return Err(ArtifactError::InvalidManifest(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        validate_safe_segment(&self.artifact_id)?;
        if !self.producing_run_id.is_safe_segment() {
            return Err(ArtifactError::UnsafeId(self.producing_run_id.0.clone()));
        }
        validate_sha256(&self.game_context.game_pack_sha256)?;
        validate_sha256(&self.game_context.snapshot_id)?;
        validate_sha256(&self.generation.inputs_sha256)?;
        if let Some(image_processing) = &self.image_processing {
            validate_image_processing(image_processing)?;
        }
        for file in &self.files {
            let relative = normalize_relative_path(Path::new(&file.snapshot_relative_path))?;
            if !relative.starts_with("files/") {
                return Err(ArtifactError::UnsafeRelativePath(relative));
            }
            validate_sha256(&file.sha256)?;
            if let Some(path) = &file.published_relative_path {
                normalize_relative_path(Path::new(path))?;
            }
        }
        Ok(())
    }
}

fn validate_image_processing(provenance: &ImageProcessingProvenance) -> ArtifactResult<()> {
    if !provenance.build.is_consistent() {
        return Err(ArtifactError::InvalidManifest(
            "image processing BuildInfo is inconsistent".into(),
        ));
    }
    match provenance.processor {
        ImageProcessor::Simple => {
            if provenance.model_sha256.is_some() || provenance.runtime_version.is_some() {
                return Err(ArtifactError::InvalidManifest(
                    "simple image processing cannot claim model or runtime metadata".into(),
                ));
            }
        }
        ImageProcessor::MlU2netp => {
            let model_sha256 = provenance.model_sha256.as_deref().ok_or_else(|| {
                ArtifactError::InvalidManifest(
                    "ML image processing requires a model SHA-256".into(),
                )
            })?;
            validate_sha256(model_sha256)?;
            if provenance.runtime_version.as_deref().is_none_or(|version| {
                version.is_empty()
                    || version.len() > 64
                    || !version.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    })
            }) {
                return Err(ArtifactError::InvalidManifest(
                    "ML image processing requires a runtime version".into(),
                ));
            }
            if provenance.fallback.is_some() {
                return Err(ArtifactError::InvalidManifest(
                    "an ML outcome cannot also be a fallback outcome".into(),
                ));
            }
        }
    }
    if let Some(fallback) = &provenance.fallback
        && (provenance.processor != ImageProcessor::Simple
            || fallback.from != ImageProcessor::MlU2netp)
    {
        return Err(ArtifactError::InvalidManifest(
            "fallback provenance must describe ML to simple processing".into(),
        ));
    }
    Ok(())
}

#[must_use]
pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_sha256(value: &str) -> ArtifactResult<()> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ArtifactError::InvalidManifest(format!(
            "invalid SHA-256 `{value}`"
        )))
    }
}

fn validate_safe_segment(value: &str) -> ArtifactResult<()> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(ArtifactError::UnsafeId(value.to_string()))
    }
}

fn ensure_directory(path: &Path) -> ArtifactResult<()> {
    reject_symlink_if_exists(path)?;
    fs::create_dir(path).or_else(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Ok(())
        } else {
            Err(error)
        }
    })?;
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(ArtifactError::Symlink(path.display().to_string()));
    }
    if !metadata.is_dir() {
        return Err(ArtifactError::NotRegularFile(path.display().to_string()));
    }
    Ok(())
}

fn reject_symlink_if_exists(path: &Path) -> ArtifactResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ArtifactError::Symlink(path.display().to_string()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn normalize_relative_path(path: &Path) -> ArtifactResult<String> {
    if path.is_absolute() || path.as_os_str().is_empty() {
        return Err(ArtifactError::UnsafeRelativePath(
            path.display().to_string(),
        ));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            _ => {
                return Err(ArtifactError::UnsafeRelativePath(
                    path.display().to_string(),
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(ArtifactError::UnsafeRelativePath(
            path.display().to_string(),
        ));
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image_proc::{
        ImageProcFallback, ImageProcFallbackReason, ImageProcessingProvenance, ImageProcessor,
    };
    use crate::knowledge::test_support::fixture_game_context;

    #[test]
    fn publishes_immutable_manifest_and_hashes() {
        let temp = tempfile::TempDir::new().unwrap();
        let context = fixture_game_context(temp.path(), &[], &[]);
        let project = temp.path().join("project");
        fs::create_dir_all(project.join("Generated")).unwrap();
        let source = project.join("Generated/Demo.cs");
        fs::write(&source, "class Demo {}").unwrap();
        fs::create_dir_all(project.join("artifacts/Demo")).unwrap();
        let legacy = project.join("artifacts/Demo/legacy.txt");
        fs::write(&legacy, b"legacy").unwrap();
        let store = ArtifactStore::new(project.clone());
        let run_id = RunId::new();
        let published = store
            .publish(ArtifactPublishRequest {
                artifact_id: "Demo".into(),
                artifact_kind: "code".into(),
                run_id: run_id.clone(),
                game_context: ArtifactGameContext::from(&context),
                evidence: Vec::new(),
                generation: ArtifactGeneration {
                    provider: "fixture".into(),
                    model: "fixture".into(),
                    inputs_sha256: sha256_bytes(b"fixture input"),
                },
                image_processing: None,
                files: vec![ArtifactFileInput {
                    role: "csharp".into(),
                    source_path: source,
                    published_relative_path: Some("Generated/Demo.cs".into()),
                }],
            })
            .unwrap();

        assert!(project.join(&published.artifact_manifest_ref).is_file());
        assert_eq!(published.manifest_sha256.len(), 64);
        let cleanup = store.begin_legacy_cleanup("Demo", &run_id).unwrap();
        assert!(!legacy.exists());
        cleanup.rollback().unwrap();
        assert!(legacy.exists());
        let cleanup = store.begin_legacy_cleanup("Demo", &run_id).unwrap();
        cleanup.commit().unwrap();
        assert!(!legacy.exists());
        assert!(
            store
                .publish(ArtifactPublishRequest {
                    artifact_id: "Demo".into(),
                    artifact_kind: "code".into(),
                    run_id: run_id.clone(),
                    game_context: ArtifactGameContext::from(&context),
                    evidence: Vec::new(),
                    generation: ArtifactGeneration {
                        provider: "fixture".into(),
                        model: "fixture".into(),
                        inputs_sha256: sha256_bytes(b"fixture input"),
                    },
                    image_processing: None,
                    files: vec![],
                })
                .is_err()
        );

        store.remove_published_run("Demo", &run_id).unwrap();
        assert!(!project.join(&published.artifact_manifest_ref).exists());
    }

    #[test]
    fn publish_directory_retries_transient_conflicts_then_renames_atomically() {
        let temp = tempfile::TempDir::new().unwrap();
        let staging = temp.path().join(".staging-run");
        let final_dir = temp.path().join("run");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("artifact-manifest.json"), b"complete").unwrap();
        let mut attempts = 0_u8;
        let mut delays = Vec::new();

        publish_staging_directory_with(
            &staging,
            &final_dir,
            |from, to| {
                attempts += 1;
                if attempts <= 2 {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "sharing violation canary",
                    ))
                } else {
                    fs::rename(from, to)
                }
            },
            |delay| delays.push(delay),
            |error| error.kind() == io::ErrorKind::PermissionDenied,
        )
        .unwrap();

        assert_eq!(attempts, 3);
        assert_eq!(delays, PUBLISH_RENAME_RETRY_DELAYS[..2]);
        assert!(!staging.exists());
        assert_eq!(
            fs::read(final_dir.join("artifact-manifest.json")).unwrap(),
            b"complete"
        );
    }

    #[test]
    fn publish_directory_exhaustion_removes_incomplete_staging() {
        let temp = tempfile::TempDir::new().unwrap();
        let staging = temp.path().join(".staging-run");
        let final_dir = temp.path().join("run");
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("artifact-manifest.json"), b"incomplete").unwrap();
        let mut attempts = 0_u8;

        let error = publish_staging_directory_with(
            &staging,
            &final_dir,
            |_, _| {
                attempts += 1;
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "sharing violation canary",
                ))
            },
            |_| {},
            |_| true,
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(usize::from(attempts), PUBLISH_RENAME_RETRY_DELAYS.len() + 1);
        assert!(!staging.exists());
        assert!(!final_dir.exists());
    }

    #[test]
    fn publish_directory_does_not_retry_deterministic_errors() {
        let temp = tempfile::TempDir::new().unwrap();
        let staging = temp.path().join(".staging-run");
        let final_dir = temp.path().join("run");
        fs::create_dir(&staging).unwrap();
        let mut attempts = 0_u8;
        let mut sleeps = 0_u8;

        let error = publish_staging_directory_with(
            &staging,
            &final_dir,
            |_, _| {
                attempts += 1;
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "deterministic conflict canary",
                ))
            },
            |_| sleeps += 1,
            |_| false,
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(attempts, 1);
        assert_eq!(sleeps, 0);
        assert!(!staging.exists());
        assert!(!final_dir.exists());
    }

    #[test]
    fn publish_rename_classifier_is_platform_bounded() {
        assert!(is_transient_publish_rename_error(&io::Error::new(
            io::ErrorKind::Interrupted,
            "interrupted canary",
        )));
        assert!(!is_transient_publish_rename_error(&io::Error::new(
            io::ErrorKind::AlreadyExists,
            "deterministic canary",
        )));

        let permission = io::Error::new(io::ErrorKind::PermissionDenied, "permission canary");
        #[cfg(windows)]
        assert!(is_transient_publish_rename_error(&permission));
        #[cfg(not(windows))]
        assert!(!is_transient_publish_rename_error(&permission));
    }

    #[test]
    fn rejects_traversal_and_symlink_inputs() {
        assert!(normalize_relative_path(Path::new("../outside")).is_err());
        assert!(validate_safe_segment("bad/id").is_err());
    }

    #[test]
    fn image_processing_provenance_enforces_processor_specific_metadata() {
        let simple = ImageProcessingProvenance::simple();
        validate_image_processing(&simple).unwrap();

        let mut invalid_simple = simple.clone();
        invalid_simple.model_sha256 = Some(sha256_bytes(b"model"));
        assert!(validate_image_processing(&invalid_simple).is_err());

        let mut fallback = simple;
        fallback.fallback = Some(ImageProcFallback {
            from: ImageProcessor::MlU2netp,
            reason: ImageProcFallbackReason::Runtime,
        });
        validate_image_processing(&fallback).unwrap();

        let invalid_ml = ImageProcessingProvenance {
            processor: ImageProcessor::MlU2netp,
            model_sha256: None,
            runtime_version: None,
            fallback: None,
            build: crate::build_info::BuildInfo::current(),
        };
        assert!(validate_image_processing(&invalid_ml).is_err());
    }

    #[test]
    fn rejects_non_directory_artifact_ancestor() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(&project).unwrap();
        let source = project.join("Demo.cs");
        fs::write(&source, b"class Demo {}").unwrap();
        fs::write(project.join("artifacts"), b"not a directory").unwrap();
        let store = ArtifactStore::new(project);
        let error = store
            .publish(ArtifactPublishRequest {
                artifact_id: "Demo".into(),
                artifact_kind: "code".into(),
                run_id: RunId::new(),
                game_context: ArtifactGameContext {
                    game_pack_id: "fixture".into(),
                    game_pack_schema_version: 1,
                    game_pack_sha256: sha256_bytes(b"pack"),
                    snapshot_id: sha256_bytes(b"snapshot"),
                    snapshot_schema_version: 1,
                    sources: Vec::new(),
                    indexes: Vec::new(),
                    tool_versions: BTreeMap::new(),
                },
                evidence: Vec::new(),
                generation: ArtifactGeneration {
                    provider: "fixture".into(),
                    model: "fixture".into(),
                    inputs_sha256: sha256_bytes(b"input"),
                },
                image_processing: None,
                files: vec![ArtifactFileInput {
                    role: "csharp".into(),
                    source_path: source,
                    published_relative_path: Some("Demo.cs".into()),
                }],
            })
            .unwrap_err();
        assert!(matches!(error, ArtifactError::NotRegularFile(_)));
    }
}
