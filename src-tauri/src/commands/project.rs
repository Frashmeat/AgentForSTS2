//! Project commands —— 工程文件夹生命周期。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use ats_core::config::Settings;
use ats_core::game_pack::GamePackRegistry;
use ats_core::project::{
    LocalBuildInputs, LocalPropsSync, ProjectFolder, ProjectMeta, RecentEntry, RecentProjects,
    sync_local_props,
};
use ats_core::toolchain::validate_godot_executable;
use serde::Serialize;
use tauri::{Emitter, State};

use crate::commands::failure::{CommandFailure, CommandResult};
use crate::{AppConfig, AppPaths};

/// 当前活动工程的进程内单例。`None` 表示用户尚未打开任何工程。
pub struct ActiveProject(pub Mutex<Option<ProjectFolder>>);

impl ActiveProject {
    #[must_use]
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

impl Default for ActiveProject {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSnapshot {
    pub path: String,
    pub meta: ProjectMeta,
}

#[tauri::command]
pub fn list_recent_projects(paths: State<'_, AppPaths>) -> Vec<RecentEntry> {
    let r = RecentProjects::load(&paths.recents_path());
    r.items
}

#[tauri::command]
pub fn create_project(
    app: tauri::AppHandle,
    config: State<'_, AppConfig>,
    paths: State<'_, AppPaths>,
    active: State<'_, ActiveProject>,
    parent_dir: String,
    name: String,
    game_id: String,
) -> CommandResult<ProjectSnapshot> {
    let parent = PathBuf::from(parent_dir);
    let folder = ProjectFolder::create(&parent, &name, &game_id)
        .map_err(|error| CommandFailure::project("project.create", &error))?;
    let snap = snapshot(&folder);
    record_recent(&paths, folder.path(), folder.meta())?;
    *lock_active(&active)? = Some(folder);

    // 尝试从配置中的 STS2 DLL 路径自动生成 local.props
    if sync_project_local_props(
        Path::new(&snap.path),
        &snap.meta.game_id,
        &config.settings_snapshot(),
    )
    .is_err()
    {
        eprintln!("local.props auto-generation was skipped");
    }

    app.emit("project-changed", Some(snap.clone())).ok();
    Ok(snap)
}
#[tauri::command]
pub fn open_project(
    app: tauri::AppHandle,
    config: State<'_, AppConfig>,
    paths: State<'_, AppPaths>,
    active: State<'_, ActiveProject>,
    path: String,
) -> CommandResult<ProjectSnapshot> {
    let p = PathBuf::from(path);
    drop(lock_active(&active)?.take());
    let folder =
        ProjectFolder::open(&p).map_err(|error| CommandFailure::project("project.open", &error))?;
    let snap = snapshot(&folder);
    record_recent(&paths, folder.path(), folder.meta())?;
    *lock_active(&active)? = Some(folder);

    // 老工程可能没有 local.props——自动从配置中的 STS2 DLL 路径补齐
    if sync_project_local_props(&p, &snap.meta.game_id, &config.settings_snapshot()).is_err() {
        eprintln!("local.props auto-generation was skipped while opening a project");
    }

    app.emit("project-changed", Some(snap.clone())).ok();
    Ok(snap)
}

#[tauri::command]
pub fn close_project(app: tauri::AppHandle, active: State<'_, ActiveProject>) -> CommandResult<()> {
    drop(lock_active(&active)?.take());
    app.emit("project-changed", Option::<ProjectSnapshot>::None)
        .ok();
    Ok(())
}

#[tauri::command]
pub fn current_project(active: State<'_, ActiveProject>) -> CommandResult<Option<ProjectSnapshot>> {
    let guard = lock_active(&active)?;
    Ok(guard.as_ref().map(snapshot))
}

#[tauri::command]
pub fn forget_recent_project(paths: State<'_, AppPaths>, path: String) -> CommandResult<()> {
    let mut r = RecentProjects::load(&paths.recents_path());
    r.forget(&PathBuf::from(path));
    r.save(&paths.recents_path())
        .map_err(|_| CommandFailure::unclassified("project.recents_save"))?;
    Ok(())
}

fn snapshot(folder: &ProjectFolder) -> ProjectSnapshot {
    ProjectSnapshot {
        path: folder.path().to_string_lossy().to_string(),
        meta: folder.meta().clone(),
    }
}

pub(crate) fn sync_project_local_props(
    project_root: &Path,
    game_id: &str,
    settings: &Settings,
) -> CommandResult<LocalPropsSync> {
    sync_project_local_props_with_mode(project_root, game_id, settings, true)
}

pub(crate) fn sync_project_local_props_after_settings(
    project_root: &Path,
    game_id: &str,
    settings: &Settings,
) -> CommandResult<LocalPropsSync> {
    sync_project_local_props_with_mode(project_root, game_id, settings, false)
}

fn sync_project_local_props_with_mode(
    project_root: &Path,
    game_id: &str,
    settings: &Settings,
    require_godot: bool,
) -> CommandResult<LocalPropsSync> {
    let sts2_dll_path = PathBuf::from(&settings.knowledge.sts2_dll_path);
    if settings.knowledge.sts2_dll_path.is_empty() {
        return Err(CommandFailure::local_props(
            "project.local_props",
            &ats_core::project::LocalPropsError::MissingInput("knowledge.sts2_dll_path".into()),
        ));
    }
    if !sts2_dll_path.is_file() {
        return Err(CommandFailure::invalid_input(
            "project.local_props",
            "The configured game assembly path is not a file.",
        ));
    }
    let godot_exe_path = PathBuf::from(&settings.toolchain.godot_exe_path);
    if require_godot && settings.toolchain.godot_exe_path.is_empty() {
        return Err(CommandFailure::local_props(
            "project.local_props",
            &ats_core::project::LocalPropsError::MissingInput("toolchain.godot_exe_path".into()),
        ));
    }
    if !settings.toolchain.godot_exe_path.is_empty() {
        validate_godot_executable(&godot_exe_path, Duration::from_secs(5))
            .map_err(|error| CommandFailure::toolchain("project.godot_validate", &error))?;
    }
    let registry = GamePackRegistry::built_in()
        .map_err(|_| CommandFailure::unclassified("project.game_pack_registry"))?;
    let pack = registry.require(game_id).map_err(|_| {
        CommandFailure::invalid_input(
            "project.game_pack",
            "The project references an unavailable Game Pack.",
        )
    })?;
    let inputs = LocalBuildInputs {
        values: BTreeMap::from([
            ("game_assembly".into(), sts2_dll_path),
            ("godot_executable".into(), godot_exe_path),
        ]),
    };
    sync_local_props(project_root, pack.build_recipe.as_ref(), &inputs)
        .map_err(|error| CommandFailure::local_props("project.local_props", &error))
}

fn record_recent(paths: &AppPaths, project_path: &Path, meta: &ProjectMeta) -> CommandResult<()> {
    let recents_path = paths.recents_path();
    let mut r = RecentProjects::load(&recents_path);
    r.record(project_path, meta);
    if let Some(parent) = recents_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            CommandFailure::io(
                "run.storage_failed",
                "project.recents_create",
                "The recent project list could not be saved.",
                &error,
            )
        })?;
    }
    r.save(&recents_path)
        .map_err(|_| CommandFailure::unclassified("project.recents_save"))
}

fn lock_active<'a>(
    active: &'a State<'_, ActiveProject>,
) -> CommandResult<std::sync::MutexGuard<'a, Option<ProjectFolder>>> {
    active
        .0
        .lock()
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))
}
