//! Codegen commands —— prompt 预览，对标 routes::codegen。

use ats_core::codegen::{
    AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest,
    PromptAssembler,
};
use ats_core::game_pack::VerifiedGameContext;
use tauri::State;

use crate::AppConfig;
use crate::commands::platform::active_game_context;
use crate::commands::project::ActiveProject;

fn assemble<F>(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
    f: F,
) -> Result<String, String>
where
    F: FnOnce(
        &PromptAssembler,
        &VerifiedGameContext,
    ) -> Result<String, ats_core::codegen::PromptAssemblyError>,
{
    let context = active_game_context(config, active)?;
    let assembler = PromptAssembler::built_in();
    f(&assembler, &context).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn codegen_asset_prompt(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: AssetCodegenRequest,
) -> Result<String, String> {
    assemble(&config, &active, |assembler, context| {
        assembler.assemble_asset_prompt(&request, context)
    })
}

#[tauri::command]
pub fn codegen_custom_code_prompt(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: CustomCodegenRequest,
) -> Result<String, String> {
    assemble(&config, &active, |assembler, context| {
        assembler.assemble_custom_code_prompt(&request, context)
    })
}

#[tauri::command]
pub fn codegen_asset_group_prompt(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: AssetGroupRequest,
) -> Result<String, String> {
    assemble(&config, &active, |assembler, context| {
        assembler.assemble_asset_group_prompt(&request, context)
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
