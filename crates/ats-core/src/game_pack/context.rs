//! Verified game identity and immutable truth snapshot bound for one run.

use std::collections::BTreeMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Serialize;
use thiserror::Error;

use super::{
    GamePackError, GamePackRegistry, LoadedGamePack, TruthSnapshotError, TruthSnapshotIndex,
    TruthSnapshotSource, TruthSnapshotStore, VerifiedTruthSnapshot,
};

#[derive(Debug, Error)]
pub enum GameContextError {
    #[error(transparent)]
    GamePack(#[from] GamePackError),
    #[error(transparent)]
    TruthSnapshot(#[from] TruthSnapshotError),
    #[error(
        "game pack `{game_pack_id}` has no verified current truth snapshot; refresh current game sources before generation"
    )]
    MissingCurrent { game_pack_id: String },
}

pub type GameContextResult<T> = Result<T, GameContextError>;

/// Pack and Snapshot identity that stays fixed for one complete generation run.
#[derive(Debug, Clone)]
pub struct VerifiedGameContext {
    pack: LoadedGamePack,
    snapshot: VerifiedTruthSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedGameContextEvidence<'a> {
    pub game_pack_id: &'a str,
    pub game_pack_display_name: &'a str,
    pub game_pack_schema_version: u32,
    pub game_pack_sha256: &'a str,
    pub snapshot_schema_version: u32,
    pub snapshot_id: &'a str,
    pub sources: &'a [TruthSnapshotSource],
    pub indexes: &'a [TruthSnapshotIndex],
    pub tool_versions: &'a BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
}

impl VerifiedGameContext {
    /// Resolve a registry-backed Pack and verify its atomically active Snapshot.
    pub fn open_current(
        runtime_dir: &Path,
        registry: &GamePackRegistry,
        game_pack_id: &str,
    ) -> GameContextResult<Self> {
        let pack = registry.require(game_pack_id)?.clone();
        let store = TruthSnapshotStore::new(runtime_dir, &pack);
        let snapshot =
            store
                .open_current(&pack)?
                .ok_or_else(|| GameContextError::MissingCurrent {
                    game_pack_id: game_pack_id.into(),
                })?;
        Ok(Self { pack, snapshot })
    }

    #[must_use]
    pub fn pack(&self) -> &LoadedGamePack {
        &self.pack
    }

    #[must_use]
    pub fn snapshot(&self) -> &VerifiedTruthSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn game_pack_id(&self) -> &str {
        &self.pack.id
    }

    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        self.snapshot.snapshot_id()
    }

    #[must_use]
    pub fn evidence(&self) -> VerifiedGameContextEvidence<'_> {
        let manifest = self.snapshot.manifest();
        VerifiedGameContextEvidence {
            game_pack_id: &self.pack.id,
            game_pack_display_name: &self.pack.display_name,
            game_pack_schema_version: self.pack.schema_version,
            game_pack_sha256: &self.pack.content_sha256,
            snapshot_schema_version: manifest.schema_version,
            snapshot_id: &manifest.snapshot_id,
            sources: &manifest.sources,
            indexes: &manifest.indexes,
            tool_versions: &manifest.tool_versions,
            created_at: manifest.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use super::*;
    use crate::game_pack::{GamePackLoadPolicy, GamePackLoader};

    #[test]
    fn missing_current_snapshot_is_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let registry = GamePackRegistry::built_in().unwrap();
        let error = VerifiedGameContext::open_current(temp.path(), &registry, "sts2").unwrap_err();
        assert!(matches!(
            error,
            GameContextError::MissingCurrent { game_pack_id } if game_pack_id == "sts2"
        ));
    }

    #[test]
    fn opens_registry_pack_and_verified_current_snapshot() {
        let temp = tempfile::TempDir::new().unwrap();
        let loader = GamePackLoader::new(GamePackLoadPolicy::new(
            ["truth_sources"],
            ["dotnet_project"],
            ["sts2_code_facts"],
        ));
        let pack = loader
            .load_str(
                "fixture",
                r#"{
                  "schema_version": 1,
                  "id": "fixture-game",
                  "display_name": "Fixture Game",
                  "capabilities": ["truth_sources"],
                  "truth_sources": [
                    {
                      "id": "game",
                      "kind": "local_file",
                      "input_key": "game_assembly",
                      "indexer": "dotnet_project",
                      "provider": "sts2_code_facts"
                    },
                    {
                      "id": "library",
                      "kind": "local_file",
                      "input_key": "library_assembly",
                      "indexer": "dotnet_project",
                      "provider": "sts2_code_facts"
                    }
                  ]
                }"#,
            )
            .unwrap();
        let raw = temp.path().join("raw");
        fs::create_dir_all(&raw).unwrap();
        let game = raw.join("game.dll");
        let library = raw.join("library.dll");
        fs::write(&game, b"fixture-game").unwrap();
        fs::write(&library, b"fixture-library").unwrap();
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        let mut draft = store.begin(&pack).unwrap();
        draft.stage_source("game", &game).unwrap();
        draft.stage_source("library", &library).unwrap();
        fs::write(
            draft.index_output_dir("game").unwrap().join("Game.cs"),
            "class Game {}",
        )
        .unwrap();
        fs::write(
            draft
                .index_output_dir("library")
                .unwrap()
                .join("Library.cs"),
            "class Library {}",
        )
        .unwrap();
        let snapshot = draft
            .finalize(BTreeMap::from([("fixture".into(), "1".into())]))
            .unwrap();

        // A registry always owns the accepted Pack bytes, so construct a fixture
        // registry through its loader rather than exposing a context constructor.
        let fixture_registry = GamePackRegistry::from_packs([pack]).unwrap();
        let context =
            VerifiedGameContext::open_current(temp.path(), &fixture_registry, "fixture-game")
                .unwrap();
        assert_eq!(context.game_pack_id(), "fixture-game");
        assert_eq!(context.snapshot_id(), snapshot.snapshot_id());
        assert_eq!(context.evidence().sources.len(), 2);
    }
}
