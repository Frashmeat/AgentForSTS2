use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ats_kernel::CompositionDraftId;
use ats_workspace::{CompositionDraft, CompositionDraftError, CompositionDraftRepository};
use serde::Serialize;
use thiserror::Error;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum CompositionDraftStoreError {
    #[error("composition Draft contract failed")]
    Contract(#[source] CompositionDraftError),
    #[error("composition Draft was not found")]
    NotFound,
    #[error("composition Draft revision changed")]
    Conflict,
    #[error("composition Draft workspace path is invalid")]
    PathInvalid,
    #[error("composition Draft workspace JSON is invalid")]
    Json(#[source] serde_json::Error),
    #[error("composition Draft workspace I/O failed during {operation}")]
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
        #[source]
        source: io::Error,
    },
    #[error("composition Draft workspace lock is unavailable")]
    LockUnavailable,
}

impl From<CompositionDraftError> for CompositionDraftStoreError {
    fn from(error: CompositionDraftError) -> Self {
        Self::Contract(error)
    }
}

#[derive(Debug)]
pub struct FileCompositionDraftRepository {
    project_root: PathBuf,
    gate: Mutex<()>,
}

impl FileCompositionDraftRepository {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            gate: Mutex::new(()),
        }
    }

    fn drafts_root(&self) -> PathBuf {
        self.project_root.join(".ats").join("composition-drafts-v1")
    }

    fn prepare_root(&self) -> Result<PathBuf, CompositionDraftStoreError> {
        validate_directory(&self.project_root, "validate_project_root")?;
        let ats_root = self.project_root.join(".ats");
        ensure_directory(&ats_root, "create_ats_root")?;
        let drafts_root = self.drafts_root();
        ensure_directory(&drafts_root, "create_drafts_root")?;
        cleanup_owned_temporary_files(&drafts_root)?;
        Ok(drafts_root)
    }

    fn path(&self, draft_id: &CompositionDraftId) -> PathBuf {
        self.drafts_root().join(format!("{draft_id}.json"))
    }

    fn load_unlocked(
        &self,
        draft_id: &CompositionDraftId,
    ) -> Result<CompositionDraft, CompositionDraftStoreError> {
        let path = self.path(draft_id);
        let bytes = read_regular_file(&path, "read_composition_draft").map_err(map_not_found)?;
        let draft: CompositionDraft =
            serde_json::from_slice(&bytes).map_err(CompositionDraftStoreError::Json)?;
        if &draft.draft_id != draft_id {
            return Err(CompositionDraftStoreError::PathInvalid);
        }
        Ok(draft)
    }
}

impl CompositionDraftRepository for FileCompositionDraftRepository {
    type Error = CompositionDraftStoreError;

    fn create(&self, draft: &CompositionDraft) -> Result<(), Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| CompositionDraftStoreError::LockUnavailable)?;
        draft.validate()?;
        if draft.revision != 1 {
            return Err(CompositionDraftStoreError::Conflict);
        }
        self.prepare_root()?;
        let bytes = serde_json::to_vec_pretty(draft).map_err(CompositionDraftStoreError::Json)?;
        write_new_synced(
            &self.path(&draft.draft_id),
            &bytes,
            "create_composition_draft",
        )
        .map_err(|error| match error {
            CompositionDraftStoreError::Io {
                kind: io::ErrorKind::AlreadyExists,
                ..
            } => CompositionDraftStoreError::Conflict,
            other => other,
        })
    }

    fn load(&self, draft_id: &CompositionDraftId) -> Result<CompositionDraft, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| CompositionDraftStoreError::LockUnavailable)?;
        self.prepare_root()?;
        self.load_unlocked(draft_id)
    }

    fn compare_and_set(
        &self,
        expected_revision: u64,
        next: &CompositionDraft,
    ) -> Result<(), Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| CompositionDraftStoreError::LockUnavailable)?;
        next.validate()?;
        self.prepare_root()?;
        let current = self.load_unlocked(&next.draft_id)?;
        let next_revision = expected_revision
            .checked_add(1)
            .ok_or(CompositionDraftStoreError::Conflict)?;
        if current.revision != expected_revision
            || next.revision != next_revision
            || next.game_pack_id != current.game_pack_id
            || next.game_pack_sha256 != current.game_pack_sha256
            || next.root_item_id != current.root_item_id
            || next.created_at != current.created_at
            || next.updated_at < current.updated_at
        {
            return Err(CompositionDraftStoreError::Conflict);
        }
        write_json_atomic(&self.path(&next.draft_id), next)
    }

    fn list(&self) -> Result<Vec<CompositionDraft>, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| CompositionDraftStoreError::LockUnavailable)?;
        let root = self.prepare_root()?;
        let mut drafts = Vec::new();
        for entry in
            fs::read_dir(root).map_err(|error| io_error("list_composition_drafts", error))?
        {
            let entry = entry.map_err(|error| io_error("list_composition_drafts", error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| io_error("inspect_composition_draft", error))?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(CompositionDraftStoreError::PathInvalid);
            }
            let name = entry
                .file_name()
                .to_str()
                .ok_or(CompositionDraftStoreError::PathInvalid)?
                .to_owned();
            let id = name
                .strip_suffix(".json")
                .ok_or(CompositionDraftStoreError::PathInvalid)
                .and_then(|value| {
                    CompositionDraftId::parse(value)
                        .map_err(|_| CompositionDraftStoreError::PathInvalid)
                })?;
            drafts.push(self.load_unlocked(&id)?);
        }
        drafts.sort_by(|left, right| left.draft_id.cmp(&right.draft_id));
        Ok(drafts)
    }
}

