use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use ats_kernel::Sha256Digest;
use ats_runtime::{
    ARTIFACT_MANIFEST_SCHEMA_VERSION, ArtifactContractError, ArtifactFileRecord, ArtifactManifest,
    ArtifactPublishRequest, ArtifactPublisher, PublishedArtifact, normalize_relative_path,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use thiserror::Error;

const MANIFEST_FILE: &str = "artifact-manifest.json";
const RENAME_RETRY_DELAYS: [Duration; 5] = [
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(400),
    Duration::from_millis(800),
];

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum ArtifactStoreError {
    #[error("artifact contract validation failed")]
    Contract(#[source] ArtifactContractError),
    #[error("artifact path validation failed")]
    PathInvalid,
    #[error("artifact snapshot already exists")]
    SnapshotExists,
    #[error("artifact manifest serialization failed")]
    ManifestSerialization(#[source] serde_json::Error),
    #[error("artifact I/O failed during {operation}")]
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
        #[source]
        source: io::Error,
    },
}

impl ArtifactStoreError {
    #[must_use]
    pub fn io_kind(&self) -> Option<io::ErrorKind> {
        match self {
            Self::Io { kind, .. } => Some(*kind),
            _ => None,
        }
    }
}

impl From<ArtifactContractError> for ArtifactStoreError {
    fn from(error: ArtifactContractError) -> Self {
        Self::Contract(error)
    }
}

#[derive(Debug, Clone)]
pub struct FileArtifactStore {
    project_root: PathBuf,
}

impl FileArtifactStore {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    fn publish_with<R, S, C>(
        &self,
        request: ArtifactPublishRequest,
        mut rename: R,
        mut sleep: S,
        is_retryable: C,
    ) -> Result<PublishedArtifact, ArtifactStoreError>
    where
        R: FnMut(&Path, &Path) -> io::Result<()>,
        S: FnMut(Duration),
        C: Fn(&io::Error) -> bool,
    {
        validate_request_shape(&request)?;
        validate_existing_directory(&self.project_root)?;
        for input in &request.files {
            validate_source(&self.project_root, &input.source_path)?;
        }

        let artifacts_root = self.project_root.join("artifacts");
        ensure_child_directory(&self.project_root, &artifacts_root, "create_artifacts_root")?;
        let artifact_root = artifacts_root.join(&request.artifact_id);
        ensure_child_directory(&artifacts_root, &artifact_root, "create_artifact_root")?;
        let runs_root = artifact_root.join("runs");
        ensure_child_directory(&artifact_root, &runs_root, "create_runs_root")?;

        let final_dir = runs_root.join(request.producing_run_id.as_str());
        reject_existing_final(&final_dir)?;

        let staging_number = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let staging_dir = runs_root.join(format!(
            ".staging-{}-{}-{staging_number}",
            request.producing_run_id,
            std::process::id()
        ));
        create_new_directory(&staging_dir, "create_staging")?;

        let result = (|| {
            let files_dir = staging_dir.join("files");
            create_new_directory(&files_dir, "create_staging_files")?;
            let manifest = self.build_manifest(&staging_dir, &request)?;
            let manifest_bytes = serde_json::to_vec_pretty(&manifest)
                .map_err(ArtifactStoreError::ManifestSerialization)?;
            write_file(
                &staging_dir.join(MANIFEST_FILE),
                &manifest_bytes,
                "write_manifest",
            )?;

            rename_with_retry(
                &staging_dir,
                &final_dir,
                &mut rename,
                &mut sleep,
                &is_retryable,
            )?;

            Ok(PublishedArtifact {
                artifact_manifest_ref: format!(
                    "artifacts/{}/runs/{}/{MANIFEST_FILE}",
                    request.artifact_id, request.producing_run_id
                ),
                manifest_sha256: sha256_bytes(&manifest_bytes),
            })
        })();

        if result.is_err() {
            let _ = fs::remove_dir_all(&staging_dir);
        }
        result
    }

    fn build_manifest(
        &self,
        staging_dir: &Path,
        request: &ArtifactPublishRequest,
    ) -> Result<ArtifactManifest, ArtifactStoreError> {
        let mut records = Vec::with_capacity(request.files.len());
        for (index, input) in request.files.iter().enumerate() {
            let source = validate_source(&self.project_root, &input.source_path)?;
            let file_name = source
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .ok_or(ArtifactStoreError::PathInvalid)?;
            let snapshot_relative_path = format!("files/{index:03}-{file_name}");
            let bytes = read_file(&source, "read_source")?;
            write_file(
                &staging_dir.join(&snapshot_relative_path),
                &bytes,
                "write_snapshot",
            )?;
            let published_relative_path = input
                .published_relative_path
                .as_deref()
                .map(|path| normalize_relative_path(Path::new(path)))
                .transpose()?;
            records.push(ArtifactFileRecord {
                role: input.role.clone(),
                snapshot_relative_path,
                published_relative_path,
                byte_length: u64::try_from(bytes.len())
                    .map_err(|_| ArtifactStoreError::PathInvalid)?,
                sha256: sha256_bytes(&bytes),
            });
        }

        let manifest = ArtifactManifest {
            schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION,
            artifact_id: request.artifact_id.clone(),
            artifact_kind: request.artifact_kind.clone(),
            feature_id: request.feature_id.clone(),
            producing_run_id: request.producing_run_id.clone(),
            created_at: Utc::now(),
            contexts: request.contexts.clone(),
            provenance: request.provenance.clone(),
            feature_extension: request.feature_extension.clone(),
            files: records,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

impl ArtifactPublisher for FileArtifactStore {
    type Error = ArtifactStoreError;

    fn publish(&self, request: ArtifactPublishRequest) -> Result<PublishedArtifact, Self::Error> {
        self.publish_with(
            request,
            |from, to| fs::rename(from, to),
            thread::sleep,
            is_transient_rename_error,
        )
    }

    fn remove_published_run(
        &self,
        artifact_id: &str,
        run_id: &ats_runtime::RunId,
    ) -> Result<(), Self::Error> {
        validate_safe_segment(artifact_id)?;
        let run_root = self
            .project_root
            .join("artifacts")
            .join(artifact_id)
            .join("runs")
            .join(run_id.as_str());
        match fs::symlink_metadata(&run_root) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                Err(ArtifactStoreError::PathInvalid)
            }
            Ok(_) => fs::remove_dir_all(run_root)
                .map_err(|error| io_error("remove_published_run", error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error("inspect_published_run", error)),
        }
    }
}

fn validate_request_shape(request: &ArtifactPublishRequest) -> Result<(), ArtifactStoreError> {
    validate_safe_segment(&request.artifact_id)?;
    validate_safe_segment(&request.artifact_kind)?;
    if request.files.is_empty() {
        return Err(ArtifactContractError::EmptyFiles.into());
    }
    for input in &request.files {
        if input.role.is_empty()
            || input.role.len() > 128
            || !input.role.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err(ArtifactContractError::InvalidRole.into());
        }
        if let Some(path) = &input.published_relative_path {
            normalize_relative_path(Path::new(path))?;
        }
    }
    Ok(())
}

fn validate_safe_segment(value: &str) -> Result<(), ArtifactStoreError> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(ArtifactContractError::UnsafeIdentifier.into())
    }
}

fn validate_existing_directory(path: &Path) -> Result<(), ArtifactStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(ArtifactStoreError::PathInvalid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            Err(ArtifactStoreError::PathInvalid)
        }
        Err(error) => Err(io_error("inspect_project_root", error)),
    }
}

