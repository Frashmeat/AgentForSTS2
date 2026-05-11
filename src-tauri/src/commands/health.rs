//! Health command — 双壳对称：Web 端走 `GET /api/health`，桌面走此 command。
//!
//! Stage 5 装配后用 report_full 把 readiness 字段（LLM / image_gen / active_project）
//! 一并填了，前端 HealthCard 能直接显示三色灯。

use ats_core::health::{HealthReport, Role};
use tauri::State;

use crate::AppConfig;
use crate::commands::project::ActiveProject;

#[tauri::command]
pub fn get_health(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
) -> HealthReport {
    let active_open = active
        .0
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false);
    ats_core::health::report_full(
        Role::Workstation,
        config.status.clone(),
        &config.settings,
        active_open,
    )
}
