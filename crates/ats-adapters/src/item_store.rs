use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ats_kernel::{ItemId, Sha256Digest};
use ats_workspace::{ItemDefinition, ItemDefinitionError, ItemRepository, StoredItemDefinition};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const POINTER_SCHEMA_VERSION: u32 = 1;
const CURRENT_FILE: &str = "current.json";
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum ItemStoreError {
    #[error("item definition contract failed")]
    Contract(#[source] ItemDefinitionError),
    #[error("item definition was not found")]
    NotFound,
    #[error("item identity cannot change type")]
    TypeConflict,
    #[error("item workspace path is invalid")]
    PathInvalid,
    #[error("item workspace JSON is invalid")]
    Json(#[source] serde_json::Error),
    #[error("item workspace I/O failed during {operation}")]
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
        #[source]
        source: io::Error,
    },
    #[error("item workspace lock is unavailable")]
    LockUnavailable,
}

impl From<ItemDefinitionError> for ItemStoreError {
    fn from(error: ItemDefinitionError) -> Self {
        Self::Contract(error)
    }
}

#[derive(Debug, Clone)]
pub struct FileItemRepository {
    project_root: PathBuf,
    gate: Arc<Mutex<()>>,
}

impl FileItemRepository {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            gate: Arc::new(Mutex::new(())),
        }
    }

    fn items_root(&self) -> PathBuf {
        self.project_root.join(".ats").join("items-v2")
    }

    fn prepare_root(&self) -> Result<PathBuf, ItemStoreError> {
        validate_directory(&self.project_root, "validate_project_root")?;
        let ats_root = self.project_root.join(".ats");
        ensure_directory(&ats_root, "create_ats_root")?;
        let items_root = self.items_root();
        ensure_directory(&items_root, "create_items_root")?;
        Ok(items_root)
    }

    fn load_version_unlocked(
        &self,
        item_id: &ItemId,
        definition_hash: &Sha256Digest,
    ) -> Result<StoredItemDefinition, ItemStoreError> {
        validate_directory(&self.project_root, "validate_project_root")?;
        validate_directory(&self.project_root.join(".ats"), "validate_ats_root")?;
        let items_root = self.items_root();
        validate_directory(&items_root, "validate_items_root")?;
        let item_root = items_root.join(item_id.as_str());
        validate_directory(&item_root, "validate_item_root").map_err(map_not_found)?;
        let definitions_root = item_root.join("definitions");
        validate_directory(&definitions_root, "validate_definitions_root")?;
        let path = definitions_root.join(format!("{definition_hash}.json"));
        let bytes = read_regular_file(&path, "read_item_definition").map_err(map_not_found)?;
        let definition: ItemDefinition =
            serde_json::from_slice(&bytes).map_err(ItemStoreError::Json)?;
        let stored = StoredItemDefinition {
            definition_hash: definition_hash.clone(),
            definition,
        };
        stored.validate()?;
        if &stored.definition.item_id != item_id {
            return Err(ItemStoreError::PathInvalid);
        }
        Ok(stored)
    }

    fn load_current_unlocked(
        &self,
        item_id: &ItemId,
    ) -> Result<StoredItemDefinition, ItemStoreError> {
        let item_root = self.items_root().join(item_id.as_str());
        validate_directory(&item_root, "validate_item_root").map_err(map_not_found)?;
        let pointer_bytes = read_regular_file(&item_root.join(CURRENT_FILE), "read_item_pointer")
            .map_err(map_not_found)?;
        let pointer: ItemPointer =
            serde_json::from_slice(&pointer_bytes).map_err(ItemStoreError::Json)?;
        pointer.validate(item_id)?;
        self.load_version_unlocked(item_id, &pointer.definition_hash)
    }

    fn save_unlocked(
        &self,
        definition: &ItemDefinition,
    ) -> Result<StoredItemDefinition, ItemStoreError> {
        let definition_hash = definition.definition_hash()?;
        let items_root = self.prepare_root()?;
        let item_root = items_root.join(definition.item_id.as_str());
        ensure_directory(&item_root, "create_item_root")?;
        let definitions_root = item_root.join("definitions");
        ensure_directory(&definitions_root, "create_definitions_root")?;

        match self.load_current_unlocked(&definition.item_id) {
            Ok(current) if current.definition.item_type != definition.item_type => {
                return Err(ItemStoreError::TypeConflict);
            }
            Ok(_) => {}
            Err(ItemStoreError::NotFound) => {
                self.ensure_existing_versions_match_type(definition)?;
            }
            Err(error) => return Err(error),
        }

        let stored = StoredItemDefinition {
            definition_hash: definition_hash.clone(),
            definition: definition.clone(),
        };
        stored.validate()?;
        let definition_path = definitions_root.join(format!("{definition_hash}.json"));
        let bytes = serde_json::to_vec_pretty(definition).map_err(ItemStoreError::Json)?;
        match write_new_synced(&definition_path, &bytes, "write_item_definition") {
            Ok(()) => {}
            Err(ItemStoreError::Io {
                kind: io::ErrorKind::AlreadyExists,
                ..
            }) => {
                let existing = self.load_version_unlocked(&definition.item_id, &definition_hash)?;
                if existing != stored {
                    return Err(ItemStoreError::PathInvalid);
                }
            }
            Err(error) => return Err(error),
        }

        let pointer = ItemPointer {
            schema_version: POINTER_SCHEMA_VERSION,
            item_id: definition.item_id.clone(),
            definition_hash,
        };
        write_json_atomic(&item_root.join(CURRENT_FILE), &pointer)?;
        Ok(stored)
    }

    fn ensure_existing_versions_match_type(
        &self,
        definition: &ItemDefinition,
    ) -> Result<(), ItemStoreError> {
        let definitions_root = self
            .items_root()
            .join(definition.item_id.as_str())
            .join("definitions");
        for entry in fs::read_dir(&definitions_root)
            .map_err(|error| io_error("list_item_versions", error))?
        {
            let entry = entry.map_err(|error| io_error("list_item_versions", error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| io_error("inspect_item_version", error))?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(ItemStoreError::PathInvalid);
            }
            let name = entry
                .file_name()
                .to_str()
                .ok_or(ItemStoreError::PathInvalid)?
                .to_owned();
            let hash = name
                .strip_suffix(".json")
                .ok_or(ItemStoreError::PathInvalid)
                .and_then(|value| {
                    Sha256Digest::parse(value).map_err(|_| ItemStoreError::PathInvalid)
                })?;
            let stored = self.load_version_unlocked(&definition.item_id, &hash)?;
            if stored.definition.item_type != definition.item_type {
                return Err(ItemStoreError::TypeConflict);
            }
        }
        Ok(())
    }
}

