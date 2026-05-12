//! Health command — 双壳对称：Web 端走 `GET /api/health`，桌面走此 command。
//!
//! Stage 5 装配后用 report_full 把 readiness 字段（LLM / image_gen / active_project）
//! 一并填了，前端 HealthCard 能直接显示三色灯。

use std::sync::Arc;

use ats_core::health::{HealthReport, Role};
use tauri::State;

use crate::AppConfig;
use crate::commands::image_proc_state::{ImageProcState, PrewarmStatus};
use crate::commands::project::ActiveProject;

#[tauri::command]
pub fn get_health(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    image_proc: State<'_, Arc<ImageProcState>>,
) -> HealthReport {
    let active_open = active
        .0
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false);
    let image_proc_ready = matches!(image_proc.status_snapshot(), PrewarmStatus::Ready { .. });
    ats_core::health::report_full(
        Role::Workstation,
        config.status.clone(),
        &config.settings,
        active_open,
        image_proc_ready,
    )
}
