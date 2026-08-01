//! Staging and atomic activation for immutable truth snapshots.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::cancellation::CancellationToken;
use crate::fs_atomic::write_atomic_sync;

use super::error::{TruthSnapshotError, TruthSnapshotResult};
use super::hash::{
    copy_and_digest, copy_and_digest_cancellable, digest_file, digest_file_cancellable,
    digest_tree, digest_tree_cancellable, sha256_bytes,
};
use super::model::{
    TRUTH_SNAPSHOT_SCHEMA_VERSION, TruthSnapshotIdentity, TruthSnapshotIndex,
    TruthSnapshotManifest, TruthSnapshotSource, VerifiedTruthSnapshot,
};
use crate::game_pack::{LoadedGamePack, TruthSource, TruthSourceKind};

const MANIFEST_FILE: &str = "snapshot.json";
const CURRENT_FILE: &str = "current.json";
static DRAFT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct TruthSnapshotStore {
    root: PathBuf,
    game_pack_id: String,
}

impl TruthSnapshotStore {
    /// Create a store rooted at `runtime/game-packs/<pack-id>`.
    #[must_use]
    pub fn new(runtime_dir: &Path, pack: &LoadedGamePack) -> Self {
        Self {
            root: runtime_dir.join("game-packs").join(&pack.id),
            game_pack_id: pack.id.clone(),
        }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn begin(&self, pack: &LoadedGamePack) -> TruthSnapshotResult<TruthSnapshotDraft> {
        self.ensure_pack(pack)?;
        let staging_root = self.root.join(".staging");
        let snapshots_root = self.root.join("snapshots");
        create_dir_all(&staging_root, "create truth snapshot staging root")?;
        create_dir_all(&snapshots_root, "create truth snapshot directory")?;
        let nonce = DRAFT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let staging_dir = staging_root.join(format!("{}-{nonce}", std::process::id()));
        fs::create_dir(&staging_dir).map_err(|source| TruthSnapshotError::Io {
            action: "create truth snapshot draft",
            path: staging_dir.clone(),
            source,
        })?;
        Ok(TruthSnapshotDraft {
            store: self.clone(),
            pack: pack.clone(),
            staging_dir,
            sources: BTreeMap::new(),
            cleanup_staging: true,
        })
    }

    pub fn open_current(
        &self,
        pack: &LoadedGamePack,
    ) -> TruthSnapshotResult<Option<VerifiedTruthSnapshot>> {
        self.open_current_inner(pack, None)
    }

    pub(crate) fn open_current_cancellable(
        &self,
        pack: &LoadedGamePack,
        cancellation: &CancellationToken,
    ) -> TruthSnapshotResult<Option<VerifiedTruthSnapshot>> {
        self.open_current_inner(pack, Some(cancellation))
    }

    fn open_current_inner(
        &self,
        pack: &LoadedGamePack,
        cancellation: Option<&CancellationToken>,
    ) -> TruthSnapshotResult<Option<VerifiedTruthSnapshot>> {
        check_cancelled(cancellation)?;
        self.ensure_pack(pack)?;
        let current_path = self.root.join(CURRENT_FILE);
        let bytes = match fs::read(&current_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(TruthSnapshotError::Io {
                    action: "read current truth snapshot pointer",
                    path: current_path,
                    source,
                });
            }
        };
        let pointer: CurrentSnapshotPointer =
            serde_json::from_slice(&bytes).map_err(|error| TruthSnapshotError::Json {
                path: current_path,
                message: error.to_string(),
            })?;
        if pointer.schema_version != TRUTH_SNAPSHOT_SCHEMA_VERSION {
            return Err(TruthSnapshotError::InvalidManifest(format!(
                "current pointer schema {} is unsupported",
                pointer.schema_version
            )));
        }
        validate_snapshot_id(&pointer.snapshot_id)?;
        let snapshot_root = self.root.join("snapshots").join(&pointer.snapshot_id);
        verify_snapshot_root(
            pack,
            &snapshot_root,
            &pointer.snapshot_id,
            Some(&pointer.manifest_sha256),
            cancellation,
        )
        .map(Some)
    }