impl ItemRepository for FileItemRepository {
    type Error = ItemStoreError;

    fn save(&self, definition: &ItemDefinition) -> Result<StoredItemDefinition, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ItemStoreError::LockUnavailable)?;
        self.save_unlocked(definition)
    }

    fn load_current(&self, item_id: &ItemId) -> Result<StoredItemDefinition, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ItemStoreError::LockUnavailable)?;
        self.load_current_unlocked(item_id)
    }

    fn load_version(
        &self,
        item_id: &ItemId,
        definition_hash: &Sha256Digest,
    ) -> Result<StoredItemDefinition, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ItemStoreError::LockUnavailable)?;
        self.load_version_unlocked(item_id, definition_hash)
    }

    fn list_current(&self) -> Result<Vec<StoredItemDefinition>, Self::Error> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ItemStoreError::LockUnavailable)?;
        let items_root = self.prepare_root()?;
        let mut items = Vec::new();
        for entry in fs::read_dir(&items_root).map_err(|error| io_error("list_items", error))? {
            let entry = entry.map_err(|error| io_error("list_items", error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| io_error("inspect_item_entry", error))?;
            if !file_type.is_dir() || file_type.is_symlink() {
                return Err(ItemStoreError::PathInvalid);
            }
            let name = entry
                .file_name()
                .to_str()
                .ok_or(ItemStoreError::PathInvalid)?
                .to_owned();
            let item_id = ItemId::parse(name).map_err(|_| ItemStoreError::PathInvalid)?;
            items.push(self.load_current_unlocked(&item_id)?);
        }
        items.sort_by(|left, right| left.definition.item_id.cmp(&right.definition.item_id));
        Ok(items)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ItemPointer {
    schema_version: u32,
    item_id: ItemId,
    definition_hash: Sha256Digest,
}

impl ItemPointer {
    fn validate(&self, expected_item_id: &ItemId) -> Result<(), ItemStoreError> {
        if self.schema_version != POINTER_SCHEMA_VERSION || &self.item_id != expected_item_id {
            Err(ItemStoreError::PathInvalid)
        } else {
            Ok(())
        }
    }
}

fn ensure_directory(path: &Path, operation: &'static str) -> Result<(), ItemStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            Ok(())
        }
        Ok(_) => Err(ItemStoreError::PathInvalid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| io_error(operation, error))?;
            validate_directory(path, operation)
        }
        Err(error) => Err(io_error(operation, error)),
    }
}

