//! Codegen commands —— prompt 预览，对标 routes::codegen。

use ats_core::codegen::{
    AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest,
    PromptAssembler,
};
use ats_core::knowledge::{KnowledgePaths, SourceMode};
use tauri::State;

use crate::AppConfig;

fn assemble<F>(config: &State<'_, AppConfig>, f: F) -> Result<String, String>
where
    F: FnOnce(
        &PromptAssembler,
        &KnowledgePaths,
    ) -> Result<String, ats_core::prompting::PromptError>,
{
    let paths = KnowledgePaths::from_runtime_dir(&config.status_snapshot().runtime_dir());
    let assembler = PromptAssembler::built_in();
    f(&assembler, &paths).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn codegen_asset_prompt(
    config: State<'_, AppConfig>,
    request: AssetCodegenRequest,
) -> Result<String, String> {
    assemble(&config, |a, p| {
        a.assemble_asset_prompt(&request, p, SourceMode::Missing)
    })
}

#[tauri::command]
pub fn codegen_custom_code_prompt(
    config: State<'_, AppConfig>,
    request: CustomCodegenRequest,
) -> Result<String, String> {
    assemble(&config, |a, p| {
        a.assemble_custom_code_prompt(&request, p, SourceMode::Missing)
    })
}

#[tauri::command]
pub fn codegen_asset_group_prompt(
    config: State<'_, AppConfig>,
    request: AssetGroupRequest,
) -> Result<String, String> {
    assemble(&config, |a, p| {
        a.assemble_asset_group_prompt(&request, p, SourceMode::Missing)
    })
}

#[tauri::command]
pub fn codegen_build_prompt(max_attempts: Option<u32>) -> Result<String, String> {
    PromptAssembler::built_in()
        .assemble_build_prompt(max_attempts.unwrap_or(3))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn codegen_create_mod_project_prompt(request: ModProjectRequest) -> Result<String, String> {
    PromptAssembler::built_in()
        .assemble_create_mod_project_prompt(&request)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn codegen_package_prompt() -> Result<String, String> {
    PromptAssembler::built_in()
        .assemble_package_prompt()
        .map_err(|e| e.to_string())
}