fn ensure_child_directory(
    parent: &Path,
    path: &Path,
    operation: &'static str,
) -> Result<(), ArtifactStoreError> {
    validate_existing_directory(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(ArtifactStoreError::PathInvalid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_new_directory(path, operation)
        }
        Err(error) => Err(io_error(operation, error)),
    }
}

fn create_new_directory(path: &Path, operation: &'static str) -> Result<(), ArtifactStoreError> {
    fs::create_dir(path).map_err(|error| {
        if error.kind() == io::ErrorKind::AlreadyExists {
            ArtifactStoreError::PathInvalid
        } else {
            io_error(operation, error)
        }
    })
}

fn reject_existing_final(path: &Path) -> Result<(), ArtifactStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(ArtifactStoreError::PathInvalid),
        Ok(_) => Err(ArtifactStoreError::SnapshotExists),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("inspect_final", error)),
    }
}

fn validate_source(project_root: &Path, source_path: &Path) -> Result<PathBuf, ArtifactStoreError> {
    let relative = source_path
        .strip_prefix(project_root)
        .map_err(|_| ArtifactStoreError::PathInvalid)?;
    normalize_relative_path(relative)?;

    let mut current = project_root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                ArtifactStoreError::PathInvalid
            } else {
                io_error("inspect_source", error)
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(ArtifactStoreError::PathInvalid);
        }
    }
    let metadata =
        fs::symlink_metadata(&current).map_err(|error| io_error("inspect_source", error))?;
    if !metadata.is_file() {
        return Err(ArtifactStoreError::PathInvalid);
    }
    Ok(current)
}

