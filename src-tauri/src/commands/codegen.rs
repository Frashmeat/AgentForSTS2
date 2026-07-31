//! Codegen commands —— prompt 预览，对标 routes::codegen。

use ats_core::codegen::{
    AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest,
    PromptAssembler,
};
use ats_core::game_pack::VerifiedGameContext;
use tauri::State;

use crate::AppConfig;
use crate::commands::failure::{CommandFailure, CommandResult};
use crate::commands::platform::active_game_context;
use crate::commands::project::ActiveProject;

fn assemble<F>(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
    f: F,
) -> CommandResult<String>
where
    F: FnOnce(
        &PromptAssembler,
        &VerifiedGameContext,
    ) -> Result<String, ats_core::codegen::PromptAssemblyError>,
{
    let context = active_game_context(config, active)?;
    let assembler = PromptAssembler::built_in();
    f(&assembler, &context).map_err(|_| CommandFailure::unclassified("codegen.prompt_assembly"))
}

#[tauri::command]
pub fn codegen_asset_prompt(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: AssetCodegenRequest,
) -> CommandResult<String> {
    assemble(&config, &active, |assembler, context| {
        assembler.assemble_asset_prompt(&request, context)
    })
}

#[tauri::command]
pub fn codegen_custom_code_prompt(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: CustomCodegenRequest,
) -> CommandResult<String> {
    assemble(&config, &active, |assembler, context| {
        assembler.assemble_custom_code_prompt(&request, context)
    })
}

#[tauri::command]
pub fn codegen_asset_group_prompt(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: AssetGroupRequest,
) -> CommandResult<String> {
    assemble(&config, &active, |assembler, context| {
        assembler.assemble_asset_group_prompt(&request, context)
    })
}

#[tauri::command]
pub fn codegen_build_prompt(max_attempts: Option<u32>) -> CommandResult<String> {
    PromptAssembler::built_in()
        .assemble_build_prompt(max_attempts.unwrap_or(3))
        .map_err(|_| CommandFailure::unclassified("codegen.build_prompt"))
}

#[tauri::command]
pub fn codegen_create_mod_project_prompt(request: ModProjectRequest) -> CommandResult<String> {
    PromptAssembler::built_in()
        .assemble_create_mod_project_prompt(&request)
        .map_err(|_| CommandFailure::unclassified("codegen.project_prompt"))
}

#[tauri::command]
pub fn codegen_package_prompt() -> CommandResult<String> {
    PromptAssembler::built_in()
        .assemble_package_prompt()
        .map_err(|_| CommandFailure::unclassified("codegen.package_prompt"))
}
