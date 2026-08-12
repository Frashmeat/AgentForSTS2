use std::path::Path;
use std::sync::Arc;

use ats_features::FeatureSpec;
use ats_features::project_create::{
    ProjectCreateError, ProjectCreateFeature, ProjectCreateRequest, ProjectCreateService,
};
use ats_runtime::{CancellationReason, RunRecord, RunTransition, VersionedPayload};
use ats_workspace::{
    LocalBuildPaths, ProjectError, ProjectFolder, RecentEntry, RecentProjects,
    sync_project_local_props,
};
use chrono::Utc;
use serde::Serialize;
use tauri::State;

use crate::commands::failure::{CommandFailure, CommandResult};
use crate::composition::Stage2Composition;
use crate::project_session::{ActiveProject, PROJECT_DRAIN_TIMEOUT, ProjectSession};
use crate::{AppConfig, AppPaths};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentProject {
    pub path: String,
    pub name: String,
    pub csharp_name: String,
    pub game_id: String,
    pub closing: bool,
}

#[tauri::command]
pub fn list_recent_projects(paths: State<'_, AppPaths>) -> Vec<RecentEntry> {
    RecentProjects::load(&paths.recents_path()).items
}

#[tauri::command]
pub async fn create_project(
    active: State<'_, ActiveProject>,
    paths: State<'_, AppPaths>,
    config: State<'_, Arc<AppConfig>>,
    composition: State<'_, Arc<Stage2Composition>>,
    parent_dir: String,
    name: String,
) -> CommandResult<CurrentProject> {
    let local_paths = configured_local_build_paths(&config)?;
    let _lifecycle = active.lifecycle.lock().await;
    drain_previous(&active, CancellationReason::ProjectSwitch).await?;
    let request = ProjectCreateRequest { name };
    let contributions = composition
        .resolve(
            &ProjectCreateFeature::id(),
            &[ProjectCreateFeature::contribution_requirement()],
        )
        .map_err(|_| CommandFailure::invalid_input("project.create_contribution"))?;
    let (folder, result) = ProjectCreateService
        .execute(
            Path::new(&parent_dir),
            &request,
            composition.pack(),
            &contributions,
            &local_paths,
        )
        .map_err(|error| project_create_failure("project.create", error))?;
    let session = ProjectSession::open(folder)
        .map_err(|_| CommandFailure::storage("project.create_session"))?;
    let payload = VersionedPayload::from_typed(ProjectCreateFeature::request_schema(), &request)
        .map_err(|_| CommandFailure::invalid_input("project.create_request"))?;
    let mut run = RunRecord::new(ProjectCreateFeature::id(), payload);
    run.apply_transition(RunTransition::Start, Utc::now())
        .map_err(|_| CommandFailure::unclassified("project.create_start"))?;
    let result_payload =
        VersionedPayload::from_typed(ProjectCreateFeature::result_schema(), &result)
            .map_err(|_| CommandFailure::unclassified("project.create_result"))?;
    run.apply_transition(
        RunTransition::Succeed {
            result: result_payload,
        },
        Utc::now(),
    )
    .map_err(|_| CommandFailure::unclassified("project.create_finish"))?;
    session
        .repository()
        .create(&run)
        .map_err(|_| CommandFailure::storage("project.create_run"))?;
    record_recent(&paths, &session)?;
    active
        .replace(Some(Arc::clone(&session)))
        .map_err(|_| CommandFailure::unclassified("project.create_activate"))?;
    Ok(snapshot(&session))
}

#[tauri::command]
pub async fn open_project(
    active: State<'_, ActiveProject>,
    paths: State<'_, AppPaths>,
    config: State<'_, Arc<AppConfig>>,
    composition: State<'_, Arc<Stage2Composition>>,
    path: String,
) -> CommandResult<CurrentProject> {
    let local_paths = configured_local_build_paths(&config)?;
    let _lifecycle = active.lifecycle.lock().await;
    drain_previous(&active, CancellationReason::ProjectSwitch).await?;
    let folder = ProjectFolder::open(Path::new(&path))
        .map_err(|error| project_failure("project.open", error))?;
    if folder.meta().game_id != composition.pack().id().as_str() {
        return Err(CommandFailure::invalid_input("project.open_pack"));
    }
    sync_local_config(folder.path(), &local_paths)?;
    let session = ProjectSession::open(folder)
        .map_err(|_| CommandFailure::storage("project.open_session"))?;
    record_recent(&paths, &session)?;
    active
        .replace(Some(Arc::clone(&session)))
        .map_err(|_| CommandFailure::unclassified("project.open_activate"))?;
    Ok(snapshot(&session))
}

