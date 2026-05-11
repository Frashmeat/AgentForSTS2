//! Project commands —— 工程文件夹生命周期。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ats_core::project::{ProjectFolder, ProjectMeta, RecentEntry, RecentProjects};
use serde::Serialize;
use tauri::State;

use crate::AppPaths;

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
pub fn list_recent_projects(
    paths: State<'_, AppPaths>,
) -> Result<Vec<RecentEntry>, String> {
    let r = RecentProjects::load(&paths.recents_path());
    Ok(r.items)
}

#[tauri::command]
pub fn create_project(
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
    Ok(snap)
}

#[tauri::command]
pub fn open_project(
    paths: State<'_, AppPaths>,
    active: State<'_, ActiveProject>,
    path: String,
) -> Result<ProjectSnapshot, String> {
    let p = PathBuf::from(path);
    // 关闭旧工程（释放 lock）
    drop(lock_active(&active)?.take());
    let folder = ProjectFolder::open(&p).map_err(|e| e.to_string())?;
    let snap = snapshot(&folder);
    record_recent(&paths, folder.path(), folder.meta())?;
    *lock_active(&active)? = Some(folder);
    Ok(snap)
}

#[tauri::command]
pub fn close_project(active: State<'_, ActiveProject>) -> Result<(), String> {
    drop(lock_active(&active)?.take());
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
pub fn forget_recent_project(
    paths: State<'_, AppPaths>,
    path: String,
) -> Result<(), String> {
    let mut r = RecentProjects::load(&paths.recents_path());
    r.forget(&PathBuf::from(path));
    r.save(&paths.recents_path()).map_err(|e| e.to_string())?;
    Ok(())
}

fn snapshot(folder: &ProjectFolder) -> ProjectSnapshot {
    ProjectSnapshot {
        path: folder.path().display().to_string(),
        meta: folder.meta().clone(),
    }
}

fn record_recent(
    paths: &AppPaths,
    project_path: &Path,
    meta: &ProjectMeta,
) -> Result<(), String> {
    let recents_path = paths.recents_path();
    let mut r = RecentProjects::load(&recents_path);
    r.record(project_path, meta);
    if let Some(parent) = recents_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create app data dir: {e}"))?;
    }
    r.save(&recents_path).map_err(|e| format!("save recents: {e}"))?;
    Ok(())
}

fn lock_active<'a>(
    active: &'a State<'_, ActiveProject>,
) -> Result<std::sync::MutexGuard<'a, Option<ProjectFolder>>, String> {
    active
        .0
        .lock()
        .map_err(|e| format!("active project mutex poisoned: {e}"))
}
