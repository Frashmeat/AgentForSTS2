//! Codegen HTTP 路由——prompt 预览端点（不执行 LLM）。

use std::sync::Arc;

use ats_core::codegen::{
    AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest,
    PromptAssembler,
};
use ats_core::game_pack::{GamePackRegistry, VerifiedGameContext};
use ats_core::project::ProjectMeta;
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

fn game_context(
    state: &AppState,
    project_root: &std::path::Path,
) -> Result<VerifiedGameContext, (StatusCode, String)> {
    let project_path = project_root.join("project.json");
    let text = std::fs::read_to_string(&project_path).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("read project metadata {}: {error}", project_path.display()),
        )
    })?;
    let meta: ProjectMeta = serde_json::from_str(&text).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("parse project metadata {}: {error}", project_path.display()),
        )
    })?;
    let registry = GamePackRegistry::built_in()
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    VerifiedGameContext::open_current(&state.runtime_dir, &registry, &meta.game_id)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))
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
    let context = game_context(&state, &request.project_root)?;
    into_response(PromptAssembler::built_in().assemble_asset_prompt(&request, &context))
}

async fn custom_code_prompt(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<CustomCodegenRequest>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let context = game_context(&state, &request.project_root)?;
    into_response(PromptAssembler::built_in().assemble_custom_code_prompt(&request, &context))
}

async fn asset_group_prompt(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<AssetGroupRequest>,
) -> Result<Json<PromptResponse>, (StatusCode, String)> {
    let context = game_context(&state, &request.project_root)?;
    into_response(PromptAssembler::built_in().assemble_asset_group_prompt(&request, &context))
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

#[cfg(test)]
mod tests {
    use super::*;
    use ats_core::config::ConfigStatus;

    #[test]
    fn missing_current_snapshot_is_rejected_as_bad_request() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp = std::env::temp_dir().join(format!(
            "ats-web-game-context-{}-{unique}",
            std::process::id()
        ));
        let project_root = temp.join("project");
        std::fs::create_dir_all(&project_root).unwrap();
        std::fs::write(
            project_root.join("project.json"),
            r#"{"name":"demo","csharp_name":"DemoMod","game_id":"sts2","scaffolded":true,"generated_files":[],"build_output_dir":null}"#,
        )
        .unwrap();
        let state = AppState {
            config_status: ConfigStatus {
                path: None,
                file_present: false,
                loaded: false,
                errors: Vec::new(),
            },
            runtime_dir: temp.join("runtime"),
            settings_snapshot: None,
        };

        let error = game_context(&state, &project_root).unwrap_err();
        assert_eq!(error.0, StatusCode::BAD_REQUEST);
        assert!(error.1.contains("no verified current truth snapshot"));
        std::fs::remove_dir_all(&temp).unwrap();
    }
}
