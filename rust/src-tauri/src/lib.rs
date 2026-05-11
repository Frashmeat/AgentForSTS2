//! AgentTheSpire desktop（workstation 角色）入口。

mod commands;

use std::path::PathBuf;

use ats_core::config::{ConfigStatus, Settings};
use ats_core::health::Role;
use ats_core::project::AppDataPaths;

use crate::commands::project::ActiveProject;

/// Tauri 同进程内单一配置快照，通过 `app.manage()` 注入，由 command 通过 `tauri::State` 读取。
pub struct AppConfig {
    pub settings: Settings,
    pub status: ConfigStatus,
}

/// 路径快照，提供给 project commands 读 recent_projects.json 等。
pub struct AppPaths {
    pub data: AppDataPaths,
}

impl AppPaths {
    #[must_use]
    pub fn recents_path(&self) -> PathBuf {
        self.data.recent_projects_path.clone()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (settings, mut status) = Settings::load(None);
    let role_errors = settings.validate_for_role(Role::Workstation);
    if !role_errors.is_empty() {
        status.errors.extend(role_errors);
        status.loaded = false;
    }
    eprintln!(
        "ats-desktop: config path={:?}, loaded={}, errors={}",
        status.path,
        status.loaded,
        status.errors.len()
    );

    let app_data = AppDataPaths::resolve();
    if let Err(e) = app_data.ensure_dirs() {
        eprintln!(
            "ats-desktop: failed to create app data dir {}: {e}",
            app_data.root.display()
        );
    }
    eprintln!("ats-desktop: app data root={}", app_data.root.display());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .manage(AppConfig { settings, status })
        .manage(AppPaths { data: app_data })
        .manage(ActiveProject::new())
        .invoke_handler(tauri::generate_handler![
            commands::health::get_health,
            commands::capabilities::get_local_capabilities_sync,
            commands::capabilities::get_local_capabilities_full,
            commands::knowledge::get_knowledge_status,
            commands::knowledge::check_knowledge_status,
            commands::knowledge::export_knowledge_pack,
            commands::knowledge::import_knowledge_pack,
            commands::planning::validate_plan_cmd,
            commands::planning::build_execution_plan_cmd,
            commands::codegen::codegen_asset_prompt,
            commands::codegen::codegen_custom_code_prompt,
            commands::codegen::codegen_asset_group_prompt,
            commands::codegen::codegen_build_prompt,
            commands::codegen::codegen_create_mod_project_prompt,
            commands::codegen::codegen_package_prompt,
            commands::llm::llm_complete,
            commands::llm::llm_start_stream,
            commands::project::list_recent_projects,
            commands::project::create_project,
            commands::project::open_project,
            commands::project::close_project,
            commands::project::current_project,
            commands::project::forget_recent_project,
            commands::platform::submit_text_generate_job,
            commands::platform::submit_code_generate_job,
            commands::platform::submit_build_project_job,
            commands::platform::submit_log_analysis_job,
            commands::platform::submit_package_project_job,
            commands::platform::submit_batch_custom_code_job,
            commands::platform::submit_single_asset_plan_job,
            commands::platform::submit_knowledge_refresh_job,
            commands::platform::submit_asset_generate_job,
            commands::platform::get_job,
            commands::platform::list_jobs,
            commands::platform::cancel_job,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
