//! Health command — 双壳对称：Web 端走 `GET /api/health`，桌面走此 command。

use ats_core::health::{HealthReport, Role};
use tauri::State;

use crate::AppConfig;

#[tauri::command]
pub fn get_health(config: State<'_, AppConfig>) -> HealthReport {
    ats_core::health::report(Role::Workstation, config.status.clone())
}
