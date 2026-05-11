//! AgentTheSpire desktop（workstation 角色）入口。

mod commands;

use ats_core::config::{ConfigStatus, Settings};
use ats_core::health::Role;

/// Tauri 同进程内单一配置快照，通过 `app.manage()` 注入，由 command 通过 `tauri::State` 读取。
pub struct AppConfig {
    pub settings: Settings,
    pub status: ConfigStatus,
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

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .manage(AppConfig { settings, status })
        .invoke_handler(tauri::generate_handler![
            commands::health::get_health,
            commands::knowledge::get_knowledge_status,
            commands::knowledge::check_knowledge_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
