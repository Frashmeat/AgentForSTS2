//! Platform 外部接口契约（DTO）。
//!
//! 命令 / 查询的入参与回参类型，单独成模块以便前端 + 两壳路由复用。
//! 与 Python `platform/contracts/*.py` 1:1 映射。当前 stage 覆盖：
//! - text_generate（纯 LLM 调用）
//! - code_generate（PromptAssembler + LLM stream + 解 fence + 写文件）
//! - build_project（dotnet publish 子进程）
//! - log_analysis（构建日志 → LLM 诊断报告）
//! - package_project（artifacts 目录 → zip）
//! - batch_custom_code（多 item 套 code_generate）
//! - single_asset_plan（LLM → 结构化 PlanItem）

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

/// log_analysis 任务的输入：log_path 或 log_text 至少提供一个。
/// 二者同在时优先用 log_text；都缺则任务失败。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct SubmitLogAnalysisRequest {
    pub log_path: Option<PathBuf>,
    pub log_text: Option<String>,
    /// 用户额外的上下文提示（例如 "我刚改了 csproj 的 TargetFramework"）。
    pub context_hint: Option<String>,
    /// 截断阈值：超过这个字符数的日志只取末尾，避免 LLM token 爆炸。
    /// 默认 30000。
    pub max_log_chars: Option<usize>,
}

/// package_project 任务的输入。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct SubmitPackageProjectRequest {
    /// 要打包的源目录（一般是工程的 artifacts/ 或 publish/ 输出）。
    pub source_dir: PathBuf,
    /// 输出 zip 的完整路径；不提供时落到 source_dir 同级 `<name>-<ts>.zip`。
    pub output_path: Option<PathBuf>,
    /// 压缩级别 0-9；不提供走 zip crate 默认（Deflated 中等）。
    pub compression_level: Option<i32>,
}

/// batch_custom_code 任务的输入：N 个 CustomCodegenRequest 顺序处理。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct SubmitBatchCustomCodeRequest {
    pub items: Vec<CustomCodegenRequest>,
    /// 单个 item 失败是否中断后续：默认 false（继续跑剩余 item）。
    pub fail_fast: bool,
}

/// single_asset_plan 任务的输入：用自然语言需求 + 资产类型，让 LLM 出一个
/// 结构化 PlanItem（JSON）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct SubmitSingleAssetPlanRequest {
    pub requirements: String,
    /// AssetItemType 的字符串值（如 "Hook" / "Relic" / "Character"）。
    /// 留空时由 LLM 自行判断。
    pub asset_type: Option<String>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitJobAck {
    pub job_id: JobId,
}
