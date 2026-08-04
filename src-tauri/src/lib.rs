//! AgentTheSpire desktop Stage 2 composition root.

mod app_shutdown;
mod build_identity;
mod commands;
mod composition;
mod project_session;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use ats_adapters::{ConfigStatus, Settings, SettingsStore};
use ats_workspace::AppDataPaths;
use tauri::Manager;

use crate::app_shutdown::{AppShutdown, ExitRequestAction, drain_active_project};
use crate::composition::Stage2Composition;
use crate::project_session::{ActiveProject, PROJECT_DRAIN_TIMEOUT};

const APP_DATA_ROOT_ENV: &str = "SPIREFORGE_APP_DATA_ROOT";

pub struct AppConfig {
    inner: RwLock<AppConfigInner>,
}

struct AppConfigInner {
    settings: Settings,
    status: ConfigStatus,
}

impl AppConfig {
    #[must_use]
    pub fn new(settings: Settings, status: ConfigStatus) -> Self {
        Self {
            inner: RwLock::new(AppConfigInner { settings, status }),
        }
    }

    pub fn snapshot(&self) -> (Settings, ConfigStatus) {
        let value = self.inner.read().expect("AppConfig lock poisoned");
        (value.settings.clone(), value.status.clone())
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.inner
            .read()
            .expect("AppConfig lock poisoned")
            .settings
            .clone()
    }

    pub fn status_snapshot(&self) -> ConfigStatus {
        self.inner
            .read()
            .expect("AppConfig lock poisoned")
            .status
            .clone()
    }

    pub fn replace_settings(&self, settings: Settings) {
        self.inner
            .write()
            .expect("AppConfig lock poisoned")
            .settings = settings;
    }
}

pub struct AppPaths {
    data: AppDataPaths,
}

impl AppPaths {
    #[must_use]
    pub fn recents_path(&self) -> PathBuf {
        self.data.recent_projects_path.clone()
    }
}

#[must_use]
pub fn build_info() -> ats_kernel::BuildInfo {
    build_identity::current()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (settings, status) = SettingsStore::load(None);
    let app_data = std::env::var_os(APP_DATA_ROOT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(AppDataPaths::from_root)
        .unwrap_or_else(AppDataPaths::resolve);
    if let Err(error) = app_data.ensure_dirs() {
        eprintln!("ats-desktop: app data initialization failed: {error}");
    }
    let composition = Arc::new(
        Stage2Composition::built_in(app_data.root.clone())
            .expect("built-in Stage 2 composition is valid"),
    );
    let config = Arc::new(AppConfig::new(settings, status));

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
        .manage(config)
        .manage(AppPaths { data: app_data })
        .manage(composition)
        .manage(ActiveProject::new())
        .manage(AppShutdown::new())
        .invoke_handler(tauri::generate_handler![
            commands::health::get_health,
            commands::settings::get_settings_snapshot,
            commands::settings::save_settings_patch,
            commands::settings::open_config_in_editor,
            commands::project::list_recent_projects,
            commands::project::create_project,
            commands::project::open_project,
            commands::project::close_project,
            commands::project::current_project,
            commands::project::forget_recent_project,
            commands::stage2::get_feature_catalog,
            commands::stage2::get_truth_status,
            commands::stage2::import_truth,
            commands::stage2::get_item_capabilities,
            commands::stage2::list_item_definitions,
            commands::stage2::get_item_definition,
            commands::stage2::save_item_definition,
            commands::stage2::submit_feature,
            commands::stage2::get_run,
            commands::stage2::list_runs,
            commands::stage2::cancel_run,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Tauri application")
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
                            Err(_) => shutdown.retry_after_failure(),
                        }
                    });
                }
            }
        });
}
