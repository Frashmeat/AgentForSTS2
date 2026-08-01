//! AgentTheSpire desktop（workstation 角色）入口。

mod app_shutdown;
mod commands;
mod project_session;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use ats_core::config::{ConfigStatus, Settings};
use ats_core::health::Role;
use ats_core::project::AppDataPaths;
use tauri::Manager;

use crate::app_shutdown::{AppShutdown, ExitRequestAction, drain_active_project};
use crate::commands::image_proc_state::{ImageProcState, prewarm};
use crate::project_session::{ActiveProject, PROJECT_DRAIN_TIMEOUT};

const APP_DATA_ROOT_ENV: &str = "SPIREFORGE_APP_DATA_ROOT";

/// Tauri 同进程内可变配置：包裹 RwLock 让 `save_settings_patch` command 能在用户
/// 改完表单后热替换内存里的 Settings，不重启 app。读侧用 `snapshot()` 拿
/// clone，避免持锁跨 await。
pub struct AppConfig {
    inner: RwLock<AppConfigInner>,
}

pub struct AppConfigInner {
    pub settings: Settings,
    pub status: ConfigStatus,
}

impl AppConfig {
    #[must_use]
    pub fn new(settings: Settings, status: ConfigStatus) -> Self {
        Self {
            inner: RwLock::new(AppConfigInner { settings, status }),
        }
    }

    /// 克隆当前 settings + status。clone 廉价（Settings 只是嵌套小结构体），
    /// 锁立刻释放，避免 await 时持锁。
    pub fn snapshot(&self) -> (Settings, ConfigStatus) {
        let g = self.inner.read().expect("AppConfig RwLock poisoned");
        (g.settings.clone(), g.status.clone())
    }

    /// 仅读 settings 的便捷快捷。
    pub fn settings_snapshot(&self) -> Settings {
        self.inner
            .read()
            .expect("AppConfig RwLock poisoned")
            .settings
            .clone()
    }

    /// 仅读 status 的便捷快捷。
    pub fn status_snapshot(&self) -> ConfigStatus {
        self.inner
            .read()
            .expect("AppConfig RwLock poisoned")
            .status
            .clone()
    }

    /// 用新的 Settings 替换。validate 由调用方做（save_settings_patch 命令）。
    /// status 不动 —— path / file_present / loaded 由文件本身决定，与内存值无关。
    pub fn replace_settings(&self, new: Settings) {
        self.inner
            .write()
            .expect("AppConfig RwLock poisoned")
            .settings = new;
    }
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

    let app_data = std::env::var_os(APP_DATA_ROOT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(AppDataPaths::from_root)
        .unwrap_or_else(AppDataPaths::resolve);
    if let Err(e) = app_data.ensure_dirs() {
        eprintln!(
            "ats-desktop: failed to create app data dir {}: {e}",
            app_data.root.display()
        );
    }
    eprintln!("ats-desktop: app data root={}", app_data.root.display());

    let app_data_root = app_data.root.clone();
    let image_proc_state = Arc::new(ImageProcState::new());
    let image_proc_for_prewarm = Arc::clone(&image_proc_state);

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init());

    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    builder
        .setup(move |_app| {
            // ML rembg prewarm 后台跑（feature on 才真正下载/加载，
            // off 时立刻置 Failed("feature disabled")）。失败不影响 app 启动。
            let state = Arc::clone(&image_proc_for_prewarm);
            let root = app_data_root.clone();
            tauri::async_runtime::spawn(async move {
                prewarm(state, root).await;
            });
            Ok(())
        })
        .manage(AppConfig::new(settings, status))
        .manage(AppPaths { data: app_data })
        .manage(ActiveProject::new())
        .manage(AppShutdown::new())
        .manage(image_proc_state)
        .invoke_handler(tauri::generate_handler![
            commands::health::get_health,
            commands::capabilities::get_local_capabilities_sync,
            commands::capabilities::get_local_capabilities_full,
            commands::mod_analyzer::analyze_mod_project,
            commands::plan_artifact::plan_artifact_save,
            commands::plan_artifact::plan_artifact_load,
            commands::plan_artifact::plan_artifact_list,
            commands::knowledge::get_truth_snapshot_status,
            commands::knowledge::check_truth_snapshot_status,
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
            commands::platform::submit_text_generate_run,
            commands::platform::submit_code_generate_run,
            commands::platform::submit_build_project_run,
            commands::platform::submit_log_analysis_run,
            commands::platform::submit_package_project_run,
            commands::platform::submit_batch_custom_code_run,
            commands::platform::submit_single_asset_plan_run,
            commands::platform::submit_truth_snapshot_refresh_run,
            commands::platform::submit_asset_generate_run,
            commands::image_proc_state::image_proc_status,
            commands::settings::get_settings_snapshot,
            commands::settings::open_config_in_editor,
            commands::settings::save_settings_patch,
            commands::settings::discover_sts2_dll,
            commands::platform::get_run,
            commands::platform::list_runs,
            commands::platform::cancel_run,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            let tauri::RunEvent::ExitRequested { code, api, .. } = event else {
                return;
            };
            match app.state::<AppShutdown>().on_exit_requested() {
                ExitRequestAction::Allow => {}
                ExitRequestAction::Prevent => api.prevent_exit(),
                ExitRequestAction::StartDrain => {
                    api.prevent_exit();
                    let app = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let result = {
                            let active = app.state::<ActiveProject>();
                            drain_active_project(&active, PROJECT_DRAIN_TIMEOUT).await
                        };
                        let shutdown = app.state::<AppShutdown>();
                        match result {
                            Ok(()) => {
                                shutdown.allow_exit();
                                app.exit(code.unwrap_or(0));
                            }
                            Err(error) => {
                                eprintln!(
                                    "ats-desktop: application shutdown drain blocked: {error}"
                                );
                                shutdown.retry_after_failure();
                            }
                        }
                    });
                }
            }
        });
}
