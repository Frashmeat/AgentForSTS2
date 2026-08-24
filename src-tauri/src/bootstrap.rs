use std::path::PathBuf;
use std::sync::Arc;

use ats_adapters::SettingsStore;
use ats_workspace::AppDataPaths;

use crate::commands::failure::{CommandFailure, CommandResult};
use crate::composition::Stage2Composition;
use crate::{APP_DATA_ROOT_ENV, AppConfig};

pub(crate) struct DesktopRuntime {
    pub app_data: AppDataPaths,
    pub config: Arc<AppConfig>,
    pub composition: Arc<Stage2Composition>,
}

pub(crate) fn load() -> CommandResult<DesktopRuntime> {
    let app_data = std::env::var_os(APP_DATA_ROOT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(AppDataPaths::from_root)
        .unwrap_or_else(AppDataPaths::resolve);
    let legacy_config = std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(SettingsStore::desktop_legacy_config_path);
    let (settings, status) =
        SettingsStore::load_desktop(&app_data.config_path, legacy_config.as_deref());
    app_data
        .ensure_dirs()
        .map_err(|_| CommandFailure::storage("desktop.bootstrap"))?;
    let composition = Stage2Composition::built_in(app_data.root.clone())
        .map_err(|_| CommandFailure::pack_invalid("desktop.bootstrap"))?;
    Ok(DesktopRuntime {
        app_data,
        config: Arc::new(AppConfig::new(settings, status)),
        composition: Arc::new(composition),
    })
}
