//! Verified Truth Snapshot status for the active project.

use ats_core::game_pack::{
    GamePackRegistry, TruthSnapshotStatus, TruthSnapshotStore, inspect_truth_snapshot,
};
use tauri::State;

use crate::AppConfig;
use crate::commands::failure::{CommandFailure, CommandResult};
use crate::commands::platform::active_game_id;
use crate::commands::project::ActiveProject;

#[tauri::command]
pub async fn get_truth_snapshot_status(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
) -> CommandResult<TruthSnapshotStatus> {
    status(&config, &active).await
}

#[tauri::command]
pub async fn check_truth_snapshot_status(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
) -> CommandResult<TruthSnapshotStatus> {
    status(&config, &active).await
}

async fn status(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
) -> CommandResult<TruthSnapshotStatus> {
    let game_id = active_game_id(active)?;
    let registry = GamePackRegistry::built_in()
        .map_err(|_| CommandFailure::unclassified("truth_snapshot.registry"))?;
    let pack = registry
        .require(&game_id)
        .map_err(|_| {
            CommandFailure::invalid_input(
                "truth_snapshot.game_pack",
                "The active project references an unavailable Game Pack.",
            )
        })?
        .clone();
    let store = TruthSnapshotStore::new(&config.status_snapshot().runtime_dir(), &pack);
    tokio::task::spawn_blocking(move || inspect_truth_snapshot(&pack, &store))
        .await
        .map_err(|_| CommandFailure::unclassified("truth_snapshot.worker"))
}
