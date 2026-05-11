//! Plan JSON 反序列化。
//!
//! 对外只暴露 `parse_plan(&str) -> Result<ModPlan, ParseError>`；调用方拿到
//! `ModPlan` 后可直接送 `validate_plan` 做语义校验。
//!
//! 与 Python `parse_plan` / `plan_from_dict` 的细节差异：Python 端额外做了
//! 字段强转 + 默认值补齐，本实现依赖 serde 的 `#[serde(default)]` 完成同样的工作。

use thiserror::Error;

use super::models::ModPlan;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("plan JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn parse_plan(raw_json: &str) -> Result<ModPlan, ParseError> {
    Ok(serde_json::from_str(raw_json)?)
}
