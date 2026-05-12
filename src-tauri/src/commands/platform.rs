//! Platform commands —— Job 生命周期 + text/code/build 三类 handler 提交。
//!
//! Repository 用 ActiveProject 的 history_dir 作存储路径；切换项目时下条提交
//! 会落到新工程。LLM client 每次新建（Arc 包裹的开销极小，避免与配置变更竞态）。

use std::path::PathBuf;
use std::sync::Arc;

use ats_core::audit::{AuditSinkArc, FileAuditSink};
use ats_core::image_gen::{ImageGenClient, build_from_config as build_image_gen};
use ats_core::image_proc::{BgRemoverChain, ImageProcClient};
use ats_core::knowledge::{BaselibSource, GitHubBaselibSource, KnowledgePaths};
use ats_core::llm::{LlmClient, build_from_config};
use ats_core::platform::{
    AuditedJobRepository, FileJobRepository, Job, JobApplicationService, JobId, JobRepository,
    JobSummary, ProgressEvent, ProgressSink, SubmitAssetGenerateRequest,
    SubmitBatchCustomCodeRequest, SubmitBuildProjectRequest, SubmitCodeGenerateRequest,
    SubmitJobAck, SubmitKnowledgeRefreshRequest, SubmitLogAnalysisRequest,
    SubmitPackageProjectRequest, SubmitSingleAssetPlanRequest, SubmitTextGenerateRequest,
};
use async_trait::async_trait;
use tauri::{AppHandle, Emitter, State};

use crate::commands::project::ActiveProject;
use crate::AppConfig;

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
    let service = build_service(&config, &active)?;
    service
        .get(&JobId(id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_jobs(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
) -> Result<Vec<JobSummary>, String> {
    let service = build_service(&config, &active)?;
    service.list().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn cancel_job(
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    id: String,
) -> Result<(), String> {
    let service = build_service(&config, &active)?;
    service
        .cancel(&JobId(id))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn submit_code_generate_job(
    app: AppHandle,
    config: State<'_, AppConfig>,
    active: State<'_, ActiveProject>,
    request: SubmitCodeGenerateRequest,
) -> Result<SubmitJobAck, String> {
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let artifacts_dir = active_artifacts_dir(&active)?;
    let knowledge_paths = KnowledgePaths::from_runtime_dir(&config.status_snapshot().runtime_dir());
    let job_id = service
        .submit_code_generate(request, knowledge_paths, artifacts_dir, sink)
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
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let artifacts_dir = active_artifacts_dir(&active)?;
    let knowledge_paths = KnowledgePaths::from_runtime_dir(&config.status_snapshot().runtime_dir());
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
            knowledge_paths,
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
    active: State<'_, ActiveProject>,
    request: SubmitKnowledgeRefreshRequest,
) -> Result<SubmitJobAck, String> {
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let knowledge_paths = KnowledgePaths::from_runtime_dir(&config.status_snapshot().runtime_dir());
    let baselib_source: Arc<dyn BaselibSource> = Arc::new(
        GitHubBaselibSource::default_alchyr_with_default_client()
            .map_err(|e| format!("init baselib source: {e}"))?,
    );
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
    let knowledge_paths = KnowledgePaths::from_runtime_dir(&config.status_snapshot().runtime_dir());
    let job_id = service
        .submit_batch_custom_code(request, knowledge_paths, artifacts_dir, sink)
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
    let service = build_service(&config, &active)?;
    let sink: Arc<dyn ProgressSink> = Arc::new(TauriProgressSink::new(app));
    let job_id = service
        .submit_build_project(request, sink)
        .await
        .map_err(|e| e.to_string())?;
    Ok(SubmitJobAck { job_id })
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
