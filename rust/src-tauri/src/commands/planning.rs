//! Planning commands —— 对标 routes::planning。

use ats_core::planning::{ModPlan, PlanValidationResult, ReviewStrictness, validate_plan};

#[tauri::command]
pub fn validate_plan_cmd(plan: ModPlan, strictness: Option<ReviewStrictness>) -> PlanValidationResult {
    validate_plan(&plan, strictness.unwrap_or_default())
}