fn ensure_directory(
    path: &Path,
    operation: &'static str,
) -> Result<(), CompositionDraftStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            Ok(())
        }
        Ok(_) => Err(CompositionDraftStoreError::PathInvalid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| io_error(operation, error))?;
            validate_directory(path, operation)
        }
        Err(error) => Err(io_error(operation, error)),
    }
}

fn validate_directory(
    path: &Path,
    operation: &'static str,
) -> Result<(), CompositionDraftStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(operation, error))?;
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(CompositionDraftStoreError::PathInvalid)
    }
}

fn read_regular_file(
    path: &Path,
    operation: &'static str,
) -> Result<Vec<u8>, CompositionDraftStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(operation, error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(CompositionDraftStoreError::PathInvalid);
    }
    fs::read(path).map_err(|error| io_error(operation, error))
}

fn cleanup_owned_temporary_files(root: &Path) -> Result<(), CompositionDraftStoreError> {
    for entry in fs::read_dir(root).map_err(|error| io_error("list_draft_temporary", error))? {
        let entry = entry.map_err(|error| io_error("list_draft_temporary", error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') && name.contains(".json.") && name.ends_with(".tmp") {
            let file_type = entry
                .file_type()
                .map_err(|error| io_error("inspect_draft_temporary", error))?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(CompositionDraftStoreError::PathInvalid);
            }
            fs::remove_file(entry.path())
                .map_err(|error| io_error("remove_draft_temporary", error))?;
        }
    }
    Ok(())
}

fn write_new_synced(
    path: &Path,
    bytes: &[u8],
    operation: &'static str,
) -> Result<(), CompositionDraftStoreError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_error(operation, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_error(operation, error))
}

fn write_json_atomic(
    path: &Path,
    value: &impl Serialize,
) -> Result<(), CompositionDraftStoreError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(CompositionDraftStoreError::Json)?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(CompositionDraftStoreError::PathInvalid)?;
    let temporary = path.with_file_name(format!(".{name}.{}.{counter}.tmp", std::process::id()));
    let result = (|| {
        write_new_synced(&temporary, &bytes, "write_composition_draft")?;
        fs::rename(&temporary, path).map_err(|error| io_error("replace_composition_draft", error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn map_not_found(error: CompositionDraftStoreError) -> CompositionDraftStoreError {
    match error {
        CompositionDraftStoreError::Io {
            kind: io::ErrorKind::NotFound,
            ..
        } => CompositionDraftStoreError::NotFound,
        other => other,
    }
}

fn io_error(operation: &'static str, source: io::Error) -> CompositionDraftStoreError {
    CompositionDraftStoreError::Io {
        operation,
        kind: source.kind(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ats_kernel::{
        CompositionId, CompositionParameterId, CompositionProfileId, GamePackId, ItemId,
        ItemTypeId, Sha256Digest,
    };
    use ats_workspace::{
        CompositionDraftNode, CompositionDraftRepository, ItemCompositionProfile,
        ItemCompositionSource, ItemDefinition,
    };
    use chrono::{Duration, Utc};

    use super::*;

    fn draft() -> CompositionDraft {
        let root_id = ItemId::parse("fixture-root").unwrap();
        let profile = ItemCompositionProfile {
            composition_id: CompositionId::parse("fixture-suite").unwrap(),
            source: ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            },
            parameters: BTreeMap::from([(CompositionParameterId::parse("node_count").unwrap(), 1)]),
        };
        let mut definition =
            ItemDefinition::new(root_id.clone(), ItemTypeId::parse("fixture").unwrap());
        definition.composition_profile = Some(profile.clone());
        CompositionDraft::new(
            CompositionDraftId::parse("fixture-draft").unwrap(),
            GamePackId::parse("fixture-game").unwrap(),
            Sha256Digest::parse("a".repeat(64)).unwrap(),
            root_id.clone(),
            profile,
            BTreeMap::from([(
                root_id,
                CompositionDraftNode {
                    definition,
                    expected_current_definition_hash: None,
                },
            )]),
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn persists_drafts_with_revision_compare_and_set() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(".ats")).unwrap();
        let repository = FileCompositionDraftRepository::new(temp.path().to_path_buf());
        let first = draft();
        repository.create(&first).unwrap();
        assert_eq!(repository.load(&first.draft_id).unwrap(), first);

        let mut second = first.clone();
        second.revision = 2;
        second.updated_at += Duration::seconds(1);
        repository.compare_and_set(1, &second).unwrap();
        assert_eq!(repository.list().unwrap(), vec![second.clone()]);

        let temporary = temp
            .path()
            .join(".ats/composition-drafts-v1/.fixture-draft.json.1.1.tmp");
        fs::write(&temporary, b"partial").unwrap();
        assert_eq!(repository.list().unwrap(), vec![second.clone()]);
        assert!(!temporary.exists());

        let mut stale = second;
        stale.revision = 3;
        stale.updated_at += Duration::seconds(1);
        assert!(matches!(
            repository.compare_and_set(1, &stale),
            Err(CompositionDraftStoreError::Conflict)
        ));
    }
}