    pub fn open_snapshot(
        &self,
        pack: &LoadedGamePack,
        snapshot_id: &str,
    ) -> TruthSnapshotResult<VerifiedTruthSnapshot> {
        self.ensure_pack(pack)?;
        validate_snapshot_id(snapshot_id)?;
        let snapshot_root = self.root.join("snapshots").join(snapshot_id);
        verify_snapshot_root(pack, &snapshot_root, snapshot_id, None, None)
    }

    fn ensure_pack(&self, pack: &LoadedGamePack) -> TruthSnapshotResult<()> {
        if pack.id == self.game_pack_id {
            Ok(())
        } else {
            Err(TruthSnapshotError::PackMismatch(format!(
                "store is for `{}`, pack is `{}`",
                self.game_pack_id, pack.id
            )))
        }
    }
}

#[derive(Debug)]
pub struct TruthSnapshotDraft {
    store: TruthSnapshotStore,
    pack: LoadedGamePack,
    staging_dir: PathBuf,
    sources: BTreeMap<String, TruthSnapshotSource>,
    cleanup_staging: bool,
}

impl TruthSnapshotDraft {
    #[must_use]
    pub fn staging_root(&self) -> &Path {
        &self.staging_dir
    }

    /// Copy a declared source into the draft. Indexers must consume the returned copy.
    pub fn stage_source(
        &mut self,
        source_id: &str,
        source_path: &Path,
    ) -> TruthSnapshotResult<PathBuf> {
        self.stage_source_inner(source_id, source_path, None)
    }

    pub(crate) fn stage_source_cancellable(
        &mut self,
        source_id: &str,
        source_path: &Path,
        cancellation: &CancellationToken,
    ) -> TruthSnapshotResult<PathBuf> {
        self.stage_source_inner(source_id, source_path, Some(cancellation))
    }

    fn stage_source_inner(
        &mut self,
        source_id: &str,
        source_path: &Path,
        cancellation: Option<&CancellationToken>,
    ) -> TruthSnapshotResult<PathBuf> {
        check_cancelled(cancellation)?;
        if self.sources.contains_key(source_id) {
            return Err(TruthSnapshotError::DuplicateSource(source_id.into()));
        }
        let declaration = self
            .pack
            .truth_sources
            .iter()
            .find(|source| source.id == source_id)
            .ok_or_else(|| TruthSnapshotError::UnknownSource(source_id.into()))?;
        let relative_path = PathBuf::from("sources").join(source_id).join("source.bin");
        let destination = self.staging_dir.join(&relative_path);
        let digest = match cancellation {
            Some(cancellation) => {
                copy_and_digest_cancellable(source_path, &destination, cancellation)?
            }
            None => copy_and_digest(source_path, &destination)?,
        };
        let (kind, version, expected_sha256) = source_identity(declaration);
        if let Some(expected) = expected_sha256
            && !digest.sha256.eq_ignore_ascii_case(expected)
        {
            return Err(TruthSnapshotError::SourceChecksumMismatch {
                source_id: source_id.into(),
                expected: expected.into(),
                actual: digest.sha256,
            });
        }
        let source = TruthSnapshotSource {
            id: source_id.into(),
            kind: kind.into(),
            version,
            relative_path: path_to_manifest(&relative_path)?,
            sha256: digest.sha256,
            size_bytes: digest.size_bytes,
        };
        self.sources.insert(source_id.into(), source);
        Ok(destination)
    }

    /// Return the fixed draft directory where the declared source's indexer writes output.
    pub fn index_output_dir(&self, source_id: &str) -> TruthSnapshotResult<PathBuf> {
        if !self.sources.contains_key(source_id) {
            return Err(TruthSnapshotError::MissingSource(source_id.into()));
        }
        let directory = self.staging_dir.join("indexes").join(source_id);
        create_dir_all(&directory, "create truth index output directory")?;
        Ok(directory)
    }

    pub fn finalize(
        self,
        tool_versions: BTreeMap<String, String>,
    ) -> TruthSnapshotResult<VerifiedTruthSnapshot> {
        self.prepare_inner(tool_versions, None)?
            .activate_inner(None)
    }

