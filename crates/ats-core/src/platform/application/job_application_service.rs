//! JobApplicationService —— Job 生命周期入口（submit / get / list / cancel）。
//!
//! 实际 handler 实现位于 `super::handlers::*` 子模块，本文件只负责持久化
//! 初始 Pending 记录 + spawn 后台 tokio 任务。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use super::handlers::{
    ProgressSink,
    asset_compile::{AssetCompileValidator, DotnetAssetCompileValidator},
    asset_generate::run_asset_generate,
    batch_custom_code::run_batch_custom_code,
    build_project::run_build_project,
    code_generate::run_code_generate,
    log_analysis::run_log_analysis,
    package_project::run_package_project,
    single_asset_plan::run_single_asset_plan,
    text_generate::run_text_generate,
    truth_snapshot_refresh::run_truth_snapshot_refresh,
};
use crate::game_pack::{
    LoadedGamePack, TruthSnapshotRefresher, TruthSnapshotStore, VerifiedGameContext,
};
use crate::image_gen::ImageGenClient;
use crate::image_proc::ImageProcClient;
use crate::llm::LlmClient;
use crate::platform::contracts::{
    SubmitAssetGenerateRequest, SubmitBatchCustomCodeRequest, SubmitBuildProjectRequest,
    SubmitCodeGenerateRequest, SubmitLogAnalysisRequest, SubmitPackageProjectRequest,
    SubmitSingleAssetPlanRequest, SubmitTextGenerateRequest, SubmitTruthSnapshotRefreshRequest,
};
use crate::platform::domain::{
    Job, JobError, JobId, JobKind, JobRepository, JobResult, JobStatus, JobSummary,
};

pub struct JobApplicationService {
    repo: Arc<dyn JobRepository>,
    llm: Option<Arc<dyn LlmClient>>,
    asset_compile_validator: Arc<dyn AssetCompileValidator>,
}

impl JobApplicationService {
    #[must_use]
    pub fn new(repo: Arc<dyn JobRepository>, llm: Arc<dyn LlmClient>) -> Self {
        Self {
            repo,
            llm: Some(llm),
            asset_compile_validator: Arc::new(DotnetAssetCompileValidator),
        }
    }

    /// Construct a service for jobs whose execution does not use an LLM.
    #[must_use]
    pub fn without_llm(repo: Arc<dyn JobRepository>) -> Self {
        Self {
            repo,
            llm: None,
            asset_compile_validator: Arc::new(DotnetAssetCompileValidator),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_asset_compile_validator(
        mut self,
        validator: Arc<dyn AssetCompileValidator>,
    ) -> Self {
        self.asset_compile_validator = validator;
        self
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
        // 先快速判断（非关键路径，此处 TOCTOU 无害）：已终态直接报错，行为同旧实现。
        let current = self.repo.get(id).await?;
        if current.status.is_terminal() {
            return Err(JobError::Terminal {
                id: current.id.0.clone(),
                status: format!("{:?}", current.status),
            });
        }
        // 关键路径：在仓库锁下原子置 Cancelled（仅当仍非终态），与 handler 收尾的
        // CAS 互斥，杜绝「取消被 stream 收尾复活成 Completed」的竞态。
        self.repo
            .modify(
                id,
                Box::new(|job| {
                    if job.status.is_terminal() {
                        false
                    } else {
                        job.status = JobStatus::Cancelled;
                        job.completed_at = Some(chrono::Utc::now());
                        true
                    }
                }),
            )
            .await?;
        Ok(())
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
        let llm = self.require_llm()?;
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_text_generate(repo, llm, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }

    /// 提交 code_generate 任务：asset 模式生成结构化 C# + 本地化 bundle 并编译验证；
    /// custom_code 模式保持单 C# fence 写入流程。
    ///
    /// `game_context` fixes the registry Pack and verified current Snapshot before
    /// the job record is created. Original model output and C# artifacts land in `artifacts/<name>/`;
    /// asset 正式文件只在 compile gate 通过后保留。
    pub async fn submit_code_generate(
        &self,
        request: SubmitCodeGenerateRequest,
        game_context: VerifiedGameContext,
        artifacts_dir: PathBuf,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = request_payload_with_context(&request, &game_context)?;
        let job = Job::new(JobKind::CodeGenerate, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let compile_validator = Arc::clone(&self.asset_compile_validator);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_code_generate(
                repo,
                llm,
                sink,
                id_for_task,
                request,
                game_context,
                artifacts_dir,
                compile_validator,
            )
            .await;
        });

        Ok(job_id)
    }

    /// Submit a Pack-driven refresh. The caller must resolve the Pack and local input
    /// bindings from the active project before the Pending job is persisted.
    pub async fn submit_truth_snapshot_refresh(
        &self,
        request: SubmitTruthSnapshotRefreshRequest,
        pack: LoadedGamePack,
        store: TruthSnapshotStore,
        local_inputs: BTreeMap<String, PathBuf>,
        refresher: TruthSnapshotRefresher,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let mut payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        payload
            .as_object_mut()
            .ok_or_else(|| JobError::Storage("refresh payload must be an object".into()))?
            .insert(
                "gamePackId".into(),
                serde_json::Value::String(pack.id.clone()),
            );
        let job = Job::new(JobKind::TruthSnapshotRefresh, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_truth_snapshot_refresh(
                repo,
                sink,
                id_for_task,
                request,
                pack,
                store,
                local_inputs,
                refresher,
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
        let llm = self.require_llm()?;
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
        let llm = self.require_llm()?;
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_log_analysis(repo, llm, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }

    /// 提交 asset_generate 任务：image_gen 出图 + 结构化 C#/本地化生成 + compile gate。
    /// image_prompt 留空时跳过 image_gen，等价于 code_generate(asset) 但通过统一接口。
    /// `image_proc` 走 BgRemoverChain（生产 ML→Simple 回退）；fallback 或质量门禁失败
    /// 会保留诊断文件并终止本次资产生成，不交付原图。
    #[allow(clippy::too_many_arguments)] // 同 handler，DI 注入式 service
    pub async fn submit_asset_generate(
        &self,
        request: SubmitAssetGenerateRequest,
        game_context: VerifiedGameContext,
        artifacts_dir: PathBuf,
        image_gen: Arc<dyn ImageGenClient>,
        image_proc: Arc<dyn ImageProcClient>,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = request_payload_with_context(&request, &game_context)?;
        let job = Job::new(JobKind::AssetGenerate, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let compile_validator = Arc::clone(&self.asset_compile_validator);
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
                game_context,
                artifacts_dir,
                compile_validator,
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
        game_context: VerifiedGameContext,
        artifacts_dir: PathBuf,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = request_payload_with_context(&request, &game_context)?;
        let job = Job::new(JobKind::BatchCustomCode, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_batch_custom_code(
                repo,
                llm,
                sink,
                id_for_task,
                request,
                game_context,
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

    fn require_llm(&self) -> JobResult<Arc<dyn LlmClient>> {
        self.llm
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| JobError::Storage("this Job service has no LLM client".into()))
    }
}

fn request_payload_with_context<T: Serialize>(
    request: &T,
    context: &VerifiedGameContext,
) -> JobResult<serde_json::Value> {
    let mut payload = serde_json::to_value(request)
        .map_err(|error| JobError::Storage(format!("serialize request: {error}")))?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| JobError::Storage("job request payload must be a JSON object".into()))?;
    object.insert(
        "_gameContext".into(),
        serde_json::to_value(context.evidence()).map_err(|error| {
            JobError::Storage(format!("serialize verified game context: {error}"))
        })?,
    );
    Ok(payload)
}
