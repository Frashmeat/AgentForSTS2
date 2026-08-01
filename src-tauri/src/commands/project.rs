//! Project commands —— 工程文件夹生命周期。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ats_core::config::Settings;
use ats_core::game_pack::GamePackRegistry;
use ats_core::platform::domain::CancellationReason;
use ats_core::project::{
    LocalBuildInputs, LocalPropsSync, ProjectFolder, ProjectMeta, RecentEntry, RecentProjects,
    sync_local_props,
};
use ats_core::toolchain::validate_godot_executable;
use serde::Serialize;
use tauri::{Emitter, State};

use crate::commands::failure::{CommandFailure, CommandResult};
use crate::project_session::{ActiveProject, PROJECT_DRAIN_TIMEOUT, ProjectSession};
use crate::{AppConfig, AppPaths};

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
pub async fn create_project(
    app: tauri::AppHandle,
    config: State<'_, AppConfig>,
    paths: State<'_, AppPaths>,
    active: State<'_, ActiveProject>,
    parent_dir: String,
    name: String,
    game_id: String,
) -> CommandResult<ProjectSnapshot> {
    let _lifecycle = active.lifecycle.lock().await;
    let previous = prepare_switch(&active, CancellationReason::ProjectSwitch).await?;
    let parent = PathBuf::from(parent_dir);
    let folder = ProjectFolder::create(&parent, &name, &game_id)
        .map_err(|error| CommandFailure::project("project.create", &error))?;
    let session = ProjectSession::open(folder)
        .await
        .map_err(|error| CommandFailure::run("project.reconcile", &error))?;
    let snap = snapshot(&session);
    record_recent(&paths, session.path(), session.meta())?;
    if let Some(previous) = &previous {
        previous
            .release_project_lock()
            .map_err(|_| CommandFailure::unclassified("project.active_lock"))?;
    }
    active
        .replace(Some(Arc::clone(&session)))
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))?;
    drop(previous);

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
pub async fn open_project(
    app: tauri::AppHandle,
    config: State<'_, AppConfig>,
    paths: State<'_, AppPaths>,
    active: State<'_, ActiveProject>,
    path: String,
) -> CommandResult<ProjectSnapshot> {
    let p = PathBuf::from(path);
    let _lifecycle = active.lifecycle.lock().await;
    if let Some(current) = active
        .current()
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))?
        && same_project_path(current.path(), &p)
    {
        return Ok(snapshot(&current));
    }
    let previous = prepare_switch(&active, CancellationReason::ProjectSwitch).await?;
    let folder =
        ProjectFolder::open(&p).map_err(|error| CommandFailure::project("project.open", &error))?;
    let session = ProjectSession::open(folder)
        .await
        .map_err(|error| CommandFailure::run("project.reconcile", &error))?;
    let snap = snapshot(&session);
    record_recent(&paths, session.path(), session.meta())?;
    if let Some(previous) = &previous {
        previous
            .release_project_lock()
            .map_err(|_| CommandFailure::unclassified("project.active_lock"))?;
    }
    active
        .replace(Some(Arc::clone(&session)))
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))?;
    drop(previous);

    // 老工程可能没有 local.props——自动从配置中的 STS2 DLL 路径补齐
    if sync_project_local_props(&p, &snap.meta.game_id, &config.settings_snapshot()).is_err() {
        eprintln!("local.props auto-generation was skipped while opening a project");
    }

    app.emit("project-changed", Some(snap.clone())).ok();
    Ok(snap)
}

#[tauri::command]
pub async fn close_project(
    app: tauri::AppHandle,
    active: State<'_, ActiveProject>,
) -> CommandResult<()> {
    let _lifecycle = active.lifecycle.lock().await;
    let Some(session) = active
        .current()
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))?
    else {
        return Ok(());
    };
    drain_session(&session, CancellationReason::ProjectClose).await?;
    session
        .release_project_lock()
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))?;
    active
        .replace(None)
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))?;
    drop(session);
    app.emit("project-changed", Option::<ProjectSnapshot>::None)
        .ok();
    Ok(())
}

#[tauri::command]
pub fn current_project(active: State<'_, ActiveProject>) -> CommandResult<Option<ProjectSnapshot>> {
    Ok(active
        .current()
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))?
        .as_deref()
        .map(snapshot))
}

#[tauri::command]
pub fn forget_recent_project(paths: State<'_, AppPaths>, path: String) -> CommandResult<()> {
    let mut r = RecentProjects::load(&paths.recents_path());
    r.forget(&PathBuf::from(path));
    r.save(&paths.recents_path())
        .map_err(|_| CommandFailure::unclassified("project.recents_save"))?;
    Ok(())
}

fn snapshot(session: &ProjectSession) -> ProjectSnapshot {
    ProjectSnapshot {
        path: session.path().to_string_lossy().to_string(),
        meta: session.meta().clone(),
    }
}

async fn prepare_switch(
    active: &ActiveProject,
    reason: CancellationReason,
) -> CommandResult<Option<Arc<ProjectSession>>> {
    let previous = active
        .current()
        .map_err(|_| CommandFailure::unclassified("project.active_lock"))?;
    if let Some(session) = &previous {
        drain_session(session, reason).await?;
    }
    Ok(previous)
}

async fn drain_session(session: &ProjectSession, reason: CancellationReason) -> CommandResult<()> {
    session
        .cancel_and_drain(reason, PROJECT_DRAIN_TIMEOUT)
        .await
        .map_err(|timeout| {
            CommandFailure::project_close_timeout(
                "project.close",
                timeout.blocked_runs.first().map(|id| id.0.as_str()),
            )
        })
}

fn same_project_path(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
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

#[cfg(test)]
mod tests {
    use ats_core::platform::domain::{RunKind, RunRecord, RunRepository, RunStatus};
    use ats_core::platform::{CancellationToken, SpawnedRun};

    use super::*;

    #[tokio::test]
    async fn prepare_switch_drains_with_project_switch_reason() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = ProjectFolder::create(temp.path(), "sample", "sts2").unwrap();
        let session = ProjectSession::open(project).await.unwrap();
        let repository = session.file_repository();
        let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        let run_id = run.id.clone();
        repository.create(&run).await.unwrap();
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let spawned_run_id = run_id.clone();
        session
            .submit(async move {
                Ok(SpawnedRun {
                    run_id: spawned_run_id,
                    cancellation,
                    task: tokio::spawn(async move {
                        worker_cancellation.cancelled().await;
                    }),
                })
            })
            .await
            .unwrap();
        let active = ActiveProject::new();
        active.replace(Some(Arc::clone(&session))).unwrap();

        let previous = prepare_switch(&active, CancellationReason::ProjectSwitch)
            .await
            .unwrap()
            .unwrap();

        assert!(Arc::ptr_eq(&previous, &session));
        assert!(session.is_closing());
        let record = repository.get(&run_id).await.unwrap();
        assert_eq!(record.status, RunStatus::Cancelled);
        assert_eq!(
            record.timeline.last().unwrap().cancellation_reason,
            Some(CancellationReason::ProjectSwitch)
        );
    }
}
