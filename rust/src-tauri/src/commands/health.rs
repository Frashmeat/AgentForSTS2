//! Health command — 双壳对称：Web 端走 `GET /api/health`，桌面走此 command。

use ats_core::health::{HealthReport, Role, report};

#[tauri::command]
pub fn get_health() -> HealthReport {
    report(Role::Workstation)
}
