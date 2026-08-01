//! Immutable, content-addressed truth snapshots for a validated game pack.

mod error;
mod hash;
mod model;
mod refresh;
mod store;

pub use error::{TruthSnapshotError, TruthSnapshotResult};
pub use model::{
    TruthSnapshotIndex, TruthSnapshotManifest, TruthSnapshotSource, VerifiedTruthSnapshot,
};
pub use refresh::{
    GitHubReleaseAssetFetcher, IlspycmdTruthIndexer, RemoteTruthSourceFetcher,
    TruthSnapshotReadiness, TruthSnapshotRefreshError, TruthSnapshotRefreshOutcome,
    TruthSnapshotRefreshResult, TruthSnapshotRefresher, TruthSnapshotStatus, TruthSourceIndexer,
    TruthSourceOperationError, TruthSourceOperationResult, inspect_truth_snapshot,
    validate_truth_source_inputs,
};
pub use store::{TruthSnapshotDraft, TruthSnapshotStore};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::cancellation::CancellationToken;
    use crate::game_pack::{GamePackLoadPolicy, GamePackLoader, LoadedGamePack};
    use crate::platform::domain::CancellationReason;

    const GAME_BYTES: &[u8] = b"MZ-current-game-assembly";
    const BASELIB_BYTES: &[u8] = b"MZ-pinned-baselib-assembly";

    fn fixture_pack() -> LoadedGamePack {
        let baselib_sha = format!("{:x}", Sha256::digest(BASELIB_BYTES));
        let json = format!(
            r#"{{
              "schema_version": 1,
              "id": "fixture-game",
              "display_name": "Fixture Game",
              "capabilities": ["truth_sources"],
              "truth_sources": [
                {{
                  "id": "game",
                  "kind": "local_file",
                  "input_key": "game_assembly",
                  "indexer": "dotnet_project",
                  "provider": "fixture_game_facts"
                }},
                {{
                  "id": "baselib",
                  "kind": "github_release_asset",
                  "repository": "owner/repository",
                  "pinned_release": "v1.2.3",
                  "asset": "BaseLib.dll",
                  "sha256": "{baselib_sha}",
                  "indexer": "dotnet_file",
                  "provider": "fixture_baselib_facts"
                }}
              ]
            }}"#
        );
        GamePackLoader::new(GamePackLoadPolicy::new(
            ["truth_sources"],
            ["dotnet_project", "dotnet_file"],
            ["fixture_game_facts", "fixture_baselib_facts"],
        ))
        .load_str("fixture-pack", &json)
        .unwrap()
    }

    fn source_files(root: &Path, game_bytes: &[u8], baselib_bytes: &[u8]) -> (PathBuf, PathBuf) {
        fs::create_dir_all(root).unwrap();
        let game = root.join("game.dll");
        let baselib = root.join("BaseLib.dll");
        fs::write(&game, game_bytes).unwrap();
        fs::write(&baselib, baselib_bytes).unwrap();
        (game, baselib)
    }

    fn tools() -> BTreeMap<String, String> {
        BTreeMap::from([("ilspycmd".into(), "9.1.0.7988".into())])
    }

    fn complete_snapshot(
        store: &TruthSnapshotStore,
        pack: &LoadedGamePack,
        source_root: &Path,
        game_bytes: &[u8],
        game_index: &str,
    ) -> VerifiedTruthSnapshot {
        let (game, baselib) = source_files(source_root, game_bytes, BASELIB_BYTES);
        let mut draft = store.begin(pack).unwrap();
        let staged_game = draft.stage_source("game", &game).unwrap();
        let staged_baselib = draft.stage_source("baselib", &baselib).unwrap();
        assert_eq!(fs::read(staged_game).unwrap(), game_bytes);
        assert_eq!(fs::read(staged_baselib).unwrap(), BASELIB_BYTES);
        let game_index_root = draft.index_output_dir("game").unwrap();
        let baselib_index_root = draft.index_output_dir("baselib").unwrap();
        fs::create_dir_all(game_index_root.join("nested")).unwrap();
        fs::write(game_index_root.join("nested/Game.cs"), game_index).unwrap();
        fs::write(game_index_root.join("Game.csproj"), "<Project />").unwrap();
        fs::write(baselib_index_root.join("BaseLib.cs"), "class BaseLib {}").unwrap();
        draft.finalize(tools()).unwrap()
    }

    #[test]
    fn stages_activates_and_reopens_verified_snapshot() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let verified = complete_snapshot(
            &store,
            &pack,
            &temp.path().join("sources"),
            GAME_BYTES,
            "class Game {}",
        );

        assert_eq!(verified.snapshot_id().len(), 64);
        assert_eq!(verified.game_pack_id(), "fixture-game");
        assert_eq!(verified.manifest().sources.len(), 2);
        assert_eq!(verified.manifest().indexes.len(), 2);
        assert_eq!(
            fs::read(verified.source_path("game").unwrap()).unwrap(),
            GAME_BYTES
        );
        let reopened = store.open_current(&pack).unwrap().unwrap();
        assert_eq!(reopened.snapshot_id(), verified.snapshot_id());
        assert_eq!(reopened.root(), verified.root());
    }

    #[test]
    fn cancellation_after_prepare_does_not_activate_current_pointer() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let (game, baselib) = source_files(temp.path(), GAME_BYTES, BASELIB_BYTES);
        let cancellation = CancellationToken::new();
        let mut draft = store.begin(&pack).unwrap();
        draft.stage_source("game", &game).unwrap();
        draft.stage_source("baselib", &baselib).unwrap();
        fs::write(
            draft.index_output_dir("game").unwrap().join("Game.cs"),
            "class Game {}",
        )
        .unwrap();
        fs::write(
            draft
                .index_output_dir("baselib")
                .unwrap()
                .join("BaseLib.cs"),
            "class BaseLib {}",
        )
        .unwrap();

        let prepared = draft.prepare_cancellable(tools(), &cancellation).unwrap();
        cancellation.cancel(CancellationReason::ProjectClose);
        assert!(matches!(
            prepared.activate_cancellable(&cancellation),
            Err(TruthSnapshotError::Cancelled)
        ));
        assert!(store.open_current(&pack).unwrap().is_none());
    }

    #[test]
    fn identical_content_reuses_snapshot_id_and_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let first = complete_snapshot(
            &store,
            &pack,
            &temp.path().join("sources-a"),
            GAME_BYTES,
            "class Same {}",
        );
        let second = complete_snapshot(
            &store,
            &pack,
            &temp.path().join("sources-b"),
            GAME_BYTES,
            "class Same {}",
        );

        assert_eq!(first.snapshot_id(), second.snapshot_id());
        let snapshot_count = fs::read_dir(store.root().join("snapshots"))
            .unwrap()
            .count();
        assert_eq!(snapshot_count, 1);
    }

    #[test]
    fn remote_checksum_mismatch_never_activates() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let (game, baselib) = source_files(temp.path(), GAME_BYTES, b"wrong-baselib");
        let mut draft = store.begin(&pack).unwrap();
        draft.stage_source("game", &game).unwrap();
        let error = draft.stage_source("baselib", &baselib).unwrap_err();
        assert!(matches!(
            error,
            TruthSnapshotError::SourceChecksumMismatch { source_id, .. }
                if source_id == "baselib"
        ));
        drop(draft);
        assert!(store.open_current(&pack).unwrap().is_none());
    }

    #[test]
    fn missing_source_and_empty_index_are_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let (game, baselib) = source_files(temp.path(), GAME_BYTES, BASELIB_BYTES);

        let mut missing = store.begin(&pack).unwrap();
        missing.stage_source("game", &game).unwrap();
        fs::write(
            missing.index_output_dir("game").unwrap().join("Game.cs"),
            "class Game {}",
        )
        .unwrap();
        let error = missing.finalize(tools()).unwrap_err();
        assert!(matches!(error, TruthSnapshotError::MissingSource(id) if id == "baselib"));

        let mut empty = store.begin(&pack).unwrap();
        empty.stage_source("game", &game).unwrap();
        empty.stage_source("baselib", &baselib).unwrap();
        fs::write(
            empty.index_output_dir("game").unwrap().join("Game.cs"),
            "class Game {}",
        )
        .unwrap();
        empty.index_output_dir("baselib").unwrap();
        let error = empty.finalize(tools()).unwrap_err();
        assert!(matches!(error, TruthSnapshotError::EmptyIndex(id) if id == "baselib"));
    }

    #[test]
    fn failed_draft_preserves_previous_current_snapshot() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let first = complete_snapshot(
            &store,
            &pack,
            &temp.path().join("sources-a"),
            GAME_BYTES,
            "class First {}",
        );

        let (game, baselib) = source_files(
            &temp.path().join("sources-b"),
            b"changed-game",
            BASELIB_BYTES,
        );
        let mut failed = store.begin(&pack).unwrap();
        failed.stage_source("game", &game).unwrap();
        failed.stage_source("baselib", &baselib).unwrap();
        fs::write(
            failed.index_output_dir("game").unwrap().join("Game.cs"),
            "class Changed {}",
        )
        .unwrap();
        let error = failed.finalize(tools()).unwrap_err();
        assert!(matches!(error, TruthSnapshotError::MissingIndex(id) if id == "baselib"));

        let current = store.open_current(&pack).unwrap().unwrap();
        assert_eq!(current.snapshot_id(), first.snapshot_id());
    }

    #[test]
    fn tampered_index_is_rejected_on_reopen() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let verified = complete_snapshot(
            &store,
            &pack,
            &temp.path().join("sources"),
            GAME_BYTES,
            "class Before {}",
        );
        fs::write(
            verified.index_root("game").unwrap().join("nested/Game.cs"),
            "class Tampered {}",
        )
        .unwrap();

        let error = store.open_current(&pack).unwrap_err();
        assert!(matches!(
            error,
            TruthSnapshotError::IndexIntegrityMismatch { source_id, .. }
                if source_id == "game"
        ));
    }

    #[test]
    fn tampered_source_is_rejected_on_reopen() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let verified = complete_snapshot(
            &store,
            &pack,
            &temp.path().join("sources"),
            GAME_BYTES,
            "class Game {}",
        );
        fs::write(
            verified.source_path("game").unwrap(),
            b"tampered-game-assembly",
        )
        .unwrap();

        let error = store.open_current(&pack).unwrap_err();
        assert!(matches!(
            error,
            TruthSnapshotError::SourceChecksumMismatch { source_id, .. }
                if source_id == "game"
        ));
    }

    #[test]
    fn active_snapshot_handle_remains_fixed_after_new_activation() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let first = complete_snapshot(
            &store,
            &pack,
            &temp.path().join("sources-a"),
            GAME_BYTES,
            "class First {}",
        );
        let first_root = first.index_root("game").unwrap();
        let first_text = fs::read_to_string(first_root.join("nested/Game.cs")).unwrap();

        let second = complete_snapshot(
            &store,
            &pack,
            &temp.path().join("sources-b"),
            b"new-game-assembly",
            "class Second {}",
        );
        assert_ne!(first.snapshot_id(), second.snapshot_id());
        assert_eq!(
            store.open_current(&pack).unwrap().unwrap().snapshot_id(),
            second.snapshot_id()
        );
        assert_eq!(
            fs::read_to_string(first_root.join("nested/Game.cs")).unwrap(),
            first_text
        );
        assert!(first.root().is_dir());
    }

    #[test]
    fn current_pointer_rejects_path_traversal_snapshot_id() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        fs::create_dir_all(store.root()).unwrap();
        fs::write(
            store.root().join("current.json"),
            r#"{"schemaVersion":1,"snapshotId":"../escape","manifestSha256":"bad"}"#,
        )
        .unwrap();
        let error = store.open_current(&pack).unwrap_err();
        assert!(matches!(error, TruthSnapshotError::InvalidSnapshotId(_)));
    }

    #[test]
    fn store_rejects_a_different_pack() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let mut other = pack.clone();
        other.id = "other-game".into();

        let error = store.begin(&other).unwrap_err();
        assert!(matches!(error, TruthSnapshotError::PackMismatch(_)));
        let error = store.open_current(&other).unwrap_err();
        assert!(matches!(error, TruthSnapshotError::PackMismatch(_)));
    }

    #[test]
    fn index_symlink_is_rejected() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(&temp.path().join("runtime"), &pack);
        let (game, baselib) = source_files(temp.path(), GAME_BYTES, BASELIB_BYTES);
        let mut draft = store.begin(&pack).unwrap();
        draft.stage_source("game", &game).unwrap();
        draft.stage_source("baselib", &baselib).unwrap();
        let game_index = draft.index_output_dir("game").unwrap();
        let baselib_index = draft.index_output_dir("baselib").unwrap();
        let outside = temp.path().join("outside.cs");
        fs::write(&outside, "class Outside {}").unwrap();
        if !create_file_symlink(&outside, &game_index.join("linked.cs")) {
            return;
        }
        fs::write(baselib_index.join("BaseLib.cs"), "class BaseLib {}").unwrap();
        let error = draft.finalize(tools()).unwrap_err();
        assert!(matches!(
            error,
            TruthSnapshotError::InvalidIndexEntry { reason, .. }
                if reason.contains("symbolic links")
        ));
    }

    #[cfg(unix)]
    fn create_file_symlink(source: &Path, target: &Path) -> bool {
        std::os::unix::fs::symlink(source, target).is_ok()
    }

    #[cfg(windows)]
    fn create_file_symlink(source: &Path, target: &Path) -> bool {
        std::os::windows::fs::symlink_file(source, target).is_ok()
    }
}
