//! Codegen HTTP 路由——prompt 预览端点（不执行 LLM）。

use std::sync::Arc;

use ats_core::codegen::{
    AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest,
    PromptAssembler,
};
use ats_core::knowledge::{KnowledgePaths, SourceMode};
use axum::{Extension, Json, Router, http::StatusCode, routing::post};
use serde::{Deserialize, Serialize};

use crate::AppState;

pub fn router() -> Router {
    Router::new()
        .route("/api/codegen/asset-prompt", post(asset_prompt))
        .route("/api/codegen/custom-code-prompt", post(custom_code_prompt))
        .route("/api/codegen/asset-group-prompt", post(asset_group_prompt))
        .route("/api/codegen/build-prompt", post(build_prompt))
        .route(
            "/api/codegen/create-mod-project-prompt",
            post(create_mod_project_prompt),
        )
        .route("/api/codegen/package-prompt", post(package_prompt))
}

#[derive(Debug, Serialize)]
struct PromptResponse {
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct BuildPromptBody {
    #[serde(default = "default_max_attempts")]
    max_attempts: u32,
}
fn default_max_attempts() -> u32 {
    3
}

fn assembler_and_paths(state: &AppState) -> (PromptAssembler, KnowledgePaths) {
    let paths = KnowledgePaths::from_runtime_dir(&state.runtime_dir);
    let assembler = PromptAssembler::built_in();
    (assembler, paths)
}

fn into_response<E: std::fmt::Display>(
    result: Result<String, E>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    result
        .map(|prompt| Json(PromptResponse { prompt }))
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
}

async fn asset_prompt(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<AssetCodegenRequest>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let (a, paths) = assembler_and_paths(&state);
    // Stage 2.2 之前，game source mode 暂用 Missing；后续接入实际检测。
    into_response(a.assemble_asset_prompt(&request, &paths, SourceMode::Missing))
}

async fn custom_code_prompt(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CustomCodegenRequest>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let (a, paths) = assembler_and_paths(&state);
    into_response(a.assemble_custom_code_prompt(&request, &paths, SourceMode::Missing))
}

async fn asset_group_prompt(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<AssetGroupRequest>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let (a, paths) = assembler_and_paths(&state);
    into_response(a.assemble_asset_group_prompt(&request, &paths, SourceMode::Missing))
}

async fn build_prompt(
    Json(body): Json<BuildPromptBody>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let assembler = PromptAssembler::built_in();
    into_response(assembler.assemble_build_prompt(body.max_attempts))
}

async fn create_mod_project_prompt(
    Json(request): Json<ModProjectRequest>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let assembler = PromptAssembler::built_in();
    into_response(assembler.assemble_create_mod_project_prompt(&request))
}

async fn package_prompt() -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let assembler = PromptAssembler::built_in();
    into_response(assembler.assemble_package_prompt())
}
