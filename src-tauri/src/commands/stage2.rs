use std::path::PathBuf;
use std::sync::Arc;

use ats_adapters::{FileTruthSnapshotRepository, Sts2TruthImporter};
use ats_features::{FeatureContract, built_in_feature_contracts};
use ats_game_context::TruthSnapshotRepository;
use ats_kernel::{FeatureId, Sha256Digest};
use ats_runtime::{
    CancellationReason, CancellationToken, RunId, RunRecord, RunSummary, VersionedPayload,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppConfig;
use crate::commands::failure::{CommandFailure, CommandResult};
use crate::composition::Stage2Composition;
use crate::project_session::{ActiveProject, SubmitError};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitFeatureRequest {
    pub feature_id: FeatureId,
    pub request: VersionedPayload,
    #[serde(default)]
    pub source_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruthStatus {
    pub ready: bool,
    pub snapshot_id: Option<Sha256Digest>,
}

#[tauri::command]
pub fn get_feature_catalog() -> Vec<FeatureContract> {
    built_in_feature_contracts()
}

#[tauri::command]
pub async fn submit_feature(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
    config: State<'_, Arc<AppConfig>>,
    submission: SubmitFeatureRequest,
) -> CommandResult<RunId> {
    let session = current_session(&active, "run.submit")?;
    let run = RunRecord::new(submission.feature_id, submission.request);
    let root = session.path().to_path_buf();
    let meta = session.meta().clone();
    let composition = Arc::clone(composition.inner());
    let config = Arc::clone(config.inner());
    let source_path = submission.source_path.map(PathBuf::from);
    session
        .submit(run, move |run, cancellation, repository| async move {
            composition
                .execute(
                    &config,
                    &root,
                    &meta,
                    run,
                    repository.as_ref(),
                    source_path,
                    &cancellation,
                )
                .await
        })
        .await
        .map_err(|error| match error {
            SubmitError::Closing => CommandFailure::project_closing("run.submit"),
            SubmitError::Repository => CommandFailure::storage("run.submit"),
        })
}

#[tauri::command]
pub fn get_run(active: State<'_, ActiveProject>, run_id: String) -> CommandResult<RunRecord> {
    let session = current_session(&active, "run.get")?;
    let id = RunId::parse(run_id).map_err(|_| CommandFailure::invalid_input("run.get"))?;
    session
        .repository()
        .get(&id)
        .map_err(|_| CommandFailure::storage("run.get"))
}

#[tauri::command]
pub fn list_runs(active: State<'_, ActiveProject>) -> CommandResult<Vec<RunSummary>> {
    current_session(&active, "run.list")?
        .repository()
        .list()
        .map_err(|_| CommandFailure::storage("run.list"))
}

#[tauri::command]
pub async fn cancel_run(active: State<'_, ActiveProject>, run_id: String) -> CommandResult<bool> {
    let session = current_session(&active, "run.cancel")?;
    let id = RunId::parse(run_id).map_err(|_| CommandFailure::invalid_input("run.cancel"))?;
    Ok(session.cancel_run(&id, CancellationReason::User).await)
}

#[tauri::command]
pub fn get_truth_status(
    composition: State<'_, Arc<Stage2Composition>>,
) -> CommandResult<TruthStatus> {
    let repository = FileTruthSnapshotRepository::new(composition.runtime_root().to_path_buf());
    let snapshot = repository
        .open_current(composition.pack())
        .map_err(|_| CommandFailure::truth_missing("truth.status"))?;
    Ok(TruthStatus {
        ready: snapshot.is_some(),
        snapshot_id: snapshot.map(|value| value.manifest().snapshot_id().clone()),
    })
}

#[tauri::command]
pub async fn import_truth(
    composition: State<'_, Arc<Stage2Composition>>,
) -> CommandResult<TruthStatus> {
    let composition = Arc::clone(composition.inner());
    tauri::async_runtime::spawn_blocking(move || {
        Sts2TruthImporter::import_current(
            composition.runtime_root(),
            composition.pack(),
            &CancellationToken::new(),
        )
        .map_err(|_| CommandFailure::truth_missing("truth.import"))?;
        let repository = FileTruthSnapshotRepository::new(composition.runtime_root().to_path_buf());
        let snapshot = repository
            .open_current(composition.pack())
            .map_err(|_| CommandFailure::truth_missing("truth.import"))?
            .ok_or_else(|| CommandFailure::truth_missing("truth.import"))?;
        Ok(TruthStatus {
            ready: true,
            snapshot_id: Some(snapshot.manifest().snapshot_id().clone()),
        })
    })
    .await
    .map_err(|_| CommandFailure::unclassified("truth.import_join"))?
}

fn current_session(
    active: &State<'_, ActiveProject>,
    stage: &str,
) -> CommandResult<Arc<crate::project_session::ProjectSession>> {
    active
        .current()
        .map_err(|_| CommandFailure::unclassified(stage))?
        .ok_or_else(|| CommandFailure::project_not_open(stage))
}
