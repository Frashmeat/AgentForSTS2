//! Planning HTTP 路由——验证 + 执行 bundle 推断。

use std::collections::HashMap;

use ats_core::planning::{
    BundleDecision, ExecutionPlanPreview, ModPlan, PlanValidationResult, ReviewStrictness,
    build_execution_plan, validate_plan,
};
use axum::{Json, Router, extract::Query, routing::post};
use serde::Deserialize;

pub fn router() -> Router {
    Router::new()
        .route("/api/planning/validate", post(validate_handler))
        .route("/api/planning/execution-plan", post(execution_plan_handler))
}

#[derive(Debug, Deserialize, Default)]
struct StrictnessQuery {
    #[serde(default)]
    strictness: Option<ReviewStrictness>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ExecutionPlanRequest {
    plan: ModPlan,
    #[serde(default)]
    bundle_decisions: HashMap<String, BundleDecision>,
}

async fn validate_handler(
    Query(query): Query<StrictnessQuery>,
    Json(plan): Json<ModPlan>,
) -> Json<PlanValidationResult> {
    Json(validate_plan(&plan, query.strictness.unwrap_or_default()))
}

async fn execution_plan_handler(
    Query(query): Query<StrictnessQuery>,
    Json(body): Json<ExecutionPlanRequest>,
) -> Json<ExecutionPlanPreview> {
    Json(build_execution_plan(
        &body.plan,
        query.strictness.unwrap_or_default(),
        &body.bundle_decisions,
    ))
}
