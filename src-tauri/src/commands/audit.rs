//! 审计日志 + 产物状态命令。
//!
//! 都基于当前 active project；调用方有责任先打开一个工程。

use std::path::PathBuf;
use std::sync::Arc;

use ats_core::audit::{AuditEntry, append_entry, read_recent};
use ats_core::plan_artifact::{
    ArtifactState, ArtifactStatus, list_statuses, load_status, save_status,
};
use tauri::State;

use crate::commands::project::ActiveProject;

fn active_root(active: &State<'_, ActiveProject>) -> Result<PathBuf, String> {
    let guard = active
        .0
        .lock()
        .map_err(|e| format!("active project lock poisoned: {e}"))?;
    let project = guard
        .as_ref()
        .ok_or_else(|| "no active project — open or create one first".to_string())?;
    Ok(project.path().to_path_buf())
}

#[tauri::command]
pub fn audit_append(
    active: State<'_, ActiveProject>,
    kind: String,
    message: String,
    ref_id: Option<String>,
    data: Option<serde_json::Value>,
) -> Result<(), String> {
    let root = active_root(&active)?;
    let mut entry = AuditEntry::new(kind, message);
    if let Some(id) = ref_id {
        entry = entry.with_ref(id);
    }
    if let Some(d) = data {
        entry = entry.with_data(d);
    }
    append_entry(&root, &entry).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn audit_read_recent(
    active: State<'_, ActiveProject>,
    limit: usize,
) -> Result<Vec<AuditEntry>, String> {
    let root = active_root(&active)?;
    read_recent(&root, limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn plan_artifact_save(
    active: State<'_, ActiveProject>,
    status: ArtifactStatus,
) -> Result<(), String> {
    let root = active_root(&active)?;
    save_status(&root, &status).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn plan_artifact_load(
    active: State<'_, ActiveProject>,
    item_id: String,
) -> Result<Option<ArtifactStatus>, String> {
    let root = active_root(&active)?;
    load_status(&root, &item_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn plan_artifact_list(
    active: State<'_, ActiveProject>,
) -> Result<Vec<ArtifactStatus>, String> {
    let root = active_root(&active)?;
    list_statuses(&root).map_err(|e| e.to_string())
}

/// 不在前端暴露 ArtifactState enum 时让 IDE / tsc 报错的兜底：保持类型在 binding 里。
#[allow(dead_code)]
fn _force_export_artifact_state() -> ArtifactState {
    ArtifactState::Pending
}

/// Arc 强制导入避免 unused-warning（实际使用见 active_root）。
#[allow(dead_code)]
fn _arc_in_scope(_: Arc<()>) {}