fn read_file(path: &Path, operation: &'static str) -> Result<Vec<u8>, ArtifactStoreError> {
    fs::read(path).map_err(|error| io_error(operation, error))
}

fn write_file(
    path: &Path,
    bytes: &[u8],
    operation: &'static str,
) -> Result<(), ArtifactStoreError> {
    fs::write(path, bytes).map_err(|error| io_error(operation, error))
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter always returns a valid digest")
}

fn rename_with_retry<R, S, C>(
    source: &Path,
    destination: &Path,
    rename: &mut R,
    sleep: &mut S,
    is_retryable: &C,
) -> Result<(), ArtifactStoreError>
where
    R: FnMut(&Path, &Path) -> io::Result<()>,
    S: FnMut(Duration),
    C: Fn(&io::Error) -> bool,
{
    let mut attempt = 0_usize;
    loop {
        match rename(source, destination) {
            Ok(()) => return Ok(()),
            Err(error) if is_retryable(&error) && attempt < RENAME_RETRY_DELAYS.len() => {
                sleep(RENAME_RETRY_DELAYS[attempt]);
                attempt += 1;
            }
            Err(error) => return Err(io_error("publish_snapshot", error)),
        }
    }
}

fn is_transient_rename_error(error: &io::Error) -> bool {
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

fn io_error(operation: &'static str, source: io::Error) -> ArtifactStoreError {
    ArtifactStoreError::Io {
        operation,
        kind: source.kind(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use ats_kernel::{FeatureId, SchemaId, SchemaRef, SchemaVersion};
    use ats_runtime::{ArtifactFileInput, RunId, VersionedPayload};
    use serde::Serialize;

    use super::*;

    #[derive(Serialize)]
    struct Fixture {
        value: String,
    }

    fn payload(id: &str) -> VersionedPayload {
        VersionedPayload::from_typed(
            SchemaRef {
                id: SchemaId::parse(id).unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
            &Fixture { value: "ok".into() },
        )
        .unwrap()
    }

    fn request(project: &Path, run_id: RunId) -> ArtifactPublishRequest {
        let source = project.join("Generated/Fixture.cs");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, b"class Fixture {}").unwrap();
        ArtifactPublishRequest {
            artifact_id: "Fixture".into(),
            artifact_kind: "code".into(),
            feature_id: FeatureId::parse("fixture.generate").unwrap(),
            producing_run_id: run_id,
            contexts: vec![payload("fixture.context")],
            provenance: vec![payload("fixture.provenance")],
            feature_extension: payload("fixture.extension"),
            files: vec![ArtifactFileInput {
                role: "csharp".into(),
                source_path: source,
                published_relative_path: Some("Generated/Fixture.cs".into()),
            }],
        }
    }

    #[test]
    fn publishes_manifest_and_recomputable_hashes_without_staging_residue() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let run_id = RunId::new();
        let store = FileArtifactStore::new(project.clone());

        let published = store.publish(request(&project, run_id.clone())).unwrap();
        let manifest_path = project.join(&published.artifact_manifest_ref);
        let manifest_bytes = fs::read(&manifest_path).unwrap();
        assert_eq!(sha256_bytes(&manifest_bytes), published.manifest_sha256);
        let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes).unwrap();
        let file = &manifest.files[0];
        let final_root = manifest_path.parent().unwrap();
        let snapshot = fs::read(final_root.join(&file.snapshot_relative_path)).unwrap();
        assert_eq!(u64::try_from(snapshot.len()).unwrap(), file.byte_length);
        assert_eq!(sha256_bytes(&snapshot), file.sha256);
        assert!(
            fs::read_dir(final_root.parent().unwrap())
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".staging-"))
        );

        let json = serde_json::to_value(manifest).unwrap();
        for forbidden in ["gameContext", "evidence", "generation", "imageProcessing"] {
            assert!(json.get(forbidden).is_none(), "found {forbidden}");
        }

        assert!(matches!(
            store.publish(request(&project, run_id)),
            Err(ArtifactStoreError::SnapshotExists)
        ));
    }

    #[test]
    fn rejects_empty_escaping_and_external_sources_before_publication() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let store = FileArtifactStore::new(project.clone());

        let mut empty = request(&project, RunId::new());
        empty.files.clear();
        assert!(matches!(
            store.publish(empty),
            Err(ArtifactStoreError::Contract(
                ArtifactContractError::EmptyFiles
            ))
        ));

        let mut escaping = request(&project, RunId::new());
        escaping.files[0].published_relative_path = Some("../outside".into());
        assert!(store.publish(escaping).is_err());

        let external = temp.path().join("external.cs");
        fs::write(&external, b"external").unwrap();
        let mut outside = request(&project, RunId::new());
        outside.files[0].source_path = external;
        assert!(matches!(
            store.publish(outside),
            Err(ArtifactStoreError::PathInvalid)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn rejects_symlink_source() {
        use std::os::windows::fs::symlink_file;

        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let target = project.join("target.cs");
        fs::write(&target, b"target").unwrap();
        let link = project.join("link.cs");
        if symlink_file(&target, &link).is_err() {
            return;
        }
        let store = FileArtifactStore::new(project.clone());
        let mut value = request(&project, RunId::new());
        value.files[0].source_path = link;
        assert!(matches!(
            store.publish(value),
            Err(ArtifactStoreError::PathInvalid)
        ));
    }

    #[test]
    fn transient_rename_retries_then_publishes_complete_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let store = FileArtifactStore::new(project.clone());
        let attempts = Cell::new(0_usize);
        let mut delays = Vec::new();

        let published = store
            .publish_with(
                request(&project, RunId::new()),
                |from, to| {
                    let next = attempts.get() + 1;
                    attempts.set(next);
                    if next <= 2 {
                        Err(io::Error::new(io::ErrorKind::PermissionDenied, "canary"))
                    } else {
                        fs::rename(from, to)
                    }
                },
                |delay| delays.push(delay),
                |error| error.kind() == io::ErrorKind::PermissionDenied,
            )
            .unwrap();

        assert_eq!(attempts.get(), 3);
        assert_eq!(delays, RENAME_RETRY_DELAYS[..2]);
        assert!(project.join(published.artifact_manifest_ref).is_file());
    }

    #[test]
    fn retry_exhaustion_and_deterministic_failure_clean_staging() {
        for (retryable, expected_attempts) in [(true, 6_usize), (false, 1_usize)] {
            let temp = tempfile::TempDir::new().unwrap();
            let project = temp.path().join("project");
            fs::create_dir(&project).unwrap();
            let store = FileArtifactStore::new(project.clone());
            let attempts = Cell::new(0_usize);

            let result = store.publish_with(
                request(&project, RunId::new()),
                |_, _| {
                    attempts.set(attempts.get() + 1);
                    Err(io::Error::new(io::ErrorKind::PermissionDenied, "canary"))
                },
                |_| {},
                |_| retryable,
            );

            assert!(result.is_err());
            assert_eq!(attempts.get(), expected_attempts);
            let runs = project.join("artifacts/Fixture/runs");
            assert!(fs::read_dir(runs).unwrap().next().is_none());
        }
    }
}