    pub(crate) fn prepare_cancellable(
        self,
        tool_versions: BTreeMap<String, String>,
        cancellation: &CancellationToken,
    ) -> TruthSnapshotResult<PreparedTruthSnapshot> {
        self.prepare_inner(tool_versions, Some(cancellation))
    }

    fn prepare_inner(
        mut self,
        tool_versions: BTreeMap<String, String>,
        cancellation: Option<&CancellationToken>,
    ) -> TruthSnapshotResult<PreparedTruthSnapshot> {
        check_cancelled(cancellation)?;
        validate_tool_versions(&tool_versions)?;
        let declared_ids: BTreeSet<&str> = self
            .pack
            .truth_sources
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        for source_id in &declared_ids {
            if !self.sources.contains_key(*source_id) {
                return Err(TruthSnapshotError::MissingSource((*source_id).into()));
            }
        }

        let mut sources: Vec<_> = self.sources.values().cloned().collect();
        sources.sort_by(|left, right| left.id.cmp(&right.id));
        let mut indexes = Vec::with_capacity(self.pack.truth_sources.len());
        for declaration in &self.pack.truth_sources {
            check_cancelled(cancellation)?;
            let relative_root = PathBuf::from("indexes").join(&declaration.id);
            let index_root = self.staging_dir.join(&relative_root);
            if !index_root.is_dir() {
                return Err(TruthSnapshotError::MissingIndex(declaration.id.clone()));
            }
            let digest = match cancellation {
                Some(cancellation) => digest_tree_cancellable(&index_root, cancellation)?,
                None => digest_tree(&index_root)?,
            };
            if digest.cs_file_count == 0 {
                return Err(TruthSnapshotError::EmptyIndex(declaration.id.clone()));
            }
            indexes.push(TruthSnapshotIndex {
                source_id: declaration.id.clone(),
                indexer: declaration.indexer.clone(),
                provider: declaration.provider.clone(),
                relative_root: path_to_manifest(&relative_root)?,
                file_count: digest.file_count,
                cs_file_count: digest.cs_file_count,
                total_bytes: digest.total_bytes,
                tree_sha256: digest.sha256,
            });
        }
        indexes.sort_by(|left, right| left.source_id.cmp(&right.source_id));

        let identity = TruthSnapshotIdentity {
            schema_version: TRUTH_SNAPSHOT_SCHEMA_VERSION,
            game_pack_id: &self.pack.id,
            game_pack_schema_version: self.pack.schema_version,
            game_pack_sha256: &self.pack.content_sha256,
            sources: &sources,
            indexes: &indexes,
            tool_versions: &tool_versions,
        };
        let identity_bytes = serde_json::to_vec(&identity).map_err(|error| {
            TruthSnapshotError::InvalidManifest(format!("serialize snapshot identity: {error}"))
        })?;
        let snapshot_id = sha256_bytes(&identity_bytes);
        let manifest = TruthSnapshotManifest {
            schema_version: TRUTH_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: snapshot_id.clone(),
            game_pack_id: self.pack.id.clone(),
            game_pack_schema_version: self.pack.schema_version,
            game_pack_sha256: self.pack.content_sha256.clone(),
            sources,
            indexes,
            tool_versions,
            created_at: Utc::now(),
        };
        let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
            TruthSnapshotError::InvalidManifest(format!("serialize snapshot manifest: {error}"))
        })?;
        manifest_bytes.push(b'\n');
        let manifest_path = self.staging_dir.join(MANIFEST_FILE);
        check_cancelled(cancellation)?;
        write_atomic_sync(&manifest_path, &manifest_bytes).map_err(|source| {
            TruthSnapshotError::Io {
                action: "write truth snapshot manifest",
                path: manifest_path,
                source,
            }
        })?;

        let target = self.store.root.join("snapshots").join(&snapshot_id);
        check_cancelled(cancellation)?;
        let verified = if target.exists() {
            let existing =
                verify_snapshot_root(&self.pack, &target, &snapshot_id, None, cancellation)?;
            fs::remove_dir_all(&self.staging_dir).map_err(|source| TruthSnapshotError::Io {
                action: "remove duplicate truth snapshot draft",
                path: self.staging_dir.clone(),
                source,
            })?;
            self.cleanup_staging = false;
            existing
        } else {
            fs::rename(&self.staging_dir, &target).map_err(|source| TruthSnapshotError::Io {
                action: "activate immutable truth snapshot directory",
                path: target.clone(),
                source,
            })?;
            self.cleanup_staging = false;
            verify_snapshot_root(&self.pack, &target, &snapshot_id, None, cancellation)?
        };

        Ok(PreparedTruthSnapshot {
            store: self.store.clone(),
            verified,
        })
    }
}

