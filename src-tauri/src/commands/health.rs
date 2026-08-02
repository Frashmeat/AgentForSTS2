//! Health command — 双壳对称：Web 端走 `GET /api/health`，桌面走此 command。
//!
//! Stage 5 装配后用 report_full 把 readiness 字段（LLM / image_gen / active_project）
//! 一并填了，前端 HealthCard 能直接显示三色灯。

use std::sync::Arc;

use ats_core::game_pack::{
    GamePackRegistry, TruthSnapshotReadiness, TruthSnapshotStore, inspect_truth_snapshot,
};
use ats_core::health::{HealthReport, Role};
use tauri::State;

use crate::AppConfig;
use crate::commands::failure::CommandResult;
use crate::commands::image_proc_state::{ImageProcState, PrewarmStatus};
use crate::project_session::ActiveProject;

#[tauri::command]
pub async fn get_health(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    image_proc: State<'_, Arc<ImageProcState>>,
) -> CommandResult<HealthReport> {
    let current = active.current().unwrap_or(None);
    let active_open = current.is_some();
    let game_id = current
        .as_ref()
        .map(|session| session.meta().game_id.clone());
    let image_proc_ready = matches!(image_proc.status_snapshot(), PrewarmStatus::Ready { .. });
    let status = config.status_snapshot();
    let (settings, _) = config.snapshot();
    let runtime_dir = config.runtime_dir();
    let truth_snapshot_ready = tokio::task::spawn_blocking(move || {
        let game_id = game_id?;
        let registry = GamePackRegistry::built_in().ok()?;
        let pack = registry.require(&game_id).ok()?;
        let store = TruthSnapshotStore::new(&runtime_dir, pack);
        Some(inspect_truth_snapshot(pack, &store).state == TruthSnapshotReadiness::Ready)
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(false);
    Ok(ats_core::health::report_full(
        Role::Workstation,
        status,
        &settings,
        active_open,
        image_proc_ready,
        truth_snapshot_ready,
    ))
}
