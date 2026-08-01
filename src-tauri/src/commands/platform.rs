//! Platform commands —— RunRecord 生命周期 + text/code/build 三类 handler 提交。
//!
//! Repository 用 ActiveProject 的 history_dir 作存储路径；切换项目时下条提交
//! 会落到新工程。LLM client 每次新建（Arc 包裹的开销极小，避免与配置变更竞态）。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ats_core::game_pack::{
    GamePackRegistry, GitHubReleaseAssetFetcher, IlspycmdTruthIndexer, TruthSnapshotRefresher,
    TruthSnapshotStore, VerifiedGameContext, validate_truth_source_inputs,
};
use ats_core::image_gen::{ImageGenClient, build_from_config as build_image_gen};
use ats_core::image_proc::{BgRemoverChain, ImageProcClient};
use ats_core::llm::{LlmClient, build_from_config};
use ats_core::platform::domain::{CancellationReason, RunError};
use ats_core::platform::{
    ProgressEvent, ProgressSink, RunApplicationService, RunId, RunRecord, RunSummary,
    SubmitAssetGenerateRequest, SubmitBatchCustomCodeRequest, SubmitBuildProjectRequest,
    SubmitCodeGenerateRequest, SubmitLogAnalysisRequest, SubmitPackageProjectRequest, SubmitRunAck,
    SubmitSingleAssetPlanRequest, SubmitTextGenerateRequest, SubmitTruthSnapshotRefreshRequest,
};
use tauri::{AppHandle, Emitter, State};

use crate::AppConfig;
use crate::commands::failure::{CommandFailure, CommandResult};
use crate::commands::project::sync_project_local_props;
use crate::project_session::{ActiveProject, ProjectSession, SubmitError};

const RUN_PROGRESS_EVENT: &str = "run-progress";

#[tauri::command]
pub async fn submit_text_generate_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitTextGenerateRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    let service = build_service(&config, &session)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let run_id = session
        .submit(service.submit_text_generate(request, sink))
        .await
        .map_err(|error| submit_failure("run.submit_text", error))?;
    Ok(SubmitRunAck { run_id })
}

#[tauri::command]
pub async fn get_run(
    _config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    id: String,
) -> CommandResult<RunRecord> {
    active_session(&active)?
        .repository()
        .get(&RunId(id))
        .await
        .map_err(|error| CommandFailure::run("run.get", &error))
}

#[tauri::command]
pub async fn list_runs(
    _config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
) -> CommandResult<Vec<RunSummary>> {
    active_session(&active)?
        .repository()
        .list()
        .await
        .map_err(|error| CommandFailure::run("run.list", &error))
}

#[tauri::command]
pub async fn cancel_run(
    _config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    id: String,
) -> CommandResult<()> {
    let session = active_session(&active)?;
    let run_id = RunId(id);
    if session.cancel_run(&run_id, CancellationReason::User).await {
        return Ok(());
    }
    let record = session
        .repository()
        .get(&run_id)
        .await
        .map_err(|error| CommandFailure::run("run.cancel", &error))?;
    Err(CommandFailure::run(
        "run.cancel",
        &RunError::InvalidTransition {
            id: record.id.0,
            message: format!("run is not active ({:?})", record.status),
        },
    ))
}

#[tauri::command]
pub async fn submit_code_generate_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitCodeGenerateRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    if let SubmitCodeGenerateRequest::Asset { request: asset } = &request {
        sync_requested_project(&config, &session, &asset.project_root)?;
    }
    let service = build_service(&config, &session)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let artifacts_dir = session.artifacts_dir();
    let game_context = session_game_context(&config, &session)?;
    let run_id = session
        .submit(service.submit_code_generate(request, game_context, artifacts_dir, sink))
        .await
        .map_err(|error| submit_failure("run.submit_code", error))?;
    Ok(SubmitRunAck { run_id })
}

#[tauri::command]
pub async fn submit_asset_generate_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    image_proc_state: State<'_, Arc<crate::commands::image_proc_state::ImageProcState>>,
    request: SubmitAssetGenerateRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    sync_requested_project(&config, &session, &request.asset_request.project_root)?;
    let service = build_service(&config, &session)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let artifacts_dir = session.artifacts_dir();
    let game_context = session_game_context(&config, &session)?;
    let settings = config.settings_snapshot();
    let image_gen: Arc<dyn ImageGenClient> = build_image_gen(&settings.image_gen)
        .map_err(|error| CommandFailure::image("image.configure", &error))?;
    // BgRemoverChain：prewarm 阶段装好的 ML primary（feature on 且加载成功），
    // 没装则只用启发式 fallback。
    let primary = image_proc_state.primary();
    let image_proc: Arc<dyn ImageProcClient> =
        Arc::new(BgRemoverChain::with_simple_fallback(primary));
    let run_id = session
        .submit(service.submit_asset_generate(
            request,
            game_context,
            artifacts_dir,
            image_gen,
            image_proc,
            sink,
        ))
        .await
        .map_err(|error| submit_failure("run.submit_asset", error))?;
    Ok(SubmitRunAck { run_id })
}

