//! Platform 外部接口契约（DTO）。
//!
//! 命令 / 查询的入参与回参类型，单独成模块以便前端 + 两壳路由复用。
//! 与 Python `platform/contracts/*.py` 1:1 映射。当前 stage 覆盖：
//! - text_generate（纯 LLM 调用）
//! - code_generate（PromptAssembler + LLM stream + 解 fence + 写文件）
//! - build_project（dotnet publish 子进程）

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::domain::JobId;
use crate::codegen::{AssetCodegenRequest, CustomCodegenRequest};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct SubmitTextGenerateRequest {
    pub prompt: String,
    pub system_prompt: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub model: Option<String>,
}

/// CodeGenerate 任务的两种模式：asset 或 custom_code。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SubmitCodeGenerateRequest {
    Asset { request: AssetCodegenRequest },
    CustomCode { request: CustomCodegenRequest },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct SubmitBuildProjectRequest {
    pub project_root: PathBuf,
    /// 失败时最大重试次数（当前 stage 不重试，仅记录字段）
    pub max_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitJobAck {
    pub job_id: JobId,
}
