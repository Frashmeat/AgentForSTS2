use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use ats_kernel::{ResourceId, Sha256Digest};
use ats_workspace::{
    ResourceAsset, ResourceBlob, ResourceBytesIngestRequest, ResourceDeriveRequest,
    ResourceRepository, ResourceVersion, ResourceVersionProvenance, WorkspaceError,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

const MANIFEST_FILE: &str = "resource-manifest.json";
static RESOURCE_COUNTER: AtomicU64 = AtomicU64::new(0);
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum ResourceStoreError {
    #[error("resource workspace contract failed")]
    Contract(#[source] WorkspaceError),
    #[error("resource workspace path validation failed")]
    PathInvalid,
    #[error("resource workspace JSON is invalid")]
    Json(#[source] serde_json::Error),
    #[error("resource workspace I/O failed during {operation}")]
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
        #[source]
        source: io::Error,
    },
    #[error("resource workspace lock is unavailable")]
    LockUnavailable,
}

impl From<WorkspaceError> for ResourceStoreError {
    fn from(error: WorkspaceError) -> Self {
        Self::Contract(error)
    }
}

#[derive(Debug, Clone)]
pub struct FileResourceRepository {
    project_root: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl FileResourceRepository {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            gate: Arc::new(Mutex::new(())),
        }
    }

    fn resources_root(&self) -> PathBuf {
        self.project_root.join(".ats").join("resources")
    }

    fn prepare_root(&self) -> Result<PathBuf, ResourceStoreError> {
        validate_directory(&self.project_root)?;
        let ats_root = self.project_root.join(".ats");
        ensure_directory(&ats_root, "create_ats_root")?;
        let resources_root = ats_root.join("resources");
        ensure_directory(&resources_root, "create_resources_root")?;
        Ok(resources_root)
    }

    fn load_unlocked(&self, resource_id: &ResourceId) -> Result<ResourceAsset, ResourceStoreError> {
        let resources_root = self.resources_root();
        validate_directory(&self.project_root)?;
        validate_directory(&self.project_root.join(".ats"))?;
        validate_directory(&resources_root)?;
        let asset_root = resources_root.join(resource_id.as_str());
        validate_directory(&asset_root).map_err(|error| match error {
            ResourceStoreError::Io {
                kind: io::ErrorKind::NotFound,
                ..
            } => WorkspaceError::ResourceNotFound.into(),
            other => other,
        })?;
        let manifest_path = asset_root.join(MANIFEST_FILE);
        let bytes = read_regular_file(&manifest_path, "read_resource_manifest")?;
        let asset: ResourceAsset =
            serde_json::from_slice(&bytes).map_err(ResourceStoreError::Json)?;
        if asset.resource_id() != resource_id {
            return Err(WorkspaceError::InvalidManifest.into());
        }
        for version in asset.versions() {
            let path = asset_root.join(&version.blob.relative_path);
            validate_descendant_file(&asset_root, &path)?;
            let blob = fs::read(&path).map_err(|error| io_error("read_resource_blob", error))?;
            if blob.is_empty()
                || u64::try_from(blob.len()).ok() != Some(version.blob.byte_length)
                || sha256_bytes(&blob) != version.blob.sha256
            {
                return Err(WorkspaceError::InvalidManifest.into());
            }
        }
        Ok(asset)
    }

    fn write_manifest_atomic(
        asset_root: &Path,
        asset: &ResourceAsset,
    ) -> Result<(), ResourceStoreError> {
        asset.validate()?;
        let bytes = serde_json::to_vec_pretty(asset).map_err(ResourceStoreError::Json)?;
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = asset_root.join(format!(
            ".{MANIFEST_FILE}.{}.{counter}.tmp",
            std::process::id()
        ));
        let result = (|| {
            fs::write(&temporary, bytes)
                .map_err(|error| io_error("write_resource_manifest", error))?;
            fs::rename(&temporary, asset_root.join(MANIFEST_FILE))
                .map_err(|error| io_error("replace_resource_manifest", error))
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    fn stage_ingest(
        &self,
        resources_root: &Path,
        request: ResourceBytesIngestRequest,
    ) -> Result<(ResourceAsset, PathBuf, PathBuf), ResourceStoreError> {
        let source_name = Path::new(&request.file_name);
        if source_name.file_name().and_then(|name| name.to_str())
            != Some(request.file_name.as_str())
            || request.media.bytes.is_empty()
            || request.media.bytes.len() > 64 * 1024 * 1024
        {
            return Err(ResourceStoreError::PathInvalid);
        }
        let digest = sha256_bytes(&request.media.bytes);
        let extension = safe_extension(source_name);
        let resource_id = new_resource_id();
        let blob_relative = format!("versions/{digest}/original.{extension}");
        let original = ResourceVersion {
            id: digest.clone(),
            parent_version: None,
            blob: ResourceBlob {
                relative_path: blob_relative.clone(),
                media_type: request.media.media_type,
                byte_length: u64::try_from(request.media.bytes.len())
                    .map_err(|_| ResourceStoreError::PathInvalid)?,
                sha256: digest,
                width: request.media.width,
                height: request.media.height,
                has_alpha: request.media.has_alpha,
            },
            provenance: request.provenance,
        };
        let asset = ResourceAsset::new(
            resource_id.clone(),
            request.logical_role,
            request.origin,
            original,
        )?;
        let final_root = resources_root.join(resource_id.as_str());
        if fs::symlink_metadata(&final_root).is_ok() {
            return Err(WorkspaceError::ImmutableConflict.into());
        }
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let staging_root = resources_root.join(format!(
            ".staging-{}-{}-{counter}",
            resource_id,
            std::process::id()
        ));
        create_new_directory(&staging_root, "create_resource_staging")?;
        let result = (|| {
            let blob_path = staging_root.join(&blob_relative);
            fs::create_dir_all(blob_path.parent().ok_or(ResourceStoreError::PathInvalid)?)
                .map_err(|error| io_error("create_resource_version", error))?;
            fs::write(&blob_path, &request.media.bytes)
                .map_err(|error| io_error("write_resource_blob", error))?;
            let manifest = serde_json::to_vec_pretty(&asset).map_err(ResourceStoreError::Json)?;
            fs::write(staging_root.join(MANIFEST_FILE), manifest)
                .map_err(|error| io_error("write_resource_manifest", error))?;
            Ok((asset, staging_root.clone(), final_root))
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(staging_root);
        }
        result
    }

    fn ingest_batch_unlocked(
        &self,
        requests: Vec<ResourceBytesIngestRequest>,
    ) -> Result<Vec<ResourceAsset>, ResourceStoreError> {
        if requests.is_empty() || requests.len() > 128 {
            return Err(ResourceStoreError::PathInvalid);
        }
        let resources_root = self.prepare_root()?;
        let mut staged = Vec::with_capacity(requests.len());
        for request in requests {
            match self.stage_ingest(&resources_root, request) {
                Ok(value) => staged.push(value),
                Err(error) => {
                    for (_, path, _) in staged {
                        let _ = fs::remove_dir_all(path);
                    }
                    return Err(error);
                }
            }
        }
        let mut published = Vec::new();
        for (_, staging, final_root) in &staged {
            if let Err(error) = fs::rename(staging, final_root) {
                for path in &published {
                    let _ = fs::remove_dir_all(path);
                }
                for (_, pending, _) in &staged {
                    let _ = fs::remove_dir_all(pending);
                }
                return Err(io_error("publish_resource_batch", error));
            }
            published.push(final_root.clone());
        }
        Ok(staged.into_iter().map(|(asset, _, _)| asset).collect())
    }
}

impl ResourceRepository for FileResourceRepository {
    type Error = ResourceStoreError;

    fn ingest_bytes(
        &self,
        request: ResourceBytesIngestRequest,
    ) -> Result<ResourceAsset, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ResourceStoreError::LockUnavailable)?;
        self.ingest_batch_unlocked(vec![request])?
            .pop()
            .ok_or(ResourceStoreError::PathInvalid)
    }

    fn ingest_batch(
        &self,
        requests: Vec<ResourceBytesIngestRequest>,
    ) -> Result<Vec<ResourceAsset>, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ResourceStoreError::LockUnavailable)?;
        self.ingest_batch_unlocked(requests)
    }

    fn add_version(&self, request: ResourceDeriveRequest) -> Result<ResourceAsset, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ResourceStoreError::LockUnavailable)?;
        let mut asset = self.load_unlocked(&request.resource_id)?;
        let bytes = request.media.bytes;
        let digest = sha256_bytes(&bytes);
        let extension = "png";
        let relative_path = format!("versions/{digest}/derived.{extension}");
        let version = ResourceVersion {
            id: digest.clone(),
            parent_version: Some(request.parent_version),
            blob: ResourceBlob {
                relative_path: relative_path.clone(),
                media_type: request.media.media_type,
                byte_length: u64::try_from(bytes.len())
                    .map_err(|_| ResourceStoreError::PathInvalid)?,
                sha256: digest.clone(),
                width: request.media.width,
                height: request.media.height,
                has_alpha: request.media.has_alpha,
            },
            provenance: ResourceVersionProvenance::Derived {
                source_role: request.source_role,
                source_version: request.source_version,
                transform: request.transform,
                transform_version: request.transform_version,
                parameters_sha256: request.parameters_sha256,
                game_pack_id: request.game_pack_id,
                game_pack_sha256: request.game_pack_sha256,
            },
        };
        if !asset.add_version(version)? {
            return Ok(asset);
        }

        let asset_root = self.resources_root().join(request.resource_id.as_str());
        let version_root = asset_root.join("versions").join(digest.as_str());
        if fs::symlink_metadata(&version_root).is_ok() {
            return Err(WorkspaceError::ImmutableConflict.into());
        }
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let staging_root = asset_root
            .join("versions")
            .join(format!(".staging-{}-{counter}", digest.as_str()));
        create_new_directory(&staging_root, "create_version_staging")?;
        let staged_file = staging_root.join(format!("derived.{extension}"));
        let result = (|| {
            fs::write(&staged_file, bytes)
                .map_err(|error| io_error("write_resource_version", error))?;
            fs::rename(&staging_root, &version_root)
                .map_err(|error| io_error("publish_resource_version", error))?;
            if let Err(error) = Self::write_manifest_atomic(&asset_root, &asset) {
                let _ = fs::remove_dir_all(&version_root);
                return Err(error);
            }
            Ok(asset)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(staging_root);
        }
        result
    }

    fn select(
        &self,
        resource_id: &ResourceId,
        version: &Sha256Digest,
    ) -> Result<ResourceAsset, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ResourceStoreError::LockUnavailable)?;
        let mut asset = self.load_unlocked(resource_id)?;
        asset.select(version)?;
        let asset_root = self.resources_root().join(resource_id.as_str());
        Self::write_manifest_atomic(&asset_root, &asset)?;
        Ok(asset)
    }

    fn load(&self, resource_id: &ResourceId) -> Result<ResourceAsset, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ResourceStoreError::LockUnavailable)?;
        self.load_unlocked(resource_id)
    }

    fn read_selected_bytes(
        &self,
        resource_id: &ResourceId,
        selected_version: &Sha256Digest,
    ) -> Result<Vec<u8>, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ResourceStoreError::LockUnavailable)?;
        let asset = self.load_unlocked(resource_id)?;
        if asset.selected_version() != Some(selected_version) {
            return Err(WorkspaceError::VersionNotFound.into());
        }
        let path = self.resources_root().join(resource_id.as_str()).join(
            &asset
                .selected()
                .ok_or(WorkspaceError::VersionNotFound)?
                .blob
                .relative_path,
        );
        read_regular_file(&path, "read_selected_resource")
    }

    fn read_version_bytes(
        &self,
        resource_id: &ResourceId,
        version: &Sha256Digest,
    ) -> Result<Vec<u8>, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ResourceStoreError::LockUnavailable)?;
        let asset = self.load_unlocked(resource_id)?;
        let candidate = asset
            .versions()
            .iter()
            .find(|candidate| &candidate.id == version)
            .ok_or(WorkspaceError::VersionNotFound)?;
        let path = self
            .resources_root()
            .join(resource_id.as_str())
            .join(&candidate.blob.relative_path);
        read_regular_file(&path, "read_resource_version")
    }

    fn list(&self) -> Result<Vec<ResourceAsset>, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ResourceStoreError::LockUnavailable)?;
        let resources_root = self.prepare_root()?;
        let mut assets = Vec::new();
        for entry in
            fs::read_dir(resources_root).map_err(|error| io_error("list_resources", error))?
        {
            let entry = entry.map_err(|error| io_error("list_resources", error))?;
            let name = entry
                .file_name()
                .to_str()
                .ok_or(ResourceStoreError::PathInvalid)?
                .to_owned();
            if name.starts_with(".staging-")
                || !entry
                    .file_type()
                    .map_err(|error| io_error("inspect_resource", error))?
                    .is_dir()
            {
                return Err(ResourceStoreError::PathInvalid);
            }
            let id = ResourceId::parse(name).map_err(|_| ResourceStoreError::PathInvalid)?;
            assets.push(self.load_unlocked(&id)?);
        }
        assets.sort_by(|left, right| left.resource_id().cmp(right.resource_id()));
        Ok(assets)
    }
}

