//! Project commands —— 工程文件夹生命周期。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use ats_core::config::Settings;
use ats_core::project::{
    LocalBuildPaths, LocalPropsSync, ProjectFolder, ProjectMeta, RecentEntry, RecentProjects,
    sync_local_props,
};
use ats_core::toolchain::validate_godot_executable;
use serde::Serialize;
use tauri::{Emitter, State};

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
pub fn list_recent_projects(paths: State<'_, AppPaths>) -> Result<Vec<RecentEntry>, String> {
    let r = RecentProjects::load(&paths.recents_path());
    Ok(r.items)
}

#[tauri::command]
pub fn create_project(
    app: tauri::AppHandle,
    config: State<'_, AppConfig>,
    paths: State<'_, AppPaths>,
    active: State<'_, ActiveProject>,
    parent_dir: String,
    name: String,
) -> Result<ProjectSnapshot, String> {
    let parent = PathBuf::from(parent_dir);
    let folder = ProjectFolder::create(&parent, &name).map_err(|e| e.to_string())?;
    let snap = snapshot(&folder);
    record_recent(&paths, folder.path(), folder.meta())?;
    *lock_active(&active)? = Some(folder);

    // 尝试从配置中的 STS2 DLL 路径自动生成 local.props
    if let Err(warn) = sync_project_local_props(Path::new(&snap.path), &config.settings_snapshot())
    {
        eprintln!("local.props auto-generate skipped: {warn}");
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
) -> Result<ProjectSnapshot, String> {
    let p = PathBuf::from(path);
    drop(lock_active(&active)?.take());
    let folder = ProjectFolder::open(&p).map_err(|e| e.to_string())?;
    let snap = snapshot(&folder);
    record_recent(&paths, folder.path(), folder.meta())?;
    *lock_active(&active)? = Some(folder);

    // 老工程可能没有 local.props——自动从配置中的 STS2 DLL 路径补齐
    if let Err(warn) = sync_project_local_props(&p, &config.settings_snapshot()) {
        eprintln!("local.props auto-generate skipped on open: {warn}");
    }

    app.emit("project-changed", Some(snap.clone())).ok();
    Ok(snap)
}

#[tauri::command]
pub fn close_project(
    app: tauri::AppHandle,
    active: State<'_, ActiveProject>,
) -> Result<(), String> {
    drop(lock_active(&active)?.take());
    app.emit("project-changed", Option::<ProjectSnapshot>::None).ok();
    Ok(())
}

#[tauri::command]
pub fn current_project(
    active: State<'_, ActiveProject>,
) -> Result<Option<ProjectSnapshot>, String> {
    let guard = lock_active(&active)?;
    Ok(guard.as_ref().map(snapshot))
}

#[tauri::command]
pub fn forget_recent_project(paths: State<'_, AppPaths>, path: String) -> Result<(), String> {
    let mut r = RecentProjects::load(&paths.recents_path());
    r.forget(&PathBuf::from(path));
    r.save(&paths.recents_path()).map_err(|e| e.to_string())?;
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
    settings: &Settings,
) -> Result<LocalPropsSync, String> {
    sync_project_local_props_with_mode(project_root, settings, true)
}

pub(crate) fn sync_project_local_props_after_settings(
    project_root: &Path,
    settings: &Settings,
) -> Result<LocalPropsSync, String> {
    sync_project_local_props_with_mode(project_root, settings, false)
}

fn sync_project_local_props_with_mode(
    project_root: &Path,
    settings: &Settings,
    require_godot: bool,
) -> Result<LocalPropsSync, String> {
    let sts2_dll_path = PathBuf::from(&settings.knowledge.sts2_dll_path);
    if settings.knowledge.sts2_dll_path.is_empty() {
        return Err("knowledge.sts2_dll_path is not configured".into());
    }
    if !sts2_dll_path.is_file() {
        return Err(format!(
            "configured STS2 DLL is not a file: {}",
            sts2_dll_path.display()
        ));
    }
    let godot_exe_path = PathBuf::from(&settings.toolchain.godot_exe_path);
    if require_godot && settings.toolchain.godot_exe_path.is_empty() {
        return Err("toolchain.godot_exe_path is not configured".into());
    }
    if !settings.toolchain.godot_exe_path.is_empty() {
        validate_godot_executable(&godot_exe_path, Duration::from_secs(5))
            .map_err(|error| format!("Godot validation failed: {error}"))?;
    }
    sync_local_props(
        project_root,
        &LocalBuildPaths {
            sts2_dll_path,
            godot_exe_path,
        },
    )
    .map_err(|error| format!("sync local.props: {error}"))
}

fn record_recent(paths: &AppPaths, project_path: &Path, meta: &ProjectMeta) -> Result<(), String> {
    let recents_path = paths.recents_path();
    let mut r = RecentProjects::load(&recents_path);
    r.record(project_path, meta);
    if let Some(parent) = recents_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create app data dir: {e}"))?;
    }
    r.save(&recents_path)
        .map_err(|e| format!("save recents: {e}"))
}

fn lock_active<'a>(
    active: &'a State<'_, ActiveProject>,
) -> Result<std::sync::MutexGuard<'a, Option<ProjectFolder>>, String> {
    active
        .0
        .lock()
        .map_err(|e| format!("active project mutex poisoned: {e}"))
}
