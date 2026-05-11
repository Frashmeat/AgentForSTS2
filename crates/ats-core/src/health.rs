//! 健康检查与运行时自报。
//!
//! 双壳通用：Web 端经 `GET /api/health` 暴露；Tauri 端经 `get_health` command 暴露。
//! 报告内含配置加载状态（路径、是否解析成功、校验错误），便于在前端直观看到配置问题。
//!
//! Stage 5 装配收口后扩了多个 ready 子字段，让前端能直接显示"LLM 配好了吗 /
//! image_gen 配好了吗 / 工程文件夹打开了吗"。

use serde::{Deserialize, Serialize};

use crate::config::{ConfigStatus, Settings};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Web,
    Workstation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessFlags {
    /// llm.api_key 非空（client 能构造起来）
    pub llm_configured: bool,
    /// image_gen.api_key 非空
    pub image_gen_configured: bool,
    /// 当前是否有 active project（仅 Tauri 端能填，Web 端永 false）
    pub active_project_open: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub status: &'static str,
    pub role: Role,
    pub core_version: String,
    pub server_time: chrono::DateTime<chrono::Utc>,
    pub config: ConfigStatus,
    #[serde(default)]
    pub readiness: ReadinessFlags,
}

#[must_use]
pub fn report(role: Role, config: ConfigStatus) -> HealthReport {
    HealthReport {
        status: if config.errors.is_empty() { "ok" } else { "degraded" },
        role,
        core_version: crate::version().to_string(),
        server_time: chrono::Utc::now(),
        config,
        readiness: ReadinessFlags::default(),
    }
}

/// 扩展版：把 settings + active_project 状态填进 readiness。
#[must_use]
pub fn report_full(
    role: Role,
    config: ConfigStatus,
    settings: &Settings,
    active_project_open: bool,
) -> HealthReport {
    let mut r = report(role, config);
    r.readiness = ReadinessFlags {
        llm_configured: !settings.llm.api_key.is_empty(),
        image_gen_configured: !settings.image_gen.api_key.is_empty(),
        active_project_open,
    };
    r
}
