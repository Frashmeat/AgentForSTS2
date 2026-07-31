//! Plan artifact status commands.
//!
//! 都基于当前 active project；调用方有责任先打开一个工程。

use ats_core::plan_artifact::{
    ArtifactState, ArtifactStatus, list_statuses, load_status, save_status,
};
use std::path::PathBuf;
use tauri::State;

use crate::commands::failure::{CommandFailure, CommandResult};
use crate::commands::project::ActiveProject;

fn active_root(active: &State<'_, ActiveProject>) -> CommandResult<PathBuf> {
    let guard = active
        .0
        .lock()
        .map_err(|_| CommandFailure::unclassified("plan_artifact.project_lock"))?;
    let project = guard
        .as_ref()
        .ok_or_else(|| CommandFailure::project_not_open("plan_artifact.active_project"))?;
    Ok(project.path().to_path_buf())
}

#[tauri::command]
pub fn plan_artifact_save(
    active: State<'_, ActiveProject>,
    status: ArtifactStatus,
) -> CommandResult<()> {
    let root = active_root(&active)?;
    save_status(&root, &status).map_err(|_| CommandFailure::unclassified("plan_artifact.save"))
}

#[tauri::command]
pub fn plan_artifact_load(
    active: State<'_, ActiveProject>,
    item_id: String,
) -> CommandResult<Option<ArtifactStatus>> {
    let root = active_root(&active)?;
    load_status(&root, &item_id).map_err(|_| CommandFailure::unclassified("plan_artifact.load"))
}

#[tauri::command]
pub fn plan_artifact_list(active: State<'_, ActiveProject>) -> CommandResult<Vec<ArtifactStatus>> {
    let root = active_root(&active)?;
    list_statuses(&root).map_err(|_| CommandFailure::unclassified("plan_artifact.list"))
}

/// 不在前端暴露 ArtifactState enum 时让 IDE / tsc 报错的兜底：保持类型在 binding 里。
#[allow(dead_code)]
fn _force_export_artifact_state() -> ArtifactState {
    ArtifactState::Pending
}
