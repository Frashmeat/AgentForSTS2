//! Verified Truth Snapshot status for the active project.

use ats_core::game_pack::{
    GamePackRegistry, TruthSnapshotStatus, TruthSnapshotStore, inspect_truth_snapshot,
};
use tauri::State;

use crate::AppConfig;
use crate::commands::platform::active_game_id;
use crate::commands::project::ActiveProject;

#[tauri::command]
pub async fn get_truth_snapshot_status(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
) -> Result<TruthSnapshotStatus, String> {
    status(&config, &active).await
}

#[tauri::command]
pub async fn check_truth_snapshot_status(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
) -> Result<TruthSnapshotStatus, String> {
    status(&config, &active).await
}

async fn status(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
) -> Result<TruthSnapshotStatus, String> {
    let game_id = active_game_id(active)?;
    let registry = GamePackRegistry::built_in().map_err(|error| error.to_string())?;
    let pack = registry
        .require(&game_id)
        .map_err(|error| error.to_string())?
        .clone();
    let store = TruthSnapshotStore::new(&config.status_snapshot().runtime_dir(), &pack);
    tokio::task::spawn_blocking(move || inspect_truth_snapshot(&pack, &store))
        .await
        .map_err(|error| format!("inspect truth snapshot worker: {error}"))
}
