//! 健康检查与运行时自报。
//!
//! 双壳通用：Web 端经 `GET /api/health` 暴露；Tauri 端经 `get_health` command 暴露。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Web,
    Workstation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub status: &'static str,
    pub role: Role,
    pub core_version: String,
    pub server_time: chrono::DateTime<chrono::Utc>,
}

#[must_use]
pub fn report(role: Role) -> HealthReport {
    HealthReport {
        status: "ok",
        role,
        core_version: crate::version().to_string(),
        server_time: chrono::Utc::now(),
    }
}