fn validate_directory(path: &Path, operation: &'static str) -> Result<(), ItemStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(operation, error))?;
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(ItemStoreError::PathInvalid)
    }
}

fn read_regular_file(path: &Path, operation: &'static str) -> Result<Vec<u8>, ItemStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(operation, error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(ItemStoreError::PathInvalid);
    }
    fs::read(path).map_err(|error| io_error(operation, error))
}

fn write_new_synced(
    path: &Path,
    bytes: &[u8],
    operation: &'static str,
) -> Result<(), ItemStoreError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| io_error(operation, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_error(operation, error))
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<(), ItemStoreError> {
    let bytes = serde_json::to_vec_pretty(value).map_err(ItemStoreError::Json)?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(ItemStoreError::PathInvalid)?;
    let temporary = path.with_file_name(format!(".{name}.{}.{counter}.tmp", std::process::id()));
    let result = (|| {
        write_new_synced(&temporary, &bytes, "write_item_pointer")?;
        fs::rename(&temporary, path).map_err(|error| io_error("replace_item_pointer", error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn map_not_found(error: ItemStoreError) -> ItemStoreError {
    match error {
        ItemStoreError::Io {
            kind: io::ErrorKind::NotFound,
            ..
        } => ItemStoreError::NotFound,
        other => other,
    }
}

fn io_error(operation: &'static str, source: io::Error) -> ItemStoreError {
    ItemStoreError::Io {
        operation,
        kind: source.kind(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ats_kernel::{ItemFieldId, ItemTypeId};
    use ats_workspace::{ItemFieldValue, ItemRepository};

    use super::*;

    fn definition(item_id: &str, rarity: &str) -> ItemDefinition {
        let mut definition = ItemDefinition::new(
            ItemId::parse(item_id).unwrap(),
            ItemTypeId::parse("relic").unwrap(),
        );
        definition.canonical_fields = BTreeMap::from([(
            ItemFieldId::parse("rarity").unwrap(),
            ItemFieldValue::Choice(rarity.into()),
        )]);
        definition.behavior_intent = vec!["Produce one observable effect.".into()];
        definition
    }

    #[test]
    fn saves_immutable_versions_and_moves_only_the_current_pointer() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(".ats")).unwrap();
        let repository = FileItemRepository::new(temp.path().to_path_buf());
        let first = repository.save(&definition("fixture", "common")).unwrap();
        let second = repository.save(&definition("fixture", "rare")).unwrap();
        assert_ne!(first.definition_hash, second.definition_hash);
        assert_eq!(
            repository
                .load_current(&ItemId::parse("fixture").unwrap())
                .unwrap(),
            second
        );
        assert_eq!(
            repository
                .load_version(&ItemId::parse("fixture").unwrap(), &first.definition_hash)
                .unwrap(),
            first
        );
        assert_eq!(repository.list_current().unwrap().len(), 1);
        let residues = fs::read_dir(temp.path().join(".ats/items-v2/fixture"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .count();
        assert_eq!(residues, 0);
    }

    #[test]
    fn rejects_type_changes_and_tampered_definition_bytes() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(".ats")).unwrap();
        let repository = FileItemRepository::new(temp.path().to_path_buf());
        let stored = repository.save(&definition("fixture", "common")).unwrap();
        let mut changed_type = definition("fixture", "common");
        changed_type.item_type = ItemTypeId::parse("card").unwrap();
        assert!(matches!(
            repository.save(&changed_type),
            Err(ItemStoreError::TypeConflict)
        ));

        let path = temp
            .path()
            .join(".ats/items-v2/fixture/definitions")
            .join(format!("{}.json", stored.definition_hash));
        fs::write(path, b"{}").unwrap();
        assert!(
            repository
                .load_version(&ItemId::parse("fixture").unwrap(), &stored.definition_hash)
                .is_err()
        );
    }

    #[test]
    fn orphaned_snapshot_still_reserves_the_item_type() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(".ats")).unwrap();
        let repository = FileItemRepository::new(temp.path().to_path_buf());
        repository.save(&definition("fixture", "common")).unwrap();
        fs::remove_file(temp.path().join(".ats/items-v2/fixture/current.json")).unwrap();

        let mut changed_type = definition("fixture", "rare");
        changed_type.item_type = ItemTypeId::parse("card").unwrap();
        assert!(matches!(
            repository.save(&changed_type),
            Err(ItemStoreError::TypeConflict)
        ));
    }
}
