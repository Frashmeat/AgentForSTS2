//! Project commands —— 工程文件夹生命周期。

use std::path::{Path, PathBuf};
use tauri::Emitter;
use std::sync::Mutex;

use ats_core::project::{ProjectFolder, ProjectMeta, RecentEntry, RecentProjects};
use serde::Serialize;
use tauri::State;

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
    let sts2 = &config.settings_snapshot().knowledge.sts2_dll_path;
    if !sts2.is_empty()
        && let Err(warn) = try_generate_local_props(Path::new(&snap.path), sts2)
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
    let sts2 = &config.settings_snapshot().knowledge.sts2_dll_path;
    if !sts2.is_empty()
        && let Err(warn) = try_generate_local_props(&p, sts2)
    {
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

/// 从配置中的 STS2 DLL 路径推导出 SteamLibraryPath，
/// 并自动生成 `local.props`，使新建工程可立即编译。
fn try_generate_local_props(project_root: &Path, sts2_dll_path: &str) -> Result<(), String> {
    let dll = Path::new(sts2_dll_path);
    // 向上追溯到包含 steamapps 的父目录
    let mut steam = dll.to_path_buf();
    loop {
        if steam.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steamapps")) {
            break;
        }
        steam = steam
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "cannot find steamapps ancestor".to_string())?;
    }
    let example_path = project_root.join("local.props.example");
    let example = std::fs::read_to_string(&example_path)
        .map_err(|e| format!("read local.props.example: {e}"))?;
    let props = example.replace(
        "C:/Program Files (x86)/Steam/steamapps",
        &steam.to_string_lossy(),
    );
    let target = project_root.join("local.props");
    std::fs::write(&target, &props)
        .map_err(|e| format!("write local.props: {e}"))?;
    Ok(())
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