pub(crate) struct PreparedTruthSnapshot {
    store: TruthSnapshotStore,
    verified: VerifiedTruthSnapshot,
}

impl PreparedTruthSnapshot {
    pub(crate) fn activate_cancellable(
        self,
        cancellation: &CancellationToken,
    ) -> TruthSnapshotResult<VerifiedTruthSnapshot> {
        self.activate_inner(Some(cancellation))
    }

    fn activate_inner(
        self,
        cancellation: Option<&CancellationToken>,
    ) -> TruthSnapshotResult<VerifiedTruthSnapshot> {
        check_cancelled(cancellation)?;
        let snapshot_id = self.verified.snapshot_id().to_string();
        let target = self.verified.root();

        let stored_manifest =
            fs::read(target.join(MANIFEST_FILE)).map_err(|source| TruthSnapshotError::Io {
                action: "read activated truth snapshot manifest",
                path: target.join(MANIFEST_FILE),
                source,
            })?;
        let pointer = CurrentSnapshotPointer {
            schema_version: TRUTH_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id,
            manifest_sha256: sha256_bytes(&stored_manifest),
        };
        let mut pointer_bytes = serde_json::to_vec_pretty(&pointer).map_err(|error| {
            TruthSnapshotError::InvalidManifest(format!("serialize current pointer: {error}"))
        })?;
        pointer_bytes.push(b'\n');
        let current_path = self.store.root.join(CURRENT_FILE);
        check_cancelled(cancellation)?;
        write_atomic_sync(&current_path, &pointer_bytes).map_err(|source| {
            TruthSnapshotError::Io {
                action: "activate current truth snapshot pointer",
                path: current_path,
                source,
            }
        })?;
        Ok(self.verified)
    }
}

