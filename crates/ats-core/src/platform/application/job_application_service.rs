//! JobApplicationService —— Job 生命周期入口（submit / get / list / cancel）。
//!
//! 实际 handler 实现位于 `super::handlers::*` 子模块，本文件只负责持久化
//! 初始 Pending 记录 + spawn 后台 tokio 任务。

use std::path::PathBuf;
use std::sync::Arc;

use super::handlers::{
    ProgressSink, asset_generate::run_asset_generate, batch_custom_code::run_batch_custom_code,
    build_project::run_build_project, code_generate::run_code_generate,
    knowledge_refresh::run_knowledge_refresh, log_analysis::run_log_analysis,
    package_project::run_package_project, single_asset_plan::run_single_asset_plan,
    text_generate::run_text_generate,
};
use crate::image_gen::ImageGenClient;
use crate::image_proc::ImageProcClient;
use crate::knowledge::{BaselibSource, KnowledgePaths};
use crate::llm::LlmClient;
use crate::platform::contracts::{
    SubmitAssetGenerateRequest, SubmitBatchCustomCodeRequest, SubmitBuildProjectRequest,
    SubmitCodeGenerateRequest, SubmitKnowledgeRefreshRequest, SubmitLogAnalysisRequest,
    SubmitPackageProjectRequest, SubmitSingleAssetPlanRequest, SubmitTextGenerateRequest,
};
use crate::platform::domain::{
    Job, JobError, JobId, JobKind, JobRepository, JobResult, JobStatus, JobSummary,
};

pub struct JobApplicationService {
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
}

impl JobApplicationService {
    #[must_use]
    pub fn new(repo: Arc<dyn JobRepository>, llm: Arc<dyn LlmClient>) -> Self {
        Self { repo, llm }
    }

    pub async fn get(&self, id: &JobId) -> JobResult<Job> {
        self.repo.get(id).await
    }

    pub async fn list(&self) -> JobResult<Vec<JobSummary>> {
        self.repo.list().await
    }

    /// 标记 Cancelled 并保存。任务实际执行中如已发起 LLM 请求，本 stage 不
    /// 中断网络层；handler 结束时会发现 status=Cancelled 而跳过最终结果覆写。
    pub async fn cancel(&self, id: &JobId) -> JobResult<()> {
        let mut job = self.repo.get(id).await?;
        if job.status.is_terminal() {
            return Err(JobError::Terminal {
                id: job.id.0.clone(),
                status: format!("{:?}", job.status),
            });
        }
        job.status = JobStatus::Cancelled;
        job.completed_at = Some(chrono::Utc::now());
        self.repo.update(&job).await
    }

    /// 提交 text_generate 任务：保存 Pending → spawn 后台 tokio 任务跑 LLM。
    /// 立刻返回 JobId；调用方通过 `get` / `list` / progress 事件观察进度。
    pub async fn submit_text_generate(
        &self,
        request: SubmitTextGenerateRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::TextGenerate, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_text_generate(repo, llm, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }

    /// 提交 code_generate 任务：装 prompt → LLM stream → 解 fence → 写文件。
    ///
    /// `knowledge_paths` 来自 ConfigStatus::runtime_dir()（app data 共享），
    /// 与 active project 解耦。生成的 `.cs` 落到工程的
    /// `artifacts/<name>/<name>.cs`，原始 markdown 同步存到 `raw.md` 便于排错。
    pub async fn submit_code_generate(
        &self,
        request: SubmitCodeGenerateRequest,
        knowledge_paths: KnowledgePaths,
        artifacts_dir: PathBuf,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::CodeGenerate, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_code_generate(
                repo,
                llm,
                sink,
                id_for_task,
                request,
                knowledge_paths,
                artifacts_dir,
            )
            .await;
        });