fn new_resource_id() -> ResourceId {
    let counter = RESOURCE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    ResourceId::parse(format!("resource.r{nanos:032x}{counter:016x}"))
        .expect("generated resource ID is qualified lowercase ASCII")
}

fn safe_extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 12
                && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "bin".into())
}

fn validate_descendant_file(root: &Path, path: &Path) -> Result<(), ResourceStoreError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| ResourceStoreError::PathInvalid)?;
    let mut current = root.to_path_buf();
    validate_directory(&current)?;
    let components: Vec<_> = relative.components().collect();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)
            .map_err(|error| io_error("inspect_resource_path", error))?;
        if metadata.file_type().is_symlink()
            || (index + 1 < components.len() && !metadata.is_dir())
            || (index + 1 == components.len() && !metadata.is_file())
        {
            return Err(ResourceStoreError::PathInvalid);
        }
    }
    Ok(())
}

fn read_regular_file(path: &Path, operation: &'static str) -> Result<Vec<u8>, ResourceStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(operation, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ResourceStoreError::PathInvalid);
    }
    fs::read(path).map_err(|error| io_error(operation, error))
}

fn ensure_directory(path: &Path, operation: &'static str) -> Result<(), ResourceStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(ResourceStoreError::PathInvalid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_new_directory(path, operation)
        }
        Err(error) => Err(io_error(operation, error)),
    }
}

