//! Planning HTTP 路由——POST /api/planning/validate。

use ats_core::planning::{ModPlan, PlanValidationResult, ReviewStrictness, validate_plan};
use axum::{Json, Router, extract::Query, routing::post};
use serde::Deserialize;

pub fn router() -> Router {
    Router::new().route("/api/planning/validate", post(validate_handler))
}

#[derive(Debug, Deserialize, Default)]
struct ValidateQuery {
    #[serde(default)]
    strictness: Option<ReviewStrictness>,
}

async fn validate_handler(
    Query(query): Query<ValidateQuery>,
    Json(plan): Json<ModPlan>,
) -> Json<PlanValidationResult> {
    let strictness = query.strictness.unwrap_or_default();
    Json(validate_plan(&plan, strictness))
}
