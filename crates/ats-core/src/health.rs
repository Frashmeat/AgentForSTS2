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
    /// ML 背景去除模型预热是否就绪（feature ml-rembg + 模型加载成功）。
    /// 仅 Tauri 端可能为 true；feature off / web 端永 false。
    #[serde(default)]
    pub image_proc_ready: bool,
    /// 后台 queue worker 是否在跑。当前 desktop 没有显式 worker（每个 submit
    /// spawn 一个 tokio task），所以一直是 true；Stage 3.4 Web 轨 worker 上线后
    /// 才有实际意义。
    #[serde(default = "default_true")]
    pub queue_worker_ready: bool,
}

fn default_true() -> bool {
    true
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
        status: if config.errors.is_empty() {
            "ok"
        } else {
            "degraded"
        },
        role,
        core_version: crate::version().to_string(),
        server_time: chrono::Utc::now(),
        config,
        readiness: ReadinessFlags {
            queue_worker_ready: true,
            ..ReadinessFlags::default()
        },
    }
}

/// 扩展版：把 settings + active_project + ML prewarm 状态填进 readiness。
#[must_use]
pub fn report_full(
    role: Role,
    config: ConfigStatus,
    settings: &Settings,
    active_project_open: bool,
    image_proc_ready: bool,
) -> HealthReport {
    let mut r = report(role, config);
    r.readiness = ReadinessFlags {
        llm_configured: !settings.llm.api_key.is_empty(),
        image_gen_configured: !settings.image_gen.api_key.is_empty(),
        active_project_open,
        image_proc_ready,
        queue_worker_ready: true,
    };
    r
}
