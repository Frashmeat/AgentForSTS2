use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ats_game_context::{
    LoadedGamePack, TruthEvidenceRecord, TruthSnapshotManifest, TruthSnapshotRepository,
    TruthStoreError, VerifiedTruthSnapshot,
};
use ats_kernel::Sha256Digest;
use serde::Deserialize;
use sha2::{Digest, Sha256};

const MANIFEST_FILE: &str = "truth-snapshot.json";

#[derive(Debug, Clone)]
pub struct FileTruthSnapshotRepository {
    runtime_root: PathBuf,
}

impl FileTruthSnapshotRepository {
    #[must_use]
    pub fn new(runtime_root: PathBuf) -> Self {
        Self { runtime_root }
    }
}

impl TruthSnapshotRepository for FileTruthSnapshotRepository {
    fn open_current(
        &self,
        pack: &LoadedGamePack,
    ) -> Result<Option<VerifiedTruthSnapshot>, TruthStoreError> {
        if !validate_optional_directory(&self.runtime_root)? {
            return Ok(None);
        }
        let truth_root = self.runtime_root.join("truth");
        if !validate_optional_directory(&truth_root)? {
            return Ok(None);
        }
        let pack_root = truth_root.join(pack.id().as_str());
        if !validate_optional_directory(&pack_root)? {
            return Ok(None);
        }
        let pointer_path = pack_root.join("current.json");
        let pointer_bytes = match read_regular_file(&pack_root, &pointer_path, "read_current") {
            Ok(bytes) => bytes,
            Err(TruthStoreError::Io {
                kind: io::ErrorKind::NotFound,
                ..
            }) => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct CurrentPointer {
            schema_version: u32,
            snapshot_id: Sha256Digest,
        }
        let pointer: CurrentPointer =
            serde_json::from_slice(&pointer_bytes).map_err(TruthStoreError::Json)?;
        if pointer.schema_version != 1 {
            return Err(TruthStoreError::UnsupportedSchema);
        }

        let snapshot_root = pack_root
            .join("snapshots")
            .join(pointer.snapshot_id.as_str());
        validate_directory_chain(&pack_root, &snapshot_root)?;
        let manifest_bytes = read_regular_file(
            &snapshot_root,
            &snapshot_root.join(MANIFEST_FILE),
            "read_manifest",
        )?;
        let manifest: TruthSnapshotManifest =
            serde_json::from_slice(&manifest_bytes).map_err(TruthStoreError::Json)?;
        if manifest.snapshot_id() != &pointer.snapshot_id {
            return Err(TruthStoreError::SnapshotIdentityMismatch);
        }
        manifest.verify_for_pack(pack)?;

        for source in manifest.sources() {
            let path = snapshot_root.join(&source.relative_path);
            let bytes = read_regular_file(&snapshot_root, &path, "read_source")?;
            if u64::try_from(bytes.len()).ok() != Some(source.byte_length)
                || sha256_bytes(&bytes) != source.sha256
            {
                return Err(TruthStoreError::HashMismatch);
            }
        }

        let mut evidence_by_index = BTreeMap::new();
        for index in manifest.indexes() {
            let path = snapshot_root.join(&index.relative_path);
            let bytes = read_regular_file(&snapshot_root, &path, "read_index")?;
            if sha256_bytes(&bytes) != index.sha256 {
                return Err(TruthStoreError::HashMismatch);
            }
            let records: Vec<TruthEvidenceRecord> =
                serde_json::from_slice(&bytes).map_err(TruthStoreError::Json)?;
            if u32::try_from(records.len()).ok() != Some(index.record_count) {
                return Err(TruthStoreError::InvalidEvidence);
            }
            evidence_by_index.insert(index.id.clone(), records);
        }
        VerifiedTruthSnapshot::verify(pack, manifest, evidence_by_index).map(Some)
    }
}

fn read_regular_file(
    trusted_root: &Path,
    path: &Path,
    operation: &'static str,
) -> Result<Vec<u8>, TruthStoreError> {
    validate_path_chain(trusted_root, path)?;
    fs::read(path).map_err(|source| io_error(operation, source))
}

fn validate_directory_chain(root: &Path, path: &Path) -> Result<(), TruthStoreError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| TruthStoreError::PathInvalid)?;
    let mut current = root.to_path_buf();
    validate_directory(&current)?;
    for component in relative.components() {
        current.push(component.as_os_str());
        validate_directory(&current)?;
    }
    Ok(())
}

fn validate_path_chain(root: &Path, path: &Path) -> Result<(), TruthStoreError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| TruthStoreError::PathInvalid)?;
    let mut current = root.to_path_buf();
    validate_directory(&current)?;
    let components: Vec<_> = relative.components().collect();
    for (index, component) in components.iter().enumerate() {
        current.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&current)
            .map_err(|source| io_error("inspect_snapshot_path", source))?;
        if metadata.file_type().is_symlink()
            || (index + 1 < components.len() && !metadata.is_dir())
            || (index + 1 == components.len() && !metadata.is_file())
        {
            return Err(TruthStoreError::PathInvalid);
        }
    }
    Ok(())
}

fn validate_directory(path: &Path) -> Result<(), TruthStoreError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|source| io_error("inspect_snapshot_directory", source))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(TruthStoreError::PathInvalid)
    }
}