#[tauri::command]
pub async fn close_project(active: State<'_, ActiveProject>) -> CommandResult<()> {
    let _lifecycle = active.lifecycle.lock().await;
    drain_previous(&active, CancellationReason::ProjectClose).await
}

#[tauri::command]
pub fn current_project(active: State<'_, ActiveProject>) -> CommandResult<Option<CurrentProject>> {
    active
        .current()
        .map(|value| value.as_deref().map(snapshot))
        .map_err(|_| CommandFailure::unclassified("project.current"))
}

#[tauri::command]
pub fn forget_recent_project(paths: State<'_, AppPaths>, path: String) -> CommandResult<()> {
    let recents_path = paths.recents_path();
    let mut recents = RecentProjects::load(&recents_path);
    recents.forget(Path::new(&path));
    recents
        .save(&recents_path)
        .map_err(|_| CommandFailure::storage("project.recents_forget"))
}

async fn drain_previous(active: &ActiveProject, reason: CancellationReason) -> CommandResult<()> {
    let Some(session) = active
        .current()
        .map_err(|_| CommandFailure::unclassified("project.switch_current"))?
    else {
        return Ok(());
    };
    session
        .cancel_and_drain(reason, PROJECT_DRAIN_TIMEOUT)
        .await
        .map_err(|_| CommandFailure::project_closing("project.drain"))?;
    session
        .release_project_lock()
        .map_err(|_| CommandFailure::unclassified("project.unlock"))?;
    active
        .replace(None)
        .map_err(|_| CommandFailure::unclassified("project.deactivate"))
}

fn record_recent(paths: &AppPaths, session: &ProjectSession) -> CommandResult<()> {
    let recents_path = paths.recents_path();
    let mut recents = RecentProjects::load(&recents_path);
    recents.record(session.path(), session.meta());
    recents
        .save(&recents_path)
        .map_err(|_| CommandFailure::storage("project.recents_save"))
}

fn snapshot(session: &ProjectSession) -> CurrentProject {
    CurrentProject {
        path: session.path().display().to_string(),
        name: session.meta().name.clone(),
        csharp_name: session.meta().csharp_name.clone(),
        game_id: session.meta().game_id.clone(),
        closing: session.is_closing(),
    }
}

fn configured_local_build_paths(config: &AppConfig) -> CommandResult<LocalBuildPaths> {
    let settings = config.settings_snapshot();
    let paths = LocalBuildPaths {
        sts2_assembly_path: settings.knowledge.sts2_dll_path.into(),
        godot_executable_path: settings.toolchain.godot_exe_path.into(),
    };
    paths
        .validate()
        .map_err(|_| CommandFailure::project_local_environment("project.local_props"))?;
    Ok(paths)
}

fn sync_local_config(project_root: &Path, paths: &LocalBuildPaths) -> CommandResult<()> {
    sync_project_local_props(project_root, paths)
        .map_err(|_| CommandFailure::project_local_environment("project.local_props"))
}

fn project_failure(stage: &str, error: ProjectError) -> CommandFailure {
    match error {
        ProjectError::Locked => CommandFailure::project_locked(stage),
        ProjectError::InvalidPath
        | ProjectError::InvalidName
        | ProjectError::AlreadyExists
        | ProjectError::InvalidMetadata
        | ProjectError::UnsupportedSchema
        | ProjectError::InvalidTemplate => CommandFailure::invalid_input(stage),
        ProjectError::LocalConfig(_) => CommandFailure::project_local_environment(stage),
        ProjectError::Io(_) | ProjectError::Json(_) => CommandFailure::storage(stage),
    }
}

fn project_create_failure(stage: &str, error: ProjectCreateError) -> CommandFailure {
    match error {
        ProjectCreateError::Project(error) => project_failure(stage, error),
        ProjectCreateError::InvalidInput
        | ProjectCreateError::ContextIdentityMismatch
        | ProjectCreateError::Contribution(_)
        | ProjectCreateError::Template(_) => CommandFailure::invalid_input(stage),
    }
}
