//! Platform commands —— Job 生命周期 + text/code/build 三类 handler 提交。
//!
//! Repository 用 ActiveProject 的 history_dir 作存储路径；切换项目时下条提交
//! 会落到新工程。LLM client 每次新建（Arc 包裹的开销极小，避免与配置变更竞态）。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ats_core::audit::{AuditSinkArc, FileAuditSink};
use ats_core::game_pack::{GamePackRegistry, VerifiedGameContext};
use ats_core::image_gen::{ImageGenClient, build_from_config as build_image_gen};
use ats_core::image_proc::{BgRemoverChain, ImageProcClient};
use ats_core::knowledge::{BaselibSource, GitHubBaselibSource, KnowledgePaths};
use ats_core::llm::{LlmClient, build_from_config};
use ats_core::platform::{
    AuditedJobRepository, FileJobRepository, Job, JobApplicationService, JobError, JobId,
    JobRepository, JobSummary, ProgressEvent, ProgressSink, SubmitAssetGenerateRequest,
    SubmitBatchCustomCodeRequest, SubmitBuildProjectRequest, SubmitCodeGenerateRequest,
    SubmitJobAck, SubmitKnowledgeRefreshRequest, SubmitLogAnalysisRequest,
    SubmitPackageProjectRequest, SubmitSingleAssetPlanRequest, SubmitTextGenerateRequest,
};
use tauri::{AppHandle, Emitter, State};

use crate::AppConfig;
use crate::commands::project::{ActiveProject, sync_project_local_props};

const JOB_PROGRESS_EVENT: &str = "job-progress";