fn validate_optional_directory(path: &Path) -> Result<bool, TruthStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(true),
        Ok(_) => Err(TruthStoreError::PathInvalid),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("inspect_snapshot_directory", error)),
    }
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter always returns a valid digest")
}

fn io_error(operation: &'static str, source: io::Error) -> TruthStoreError {
    TruthStoreError::Io {
        operation,
        kind: source.kind(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ats_game_context::{
        EvidenceQuery, GamePackId, GamePackLoader, GamePackRegistry, TruthSnapshotIndex,
        TruthSnapshotSource, VerifiedGameContext,
    };
    use ats_kernel::PrimitiveId;
    use chrono::Utc;

    use super::*;

    fn pack() -> LoadedGamePack {
        let json = br#"{"schemaVersion":4,"id":"fixture-game","displayName":"Fixture","itemTypes":[{"id":"fixture_item","displayNames":{"eng":"Fixture item"},"evidenceQueries":[{"symbols":["Fixture.Symbol"],"terms":[]}]}],"contributions":[]}"#;
        GamePackLoader::load(json, &sha256_bytes(json)).unwrap()
    }

    fn write_snapshot(runtime: &Path, pack: &LoadedGamePack) -> PathBuf {
        let records = vec![TruthEvidenceRecord {
            source_id: "game".into(),
            symbol: "Game.Start".into(),
            purpose: "lifecycle".into(),
            bounded_excerpt: "Game.Start initializes the fixture".into(),
            relative_path: "indexes/Game.cs".into(),
        }];
        let index_bytes = serde_json::to_vec(&records).unwrap();
        let source_bytes = b"fixture-game";
        let manifest = TruthSnapshotManifest::new(
            pack,
            vec![TruthSnapshotSource {
                id: "game".into(),
                kind: "local_file".into(),
                version: Some("1".into()),
                relative_path: "sources/game.bin".into(),
                sha256: sha256_bytes(source_bytes),
                byte_length: u64::try_from(source_bytes.len()).unwrap(),
            }],
            vec![TruthSnapshotIndex {
                id: "game-code".into(),
                provider: PrimitiveId::parse("truth.code-facts").unwrap(),
                relative_path: "indexes/game.json".into(),
                sha256: sha256_bytes(&index_bytes),
                record_count: 1,
            }],
            BTreeMap::from([("fixture-indexer".into(), "1".into())]),
            Utc::now(),
        )
        .unwrap();
        let pack_root = runtime.join("truth").join(pack.id().as_str());
        let snapshot_root = pack_root
            .join("snapshots")
            .join(manifest.snapshot_id().as_str());
        fs::create_dir_all(snapshot_root.join("sources")).unwrap();
        fs::create_dir_all(snapshot_root.join("indexes")).unwrap();
        fs::write(snapshot_root.join("sources/game.bin"), source_bytes).unwrap();
        fs::write(snapshot_root.join("indexes/game.json"), index_bytes).unwrap();
        fs::write(
            snapshot_root.join(MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(
            pack_root.join("current.json"),
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "snapshotId": manifest.snapshot_id(),
            }))
            .unwrap(),
        )
        .unwrap();
        snapshot_root
    }

    #[test]
    fn opens_only_fully_verified_current_snapshot_and_context() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = pack();
        write_snapshot(temp.path(), &pack);
        let id = pack.id().clone();
        let mut registry = GamePackRegistry::new();
        registry.register(pack).unwrap();
        let repository = FileTruthSnapshotRepository::new(temp.path().to_path_buf());
        let context = VerifiedGameContext::open_current(&registry, &repository, &id).unwrap();
        let evidence = context
            .snapshot()
            .query(&EvidenceQuery {
                symbols: vec!["game.start".into()],
                terms: vec![],
                limit: 5,
            })
            .unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            context.pack().id(),
            &GamePackId::parse("fixture-game").unwrap()
        );
    }

    #[test]
    fn rejects_tampered_source_and_index_hashes() {
        for relative in ["sources/game.bin", "indexes/game.json"] {
            let temp = tempfile::TempDir::new().unwrap();
            let pack = pack();
            let snapshot_root = write_snapshot(temp.path(), &pack);
            fs::write(snapshot_root.join(relative), b"tampered").unwrap();
            let repository = FileTruthSnapshotRepository::new(temp.path().to_path_buf());
            assert!(matches!(
                repository.open_current(&pack),
                Err(TruthStoreError::HashMismatch)
            ));
        }
    }

    #[cfg(windows)]
    #[test]
    fn rejects_symlinked_snapshot_files() {
        use std::os::windows::fs::symlink_file;

        let temp = tempfile::TempDir::new().unwrap();
        let pack = pack();
        let snapshot_root = write_snapshot(temp.path(), &pack);
        let source = snapshot_root.join("sources/game.bin");
        let target = snapshot_root.join("sources/target.bin");
        fs::rename(&source, &target).unwrap();
        if symlink_file(&target, &source).is_err() {
            return;
        }
        let repository = FileTruthSnapshotRepository::new(temp.path().to_path_buf());
        assert!(matches!(
            repository.open_current(&pack),
            Err(TruthStoreError::PathInvalid)
        ));
    }
}