#[tauri::command]
pub async fn submit_truth_snapshot_refresh_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitTruthSnapshotRefreshRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let service = RunApplicationService::without_llm(session.repository());
    let game_id = session.meta().game_id.clone();
    let registry = GamePackRegistry::built_in()
        .map_err(|_| CommandFailure::unclassified("truth_snapshot.registry"))?;
    let pack = registry
        .require(&game_id)
        .map_err(|_| {
            CommandFailure::invalid_input(
                "truth_snapshot.game_pack",
                "The active project references an unavailable Game Pack.",
            )
        })?
        .clone();
    let settings = config.settings_snapshot();
    let game_assembly = PathBuf::from(&settings.knowledge.sts2_dll_path);
    let local_inputs = BTreeMap::from([("game_assembly".into(), game_assembly)]);
    validate_truth_source_inputs(&pack, &local_inputs).map_err(|_| {
        CommandFailure::invalid_input(
            "truth_snapshot.inputs",
            "The configured truth source inputs are incomplete or invalid.",
        )
    })?;
    let github = GitHubReleaseAssetFetcher::with_default_client(
        Some(settings.runtime.workstation.github_token.clone()).filter(|token| !token.is_empty()),
    )
    .map_err(|_| CommandFailure::unclassified("truth_snapshot.github_client"))?;
    #[cfg(feature = "e2e")]
    let github = match std::env::var("ATS_E2E_BASELIB_RELEASE_URL") {
        Ok(url) if !url.is_empty() => github.with_release_url_override(url),
        _ => github,
    };
    #[cfg(feature = "e2e")]
    let explicit_ilspycmd = std::env::var_os("ATS_E2E_ILSPYCMD_PATH").map(PathBuf::from);
    #[cfg(not(feature = "e2e"))]
    let explicit_ilspycmd = None;
    let indexer = IlspycmdTruthIndexer::discover(explicit_ilspycmd)
        .map_err(|_| CommandFailure::unclassified("truth_snapshot.indexer"))?;
    let refresher = TruthSnapshotRefresher::new(Arc::new(github), Arc::new(indexer));
    let store = TruthSnapshotStore::new(&config.status_snapshot().runtime_dir(), &pack);
    let run_id = session
        .submit(service.submit_truth_snapshot_refresh(
            request,
            pack,
            store,
            local_inputs,
            refresher,
            sink,
        ))
        .await
        .map_err(|error| submit_failure("run.submit_truth_snapshot", error))?;
    Ok(SubmitRunAck { run_id })
}

#[tauri::command]
pub async fn submit_single_asset_plan_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitSingleAssetPlanRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    let service = build_service(&config, &session)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let items_dir = Some(session.items_dir());
    let game_id = session.meta().game_id.clone();
    let pack = GamePackRegistry::built_in()
        .map_err(|_| CommandFailure::unclassified("single_asset_plan.registry"))?
        .require(&game_id)
        .map_err(|_| {
            CommandFailure::invalid_input(
                "single_asset_plan.game_pack",
                "The active project references an unavailable Game Pack.",
            )
        })?
        .clone();
    let run_id = session
        .submit(service.submit_single_asset_plan(request, pack, items_dir, sink))
        .await
        .map_err(|error| submit_failure("run.submit_plan", error))?;
    Ok(SubmitRunAck { run_id })
}

#[tauri::command]
pub async fn submit_batch_custom_code_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitBatchCustomCodeRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    let service = build_service(&config, &session)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let artifacts_dir = session.artifacts_dir();
    let game_context = session_game_context(&config, &session)?;
    let run_id = session
        .submit(service.submit_batch_custom_code(request, game_context, artifacts_dir, sink))
        .await
        .map_err(|error| submit_failure("run.submit_batch", error))?;
    Ok(SubmitRunAck { run_id })
}

#[tauri::command]
pub async fn submit_package_project_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitPackageProjectRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    let service = build_service(&config, &session)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let game_context = session_game_context(&config, &session)?;
    let project_root = session.path().to_path_buf();
    let mod_id = session.meta().csharp_name.clone();
    let run_id = session
        .submit(service.submit_package_project(request, game_context, project_root, mod_id, sink))
        .await
        .map_err(|error| submit_failure("run.submit_package", error))?;
    Ok(SubmitRunAck { run_id })
}

