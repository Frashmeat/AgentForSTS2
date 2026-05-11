//! Platform 外部接口契约（DTO）。
//!
//! 命令 / 查询的入参与回参类型，单独成模块以便前端 + 两壳路由复用。
//! 与 Python `platform/contracts/*.py` 1:1 映射，但当前阶段只覆盖
//! text_generate 一条链路；后续 stage 按需填充其它 kind 的契约。

use serde::{Deserialize, Serialize};

use super::domain::JobId;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct SubmitTextGenerateRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitJobAck {
    pub job_id: JobId,
}
