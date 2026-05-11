//! AgentTheSpire desktop（workstation 角色）入口。
//!
//! Stage 0：仅注册一个 `get_health` command，作为前端双适配的最小验证。
//! 后续 stage 按 module-mapping 把 routers/*.py 平移成 commands/*.rs。

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![commands::health::get_health])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