#[tauri::command]
pub async fn submit_log_analysis_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitLogAnalysisRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    let service = build_service(&config, &session)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let run_id = session
        .submit(service.submit_log_analysis(request, sink))
        .await
        .map_err(|error| submit_failure("run.submit_log_analysis", error))?;
    Ok(SubmitRunAck { run_id })
}

#[tauri::command]
pub async fn submit_build_project_run(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitBuildProjectRequest,
) -> CommandResult<SubmitRunAck> {
    let session = active_session(&active)?;
    sync_requested_project(&config, &session, &request.project_root)?;
    let service = build_service(&config, &session)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let pack = session_game_pack(&session)?;
    let run_id = session
        .submit(service.submit_build_project(request, pack, sink))
        .await
        .map_err(|error| submit_failure("run.submit_build", error))?;
    Ok(SubmitRunAck { run_id })
}

fn sync_requested_project(
    config: &State<'_, AppConfig>,
    session: &ProjectSession,
    requested_root: &std::path::Path,
) -> CommandResult<()> {
    let active_root = session.path().to_path_buf();
    let game_id = session.meta().game_id.clone();
    let canonical_active = std::fs::canonicalize(&active_root).map_err(|error| {
        CommandFailure::io(
            "project.path_invalid",
            "project.resolve_active",
            "The active project path could not be resolved.",
            &error,
        )
    })?;
    let canonical_requested = std::fs::canonicalize(requested_root).map_err(|error| {
        CommandFailure::io(
            "project.path_invalid",
            "project.resolve_requested",
            "The requested project path could not be resolved.",
            &error,
        )
    })?;
    if canonical_active != canonical_requested {
        return Err(CommandFailure::invalid_input(
            "project.scope",
            "The requested project is not the active project.",
        ));
    }
    sync_project_local_props(&active_root, &game_id, &config.settings_snapshot()).map(|_| ())
}

pub(crate) fn active_game_context(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
) -> CommandResult<VerifiedGameContext> {
    let session = active_session(active)?;
    session_game_context(config, &session)
}

fn session_game_context(
    config: &State<'_, AppConfig>,
    session: &ProjectSession,
) -> CommandResult<VerifiedGameContext> {
    let game_id = session.meta().game_id.clone();
    let registry = GamePackRegistry::built_in()
        .map_err(|_| CommandFailure::unclassified("project.game_pack_registry"))?;
    VerifiedGameContext::open_current(&config.status_snapshot().runtime_dir(), &registry, &game_id)
        .map_err(|_| CommandFailure::unclassified("project.game_context"))
}

fn session_game_pack(
    session: &ProjectSession,
) -> CommandResult<ats_core::game_pack::LoadedGamePack> {
    let game_id = session.meta().game_id.clone();
    GamePackRegistry::built_in()
        .map_err(|_| CommandFailure::unclassified("project.game_pack_registry"))?
        .require(&game_id)
        .cloned()
        .map_err(|_| {
            CommandFailure::invalid_input(
                "project.game_pack",
                "The active project references an unavailable Game Pack.",
            )
        })
}

pub(crate) fn active_game_id(active: &State<'_, ActiveProject>) -> CommandResult<String> {
    Ok(active_session(active)?.meta().game_id.clone())
}

fn build_service(
    config: &State<'_, AppConfig>,
    session: &ProjectSession,
) -> CommandResult<RunApplicationService> {
    let llm = build_llm_client(config)?;
    Ok(RunApplicationService::new(session.repository(), llm))
}

fn active_session(active: &State<'_, ActiveProject>) -> CommandResult<Arc<ProjectSession>> {
    active.require().map_err(|error| match error {
        crate::project_session::ActiveProjectError::NotOpen => {
            CommandFailure::project_not_open("project.session")
        }
        crate::project_session::ActiveProjectError::Poisoned => {
            CommandFailure::unclassified("project.active_lock")
        }
    })
}

fn submit_failure(stage: &'static str, error: SubmitError) -> CommandFailure {
    match error {
        SubmitError::Closing => CommandFailure::project_closing(stage),
        SubmitError::Run(error) => CommandFailure::run(stage, &error),
    }
}

fn build_llm_client(config: &State<'_, AppConfig>) -> CommandResult<Arc<dyn LlmClient>> {
    let settings = config.settings_snapshot();
    build_from_config(&settings.llm).map_err(|error| CommandFailure::llm("llm.configure", &error))
}

struct TauriProgressSink {
    app: AppHandle,
}

impl TauriProgressSink {
    fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl ProgressSink for TauriProgressSink {
    async fn emit(&self, event: ProgressEvent) {
        if let Err(e) = self.app.emit(RUN_PROGRESS_EVENT, &event) {
            eprintln!("run-progress emit failed: {e}");
        }
    }
}