#[tauri::command]
pub async fn submit_text_generate_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitTextGenerateRequest,
) -> Result<SubmitJobAck, String> {
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let job_id = service
        .submit_text_generate(request, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

#[tauri::command]
pub async fn get_job(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    id: String,
) -> Result<Job, String> {
    let repositories = job_repositories(&config, &active)?;
    let (_, job) = find_job_repository(&repositories, &JobId(id)).await?;
    Ok(job)
}

#[tauri::command]
pub async fn list_jobs(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
) -> Result<Vec<JobSummary>, String> {
    let repositories = job_repositories(&config, &active)?;
    let mut jobs = Vec::new();
    for repository in repositories {
        jobs.extend(repository.list().await.map_err(|e| e.to_string())?);
    }
    jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    jobs.dedup_by(|a, b| a.id == b.id);
    Ok(jobs)
}

#[tauri::command]
pub async fn cancel_job(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    id: String,
) -> Result<(), String> {
    let repositories = job_repositories(&config, &active)?;
    let (repository, _) = find_job_repository(&repositories, &JobId(id.clone())).await?;
    let service = JobApplicationService::new(repository, build_llm_client(&config)?);
    service.cancel(&JobId(id)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn submit_code_generate_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitCodeGenerateRequest,
) -> Result<SubmitJobAck, String> {
    if let SubmitCodeGenerateRequest::Asset { request: asset } = &request {
        sync_requested_project(&config, &active, &asset.project_root)?;
    }
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let artifacts_dir = active_artifacts_dir(&active)?;
    let game_context = active_game_context(&config, &active)?;
    let job_id = service
        .submit_code_generate(request, game_context, artifacts_dir, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

#[tauri::command]
pub async fn submit_asset_generate_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    image_proc_state: State<'_, Arc<crate::commands::image_proc_state::ImageProcState>>,
    request: SubmitAssetGenerateRequest,
) -> Result<SubmitJobAck, String> {
    sync_requested_project(&config, &active, &request.asset_request.project_root)?;
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let artifacts_dir = active_artifacts_dir(&active)?;
    let game_context = active_game_context(&config, &active)?;
    let settings = config.settings_snapshot();
    let image_gen: Arc<dyn ImageGenClient> =
        build_image_gen(&settings.image_gen).map_err(|e| e.to_string())?;
    // BgRemoverChain：prewarm 阶段装好的 ML primary（feature on 且加载成功），
    // 没装则只用启发式 fallback。
    let primary = image_proc_state.primary();
    let image_proc: Arc<dyn ImageProcClient> =
        Arc::new(BgRemoverChain::with_simple_fallback(primary));
    let job_id = service
        .submit_asset_generate(
            request,
            game_context,
            artifacts_dir,
            image_gen,
            image_proc,
            sink,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

#[tauri::command]
pub async fn submit_knowledge_refresh_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    request: SubmitKnowledgeRefreshRequest,
) -> Result<SubmitJobAck, String> {
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let knowledge_paths = KnowledgePaths::from_runtime_dir(&config.status_snapshot().runtime_dir());
    let github_baselib = GitHubBaselibSource::default_alchyr_with_default_client(
        Some(
            config
                .settings_snapshot()
                .runtime
                .workstation
                .github_token
                .clone(),
        )
        .filter(|t| !t.is_empty()),
    )
    .map_err(|e| format!("init baselib source: {e}"))?;
    #[cfg(feature = "e2e")]
    let github_baselib = match std::env::var("ATS_E2E_BASELIB_RELEASE_URL") {
        Ok(url) if !url.is_empty() => github_baselib.with_latest_release_url_override(url),
        _ => github_baselib,
    };
    let baselib_source: Arc<dyn BaselibSource> = Arc::new(github_baselib);
    let sts2_dll_path = PathBuf::from(&config.settings_snapshot().knowledge.sts2_dll_path);
    if sts2_dll_path.as_os_str().is_empty() {
        return Err(
            "knowledge.sts2_dll_path is not set — configure in System > 运维 > Knowledge".into(),
        );
    }
    let mut request = request;
    request.sts2_dll_path = sts2_dll_path;
    let llm = build_llm_client(&config)?;
    let history_dir = config
        .status_snapshot()
        .runtime_dir()
        .join("knowledge")
        .join("jobs");
    std::fs::create_dir_all(&history_dir).map_err(|e| format!("create knowledge jobs dir: {e}"))?;
    let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history_dir));
    let service = JobApplicationService::new(repo, llm);
    let job_id = service
        .submit_knowledge_refresh(request, knowledge_paths, baselib_source, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

#[tauri::command]
pub async fn submit_single_asset_plan_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitSingleAssetPlanRequest,
) -> Result<SubmitJobAck, String> {
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let items_dir = active_items_dir(&active).ok();
    let job_id = service
        .submit_single_asset_plan(request, items_dir, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

#[tauri::command]
pub async fn submit_batch_custom_code_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitBatchCustomCodeRequest,
) -> Result<SubmitJobAck, String> {
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let artifacts_dir = active_artifacts_dir(&active)?;
    let game_context = active_game_context(&config, &active)?;
    let job_id = service
        .submit_batch_custom_code(request, game_context, artifacts_dir, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

#[tauri::command]
pub async fn submit_package_project_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitPackageProjectRequest,
) -> Result<SubmitJobAck, String> {
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let job_id = service
        .submit_package_project(request, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

#[tauri::command]
pub async fn submit_log_analysis_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitLogAnalysisRequest,
) -> Result<SubmitJobAck, String> {
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let job_id = service
        .submit_log_analysis(request, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

#[tauri::command]
pub async fn submit_build_project_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitBuildProjectRequest,
) -> Result<SubmitJobAck, String> {
    sync_requested_project(&config, &active, &request.project_root)?;
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let job_id = service
        .submit_build_project(request, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
}

fn sync_requested_project(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
    requested_root: &std::path::Path,
) -> Result<(), String> {
    let active_root = {
        let guard = active
            .0
            .lock()
            .map_err(|e| format!("active project lock poisoned: {e}"))?;
        guard
            .as_ref()
            .ok_or_else(|| "no active project — open or create one first".to_string())?
            .path()
            .to_path_buf()
    };
    let canonical_active = std::fs::canonicalize(&active_root)
        .map_err(|e| format!("resolve active project {}: {e}", active_root.display()))?;
    let canonical_requested = std::fs::canonicalize(requested_root).map_err(|e| {
        format!(
            "resolve requested project {}: {e}",
            requested_root.display()
        )
    })?;
    if canonical_active != canonical_requested {
        return Err(format!(
            "requested project {} is not the active project {}",
            requested_root.display(),
            active_root.display()
        ));
    }
    sync_project_local_props(&active_root, &config.settings_snapshot()).map(|_| ())
}

fn active_artifacts_dir(active: &State<'_, ActiveProject>) -> Result<PathBuf, String> {
    let guard = active
        .0
        .lock()
        .map_err(|e| format!("active project lock poisoned: {e}"))?;
    let project = guard
        .as_ref()
        .ok_or_else(|| "no active project — open or create one first".to_string())?;
    Ok(project.artifacts_dir())
}

fn active_items_dir(active: &State<'_, ActiveProject>) -> Result<PathBuf, String> {
    let guard = active
        .0
        .lock()
        .map_err(|e| format!("active project lock poisoned: {e}"))?;
    let project = guard
        .as_ref()
        .ok_or_else(|| "no active project — open or create one first".to_string())?;
    Ok(project.items_dir())
}

pub(crate) fn active_game_context(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
) -> Result<VerifiedGameContext, String> {
    let game_id = {
        let guard = active
            .0
            .lock()
            .map_err(|e| format!("active project lock poisoned: {e}"))?;
        guard
            .as_ref()
            .ok_or_else(|| "no active project — open or create one first".to_string())?
            .meta()
            .game_id
            .clone()
    };
    let registry = GamePackRegistry::built_in().map_err(|error| error.to_string())?;
    VerifiedGameContext::open_current(&config.status_snapshot().runtime_dir(), &registry, &game_id)
        .map_err(|error| error.to_string())
}

fn build_service(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
) -> Result<JobApplicationService, String> {
    let (history_dir, project_root) = {
        let guard = active
            .0
            .lock()
            .map_err(|e| format!("active project lock poisoned: {e}"))?;
        let project = guard
            .as_ref()
            .ok_or_else(|| "no active project — open or create one first".to_string())?;
        (project.history_dir(), project.path().to_path_buf())
    };
    let base_repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history_dir));
    // 自动写 audit.log 到 <project>/.ats/audit.log。Sink 内部 spawn_blocking +
    // eprintln 兜底，写失败不会影响业务路径。
    let audit_sink: AuditSinkArc = Arc::new(FileAuditSink::new(project_root));
    let repo: Arc<dyn JobRepository> = Arc::new(AuditedJobRepository::new(base_repo, audit_sink));
    let llm = build_llm_client(config)?;
    Ok(JobApplicationService::new(repo, llm))
}

fn job_repositories(
    config: &State<'_, AppConfig>,
    active: &State<'_, ActiveProject>,
) -> Result<Vec<Arc<dyn JobRepository>>, String> {
    let mut repositories: Vec<Arc<dyn JobRepository>> = Vec::with_capacity(2);
    if let Some((history_dir, project_root)) = {
        let guard = active
            .0
            .lock()
            .map_err(|e| format!("active project lock poisoned: {e}"))?;
        guard
            .as_ref()
            .map(|project| (project.history_dir(), project.path().to_path_buf()))
    } {
        let base_repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history_dir));
        let audit_sink: AuditSinkArc = Arc::new(FileAuditSink::new(project_root));
        repositories.push(Arc::new(AuditedJobRepository::new(base_repo, audit_sink)));
    }
    repositories.push(knowledge_job_repository(config));
    Ok(repositories)
}

fn knowledge_job_repository(config: &State<'_, AppConfig>) -> Arc<dyn JobRepository> {
    Arc::new(FileJobRepository::new(
        config
            .status_snapshot()
            .runtime_dir()
            .join("knowledge")
            .join("jobs"),
    ))
}

async fn find_job_repository(
    repositories: &[Arc<dyn JobRepository>],
    id: &JobId,
) -> Result<(Arc<dyn JobRepository>, Job), String> {
    for repository in repositories {
        match repository.get(id).await {
            Ok(job) => return Ok((Arc::clone(repository), job)),
            Err(JobError::NotFound(_)) => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err(JobError::NotFound(id.0.clone()).to_string())
}

fn build_llm_client(config: &State<'_, AppConfig>) -> Result<Arc<dyn LlmClient>, String> {
    let settings = config.settings_snapshot();
    build_from_config(&settings.llm).map_err(|e| e.to_string())
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
        if let Err(e) = self.app.emit(JOB_PROGRESS_EVENT, &event) {
            eprintln!("job-progress emit failed: {e}");
        }
    }
}