        Ok(job_id)
    }

    /// 提交 knowledge_refresh 任务：跑 ilspycmd 把 sts2.dll 反编译到 game 目录，
    /// 可选同时拉 BaseLib.dll 并反编译到 baselib/BaseLib.decompiled.cs。
    /// `force=false` 时若 manifest 与当前 dll 元数据一致则跳过 game 子进程。
    /// baselib 不做缓存命中检查（每次 include_baselib=true 都会拉 + 反编译）。
    pub async fn submit_knowledge_refresh(
        &self,
        request: SubmitKnowledgeRefreshRequest,
        knowledge_paths: KnowledgePaths,
        baselib_source: Arc<dyn BaselibSource>,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::KnowledgeRefresh, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_knowledge_refresh(
                repo,
                sink,
                id_for_task,
                request,
                knowledge_paths,
                baselib_source,
            )
            .await;
        });

        Ok(job_id)
    }

    /// 提交 single_asset_plan 任务：自然语言需求 → LLM 出 JSON → 解析成 PlanItem。
    /// 结果落到 job.result.item；当 `items_dir` 提供时，还会把 PlanItem 序列化到
    /// `<items_dir>/<item_id>.json`，让用户在工程目录里看到 plan 的产物。
    pub async fn submit_single_asset_plan(
        &self,
        request: SubmitSingleAssetPlanRequest,
        items_dir: Option<PathBuf>,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::SingleAssetPlan, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_single_asset_plan(repo, llm, sink, id_for_task, request, items_dir).await;
        });

        Ok(job_id)
    }

    /// 提交 log_analysis 任务：读 build log → LLM 出诊断 markdown。
    /// 结果直接落到 job.result.report，不写工程目录。
    pub async fn submit_log_analysis(
        &self,
        request: SubmitLogAnalysisRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::LogAnalysis, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_log_analysis(repo, llm, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }

    /// 提交 asset_generate 任务：image_gen 出图 + code_generate 出 .cs。
    /// image_prompt 留空时跳过 image_gen，等价于 code_generate(asset) 但通过统一接口。
    /// `image_proc` 走 BgRemoverChain（生产 ML→Simple 回退），失败不致命。
    #[allow(clippy::too_many_arguments)] // 同 handler，DI 注入式 service
    pub async fn submit_asset_generate(
        &self,
        request: SubmitAssetGenerateRequest,
        knowledge_paths: KnowledgePaths,
        artifacts_dir: PathBuf,
        image_gen: Arc<dyn ImageGenClient>,
        image_proc: Arc<dyn ImageProcClient>,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::AssetGenerate, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_asset_generate(
                repo,
                llm,
                image_gen,
                image_proc,
                sink,
                id_for_task,
                request,
                knowledge_paths,
                artifacts_dir,
            )
            .await;
        });

        Ok(job_id)
    }

    /// 提交 batch_custom_code 任务：N 个 CustomCodegenRequest 顺序处理。
    /// 单 item 失败默认继续；request.fail_fast = true 时首失立停。
    pub async fn submit_batch_custom_code(
        &self,
        request: SubmitBatchCustomCodeRequest,
        knowledge_paths: KnowledgePaths,
        artifacts_dir: PathBuf,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::BatchCustomCode, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_batch_custom_code(
                repo,
                llm,
                sink,
                id_for_task,
                request,
                knowledge_paths,
                artifacts_dir,
            )
            .await;
        });

        Ok(job_id)
    }

    /// 提交 package_project 任务：把 source_dir 整个 zip 到 output_path。
    /// 同步 IO 操作通过 spawn_blocking 包住，不阻塞 tokio 调度线程。
    pub async fn submit_package_project(
        &self,
        request: SubmitPackageProjectRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::PackageProject, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_package_project(repo, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }

    /// 提交 build_project 任务：在 `request.project_root` 下跑 `dotnet publish`，
    /// 捕获 stdout/stderr，按 exit_code + "X Error(s)" 启发判断成功。
    pub async fn submit_build_project(
        &self,
        request: SubmitBuildProjectRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::BuildProject, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_build_project(repo, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }
}