fn create_new_directory(path: &Path, operation: &'static str) -> Result<(), ResourceStoreError> {
    fs::create_dir(path).map_err(|error| io_error(operation, error))
}

fn validate_directory(path: &Path) -> Result<(), ResourceStoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| io_error("inspect_resource_directory", error))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(ResourceStoreError::PathInvalid)
    }
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter always returns a valid digest")
}

fn io_error(operation: &'static str, source: io::Error) -> ResourceStoreError {
    ResourceStoreError::Io {
        operation,
        kind: source.kind(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use ats_kernel::{ContributionId, GamePackId, PrimitiveId};
    use ats_workspace::ResourceOrigin;

    use super::*;

    fn candidate(
        logical_role: &str,
        origin: ResourceOrigin,
        bytes: &[u8],
    ) -> ResourceBytesIngestRequest {
        ResourceBytesIngestRequest {
            logical_role: logical_role.into(),
            origin,
            file_name: "fixture.png".into(),
            media: ats_workspace::PreparedResourceMedia {
                media_type: "image/png".into(),
                width: 1,
                height: 1,
                has_alpha: true,
                bytes: bytes.to_vec(),
            },
            provenance: ResourceVersionProvenance::Original,
        }
    }

    #[test]
    fn ingests_derives_selects_and_reloads_immutable_versions() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let source_root = temp.path().join("inputs");
        fs::create_dir(&source_root).unwrap();
        let repository = FileResourceRepository::new(project.clone());

        let original = repository
            .ingest_bytes(candidate(
                "relic.normal",
                ResourceOrigin::UserUpload,
                b"original-png",
            ))
            .unwrap();
        assert_eq!(original.selected_version(), None);
        let derived = repository
            .add_version(ResourceDeriveRequest {
                resource_id: original.resource_id().clone(),
                parent_version: original.versions()[0].id.clone(),
                transform: PrimitiveId::parse("image.outline").unwrap(),
                parameters_sha256: sha256_bytes(b"radius=4"),
                media: ats_workspace::PreparedResourceMedia {
                    media_type: "image/png".into(),
                    width: 1,
                    height: 1,
                    has_alpha: true,
                    bytes: b"derived-png".to_vec(),
                },
                source_role: "relic.master".into(),
                source_version: original.versions()[0].id.clone(),
                transform_version: 1,
                game_pack_id: GamePackId::parse("sts2").unwrap(),
                game_pack_sha256: sha256_bytes(b"pack"),
            })
            .unwrap();
        let derived_id = derived
            .versions()
            .iter()
            .find(|version| version.id != original.versions()[0].id)
            .unwrap()
            .id
            .clone();
        let selected = repository
            .select(original.resource_id(), &derived_id)
            .unwrap();
        assert_eq!(selected.selected_version(), Some(&derived_id));
        assert_eq!(repository.load(original.resource_id()).unwrap(), selected);
        assert_eq!(
            repository
                .read_selected_bytes(original.resource_id(), &derived_id)
                .unwrap(),
            b"derived-png"
        );
        assert!(matches!(
            repository.read_selected_bytes(original.resource_id(), &original.versions()[0].id),
            Err(ResourceStoreError::Contract(
                WorkspaceError::VersionNotFound
            ))
        ));

        let manifest = project
            .join(".ats/resources")
            .join(original.resource_id().as_str())
            .join(MANIFEST_FILE);
        let serialized = fs::read_to_string(manifest).unwrap();
        assert!(!serialized.contains(source_root.to_string_lossy().as_ref()));
        assert!(
            !fs::read_dir(project.join(".ats/resources"))
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".staging-"))
        );
    }

    #[test]
    fn all_resource_origins_use_the_same_file_repository() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let repository = FileResourceRepository::new(project);
        let origins = [
            ResourceOrigin::UserUpload,
            ResourceOrigin::AiGenerated {
                provider: PrimitiveId::parse("image.fixture-provider").unwrap(),
                model: "fixture".into(),
                request_sha256: sha256_bytes(b"request"),
            },
            ResourceOrigin::PackDefault {
                game_pack_id: GamePackId::parse("sts2").unwrap(),
                game_pack_sha256: sha256_bytes(b"pack"),
                contribution_slot: ContributionId::parse("resource.prepare.specs").unwrap(),
            },
        ];
        let mut version_ids = Vec::new();
        for origin in origins {
            let asset = repository
                .ingest_bytes(candidate("fixture.binary", origin.clone(), b"same-content"))
                .unwrap();
            assert_eq!(asset.origin(), &origin);
            assert_eq!(asset.selected_version(), None);
            version_ids.push(asset.versions()[0].id.clone());
        }
        assert!(version_ids.windows(2).all(|ids| ids[0] == ids[1]));
    }

    #[test]
    fn invalid_batch_candidate_rolls_back_every_staging_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let repository = FileResourceRepository::new(project.clone());
        let existing = repository
            .ingest_bytes(candidate(
                "relic.outline",
                ResourceOrigin::UserUpload,
                b"existing",
            ))
            .unwrap();
        let existing = repository
            .select(existing.resource_id(), &existing.versions()[0].id)
            .unwrap();
        let valid = candidate("relic.normal", ResourceOrigin::UserUpload, b"valid");
        let mut invalid = candidate("relic.big", ResourceOrigin::UserUpload, b"invalid");
        invalid.file_name = "nested/invalid.png".into();

        assert!(repository.ingest_batch(vec![valid, invalid]).is_err());
        let entries = fs::read_dir(project.join(".ats/resources"))
            .unwrap()
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert!(
            entries
                .iter()
                .all(|entry| !entry.file_name().to_string_lossy().starts_with(".staging-"))
        );
        assert_eq!(
            repository
                .load(existing.resource_id())
                .unwrap()
                .selected_version(),
            existing.selected_version()
        );
    }

    #[test]
    fn tampered_blob_and_unknown_selection_are_rejected_without_manifest_change() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let repository = FileResourceRepository::new(project.clone());
        let candidate_asset = repository
            .ingest_bytes(candidate(
                "card.normal",
                ResourceOrigin::UserUpload,
                b"source",
            ))
            .unwrap();
        let asset = repository
            .select(
                candidate_asset.resource_id(),
                &candidate_asset.versions()[0].id,
            )
            .unwrap();
        let manifest_path = project
            .join(".ats/resources")
            .join(asset.resource_id().as_str())
            .join(MANIFEST_FILE);
        let before = fs::read(&manifest_path).unwrap();
        assert!(
            repository
                .select(asset.resource_id(), &sha256_bytes(b"missing"))
                .is_err()
        );
        assert_eq!(fs::read(&manifest_path).unwrap(), before);

        let blob_path = manifest_path
            .parent()
            .unwrap()
            .join(&asset.selected().unwrap().blob.relative_path);
        fs::write(blob_path, b"tampered").unwrap();
        assert!(matches!(
            repository.load(asset.resource_id()),
            Err(ResourceStoreError::Contract(
                WorkspaceError::InvalidManifest
            ))
        ));
        assert!(
            repository
                .read_selected_bytes(asset.resource_id(), asset.selected_version().unwrap())
                .is_err()
        );
    }
}
