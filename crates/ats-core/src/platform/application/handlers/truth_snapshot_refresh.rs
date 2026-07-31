//! RunRecord lifecycle adapter for Pack-driven Truth Snapshot refresh.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::failure::ActionableFailure;
use crate::game_pack::{LoadedGamePack, TruthSnapshotRefresher, TruthSnapshotStore};
use crate::platform::contracts::SubmitTruthSnapshotRefreshRequest;
use crate::platform::domain::{RunId, RunRepository, RunResult};

use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, finalize_with_failure, finalize_with_success,
    transition_to_running,
};

#[allow(clippy::too_many_arguments)]
pub async fn run_truth_snapshot_refresh(
    repo: Arc<dyn RunRepository>,
    sink: Arc<dyn ProgressSink>,
    run_id: RunId,
    request: SubmitTruthSnapshotRefreshRequest,
    pack: LoadedGamePack,
    store: TruthSnapshotStore,
    local_inputs: BTreeMap<String, PathBuf>,
    refresher: TruthSnapshotRefresher,
) {
    if transition_to_running(&repo, &run_id, &sink).await.is_err() {
        return;
    }
    sink.emit(ProgressEvent {
        run_id: run_id.clone(),
        stage: "truth-snapshot-refresh".into(),
        percent: Some(0.1),
        message: Some(format!("refreshing verified sources for {}", pack.id)),
        delta: None,
    })
    .await;

    let outcome = match refresher
        .refresh(&pack, &store, &local_inputs, request.force)
        .await
    {
        Ok(outcome) => outcome,
        Err(_) => {
            finalize_with_failure(
                &repo,
                &run_id,
                &sink,
                ActionableFailure::unclassified("truth_snapshot.refresh"),
            )
            .await;
            return;
        }
    };
    let result = RunResult::TruthSnapshotRefresh {
        game_pack_id: pack.id,
        snapshot_id: outcome.snapshot_id,
        cache_hit: outcome.cache_hit,
        source_count: outcome.source_count,
        index_count: outcome.index_count,
        tool_versions: outcome.tool_versions,
        warnings: outcome.warnings,
    };
    match finalize_with_success(&repo, &run_id, result).await {
        FinalizeOutcome::Succeeded => {
            sink.emit(ProgressEvent {
                run_id,
                stage: "completed".into(),
                percent: Some(1.0),
                message: Some("verified truth snapshot is active".into()),
                delta: None,
            })
            .await;
        }
        FinalizeOutcome::Cancelled | FinalizeOutcome::Vanished => {}
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use async_trait::async_trait;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::game_pack::{
        GamePackLoadPolicy, GamePackLoader, RemoteTruthSourceFetcher, TruthSourceIndexer,
    };
    use crate::platform::application::{NoopProgressSink, RunApplicationService};
    use crate::platform::domain::RunStatus;
    use crate::platform::infra::FileRunRepository;

    struct FixtureFetcher;

    #[async_trait]
    impl RemoteTruthSourceFetcher for FixtureFetcher {
        async fn fetch(
            &self,
            _repository: &str,
            _pinned_release: &str,
            _asset: &str,
            destination: &Path,
        ) -> Result<(), String> {
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(destination, b"fixture-library").unwrap();
            Ok(())
        }
    }

    struct FixtureIndexer;

    impl TruthSourceIndexer for FixtureIndexer {
        fn tool_versions(&self) -> Result<BTreeMap<String, String>, String> {
            Ok(BTreeMap::from([("fixture-indexer".into(), "1".into())]))
        }

        fn index(&self, indexer: &str, _source: &Path, output_dir: &Path) -> Result<(), String> {
            fs::create_dir_all(output_dir).unwrap();
            fs::write(output_dir.join(format!("{indexer}.cs")), "class Fixture {}").unwrap();
            Ok(())
        }
    }

    fn fixture_pack() -> LoadedGamePack {
        let remote_sha = format!("{:x}", Sha256::digest(b"fixture-library"));
        let json = format!(
            r#"{{
              "schema_version": 1,
              "id": "fixture-game",
              "display_name": "Fixture Game",
              "capabilities": ["truth_sources"],
              "truth_sources": [
                {{"id":"game","kind":"local_file","input_key":"game_assembly","indexer":"dotnet_project","provider":"fixture_facts"}},
                {{"id":"library","kind":"github_release_asset","repository":"owner/repo","pinned_release":"v1","asset":"Library.dll","sha256":"{remote_sha}","indexer":"dotnet_file","provider":"fixture_facts"}}
              ]
            }}"#
        );
        GamePackLoader::new(GamePackLoadPolicy::new(
            ["truth_sources"],
            ["dotnet_project", "dotnet_file"],
            ["fixture_facts"],
        ))
        .load_str("fixture", &json)
        .unwrap()
    }

    #[tokio::test]
    async fn succeeded_run_records_snapshot_identity_and_counts() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack();
        let game = temp.path().join("game.dll");
        fs::write(&game, b"fixture-game").unwrap();
        let repo: Arc<dyn RunRepository> =
            Arc::new(FileRunRepository::new(temp.path().join("history")));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        let refresher =
            TruthSnapshotRefresher::new(Arc::new(FixtureFetcher), Arc::new(FixtureIndexer));
        let service = RunApplicationService::without_llm(Arc::clone(&repo));
        let run_id = service
            .submit_truth_snapshot_refresh(
                SubmitTruthSnapshotRefreshRequest { force: false },
                pack,
                store,
                BTreeMap::from([("game_assembly".into(), game)]),
                refresher,
                Arc::new(NoopProgressSink),
            )
            .await
            .unwrap();

        for _ in 0..100 {
            if repo.get(&run_id).await.unwrap().status.is_terminal() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let completed = repo.get(&run_id).await.unwrap();
        assert_eq!(completed.status, RunStatus::Succeeded);
        let result = serde_json::to_value(completed.result.unwrap()).unwrap();
        assert_eq!(result["gamePackId"], "fixture-game");
        assert_eq!(result["sourceCount"], 2);
        assert_eq!(result["indexCount"], 2);
        assert_eq!(result["cacheHit"], false);
        assert_eq!(result["snapshotId"].as_str().unwrap().len(), 64);
    }
}
