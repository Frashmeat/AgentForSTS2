//! Planning commands —— 对标 routes::planning。

use std::collections::HashMap;

use ats_core::planning::{
    BundleDecision, ExecutionPlanPreview, ModPlan, PlanValidationResult, ReviewStrictness,
    build_execution_plan, validate_plan,
};

#[tauri::command]
pub fn validate_plan_cmd(
    plan: ModPlan,
    strictness: Option<ReviewStrictness>,
) -> PlanValidationResult {
    validate_plan(&plan, strictness.unwrap_or_default())
}

#[tauri::command]
pub fn build_execution_plan_cmd(
    plan: ModPlan,
    strictness: Option<ReviewStrictness>,
    bundle_decisions: Option<HashMap<String, BundleDecision>>,
) -> ExecutionPlanPreview {
    build_execution_plan(
        &plan,
        strictness.unwrap_or_default(),
        &bundle_decisions.unwrap_or_default(),
    )
}