impl Drop for TruthSnapshotDraft {
    fn drop(&mut self) {
        if self.cleanup_staging {
            let _ = fs::remove_dir_all(&self.staging_dir);
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CurrentSnapshotPointer {
    schema_version: u32,
    snapshot_id: String,
    manifest_sha256: String,
}

fn verify_snapshot_root(
    pack: &LoadedGamePack,
    root: &Path,
    expected_snapshot_id: &str,
    expected_manifest_sha256: Option<&str>,
    cancellation: Option<&CancellationToken>,
) -> TruthSnapshotResult<VerifiedTruthSnapshot> {
    check_cancelled(cancellation)?;
    let manifest_path = root.join(MANIFEST_FILE);
    let bytes = fs::read(&manifest_path).map_err(|source| TruthSnapshotError::Io {
        action: "read truth snapshot manifest",
        path: manifest_path.clone(),
        source,
    })?;
    if let Some(expected) = expected_manifest_sha256 {
        let actual = sha256_bytes(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(TruthSnapshotError::InvalidManifest(format!(
                "manifest checksum mismatch: expected {expected}, got {actual}"
            )));
        }
    }
    let manifest: TruthSnapshotManifest =
        serde_json::from_slice(&bytes).map_err(|error| TruthSnapshotError::Json {
            path: manifest_path,
            message: error.to_string(),
        })?;
    validate_manifest_identity(pack, &manifest, expected_snapshot_id)?;
    validate_manifest_contents(pack, root, &manifest, cancellation)?;
    Ok(VerifiedTruthSnapshot::new(root.to_path_buf(), manifest))
}

fn validate_manifest_identity(
    pack: &LoadedGamePack,
    manifest: &TruthSnapshotManifest,
    expected_snapshot_id: &str,
) -> TruthSnapshotResult<()> {
    if manifest.schema_version != TRUTH_SNAPSHOT_SCHEMA_VERSION {
        return Err(TruthSnapshotError::InvalidManifest(format!(
            "unsupported schema version {}",
            manifest.schema_version
        )));
    }
    if manifest.game_pack_id != pack.id
        || manifest.game_pack_schema_version != pack.schema_version
        || manifest.game_pack_sha256 != pack.content_sha256
    {
        return Err(TruthSnapshotError::PackMismatch(format!(
            "manifest pack `{}` schema {} hash {}, expected `{}` schema {} hash {}",
            manifest.game_pack_id,
            manifest.game_pack_schema_version,
            manifest.game_pack_sha256,
            pack.id,
            pack.schema_version,
            pack.content_sha256
        )));
    }
    if manifest.snapshot_id != expected_snapshot_id {
        return Err(TruthSnapshotError::InvalidManifest(format!(
            "manifest snapshot id {} does not match directory {}",
            manifest.snapshot_id, expected_snapshot_id
        )));
    }
    validate_tool_versions(&manifest.tool_versions)?;
    let identity_bytes = serde_json::to_vec(&manifest.identity()).map_err(|error| {
        TruthSnapshotError::InvalidManifest(format!("serialize snapshot identity: {error}"))
    })?;
    let actual_id = sha256_bytes(&identity_bytes);
    if actual_id != manifest.snapshot_id {
        return Err(TruthSnapshotError::InvalidManifest(format!(
            "snapshot identity mismatch: expected {}, got {actual_id}",
            manifest.snapshot_id
        )));
    }
    Ok(())
}

fn validate_manifest_contents(
    pack: &LoadedGamePack,
    root: &Path,
    manifest: &TruthSnapshotManifest,
    cancellation: Option<&CancellationToken>,
) -> TruthSnapshotResult<()> {
    if manifest.sources.len() != pack.truth_sources.len()
        || manifest.indexes.len() != pack.truth_sources.len()
    {
        return Err(TruthSnapshotError::InvalidManifest(
            "source/index count does not match the game pack declaration".into(),
        ));
    }
    for declaration in &pack.truth_sources {
        check_cancelled(cancellation)?;
        let source = manifest
            .sources
            .iter()
            .find(|source| source.id == declaration.id)
            .ok_or_else(|| TruthSnapshotError::MissingSource(declaration.id.clone()))?;
        validate_source_declaration(declaration, source)?;
        let source_path = resolve_manifest_path(root, &source.relative_path)?;
        let digest = match cancellation {
            Some(cancellation) => digest_file_cancellable(&source_path, cancellation)?,
            None => digest_file(&source_path)?,
        };
        if digest.size_bytes != source.size_bytes || digest.sha256 != source.sha256 {
            return Err(TruthSnapshotError::SourceChecksumMismatch {
                source_id: source.id.clone(),
                expected: source.sha256.clone(),
                actual: digest.sha256,
            });
        }

        let index = manifest
            .indexes
            .iter()
            .find(|index| index.source_id == declaration.id)
            .ok_or_else(|| TruthSnapshotError::MissingIndex(declaration.id.clone()))?;
        if index.indexer != declaration.indexer || index.provider != declaration.provider {
            return Err(TruthSnapshotError::InvalidManifest(format!(
                "index declaration mismatch for source `{}`",
                declaration.id
            )));
        }
        let index_root = resolve_manifest_path(root, &index.relative_root)?;
        let digest = match cancellation {
            Some(cancellation) => digest_tree_cancellable(&index_root, cancellation)?,
            None => digest_tree(&index_root)?,
        };
        if digest.file_count != index.file_count
            || digest.cs_file_count != index.cs_file_count
            || digest.total_bytes != index.total_bytes
            || digest.sha256 != index.tree_sha256
        {
            return Err(TruthSnapshotError::IndexIntegrityMismatch {
                source_id: declaration.id.clone(),
                detail: format!(
                    "expected files={} cs={} bytes={} hash={}, got files={} cs={} bytes={} hash={}",
                    index.file_count,
                    index.cs_file_count,
                    index.total_bytes,
                    index.tree_sha256,
                    digest.file_count,
                    digest.cs_file_count,
                    digest.total_bytes,
                    digest.sha256
                ),
            });
        }
    }
    Ok(())
}

fn validate_source_declaration(
    declaration: &TruthSource,
    source: &TruthSnapshotSource,
) -> TruthSnapshotResult<()> {
    match &declaration.kind {
        TruthSourceKind::LocalFile { .. } => {
            if source.kind != "local_file" {
                return Err(TruthSnapshotError::InvalidManifest(format!(
                    "source `{}` kind mismatch",
                    declaration.id
                )));
            }
        }
        TruthSourceKind::GitHubReleaseAsset {
            pinned_release,
            sha256,
            ..
        } => {
            if source.kind != "github_release_asset"
                || source.version.as_deref() != Some(pinned_release)
                || !source.sha256.eq_ignore_ascii_case(sha256)
            {
                return Err(TruthSnapshotError::InvalidManifest(format!(
                    "remote source `{}` does not match pinned release and checksum",
                    declaration.id
                )));
            }
        }
    }
    Ok(())
}

fn source_identity(declaration: &TruthSource) -> (&'static str, Option<String>, Option<&str>) {
    match &declaration.kind {
        TruthSourceKind::LocalFile { .. } => ("local_file", None, None),
        TruthSourceKind::GitHubReleaseAsset {
            pinned_release,
            sha256,
            ..
        } => (
            "github_release_asset",
            Some(pinned_release.clone()),
            Some(sha256),
        ),
    }
}

fn validate_tool_versions(tool_versions: &BTreeMap<String, String>) -> TruthSnapshotResult<()> {
    if tool_versions.is_empty() {
        return Err(TruthSnapshotError::InvalidToolVersion {
            tool: "<none>".into(),
            reason: "at least one indexing tool version is required".into(),
        });
    }
    for (tool, version) in tool_versions {
        if tool.trim().is_empty() || version.trim().is_empty() {
            return Err(TruthSnapshotError::InvalidToolVersion {
                tool: tool.clone(),
                reason: "tool name and version must not be empty".into(),
            });
        }
    }
    Ok(())
}

fn validate_snapshot_id(snapshot_id: &str) -> TruthSnapshotResult<()> {
    if snapshot_id.len() == 64 && snapshot_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(TruthSnapshotError::InvalidSnapshotId(snapshot_id.into()))
    }
}

fn resolve_manifest_path(root: &Path, relative: &str) -> TruthSnapshotResult<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(TruthSnapshotError::PathOutsideRoot {
            root: root.to_path_buf(),
            relative: relative.to_path_buf(),
        });
    }
    let canonical_root = fs::canonicalize(root).map_err(|source| TruthSnapshotError::Io {
        action: "resolve truth snapshot root",
        path: root.to_path_buf(),
        source,
    })?;
    let candidate = canonical_root.join(relative);
    let resolved = fs::canonicalize(&candidate).map_err(|source| TruthSnapshotError::Io {
        action: "resolve truth snapshot path",
        path: candidate,
        source,
    })?;
    if !resolved.starts_with(&canonical_root) {
        return Err(TruthSnapshotError::PathOutsideRoot {
            root: canonical_root,
            relative: relative.to_path_buf(),
        });
    }
    Ok(resolved)
}

fn path_to_manifest(path: &Path) -> TruthSnapshotResult<String> {
    path.to_str()
        .map(|path| path.replace('\\', "/"))
        .ok_or_else(|| TruthSnapshotError::InvalidManifest("snapshot paths must be UTF-8".into()))
}

fn create_dir_all(path: &Path, action: &'static str) -> TruthSnapshotResult<()> {
    fs::create_dir_all(path).map_err(|source| TruthSnapshotError::Io {
        action,
        path: path.to_path_buf(),
        source,
    })
}

fn check_cancelled(cancellation: Option<&CancellationToken>) -> TruthSnapshotResult<()> {
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        Err(TruthSnapshotError::Cancelled)
    } else {
        Ok(())
    }
}
