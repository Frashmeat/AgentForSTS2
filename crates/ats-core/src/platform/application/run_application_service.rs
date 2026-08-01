//! RunApplicationService —— RunRecord 生命周期入口（submit / get / list / cancel）。
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
use super::{CancellationToken, SpawnedRun};
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
    RunError, RunId, RunKind, RunRecord, RunRepository, RunRepositoryResult, RunSummary,
};

pub struct RunApplicationService {
    repo: Arc<dyn RunRepository>,
    llm: Option<Arc<dyn LlmClient>>,
    asset_compile_validator: Arc<dyn AssetCompileValidator>,
}

impl RunApplicationService {
    #[must_use]
    pub fn new(repo: Arc<dyn RunRepository>, llm: Arc<dyn LlmClient>) -> Self {
        Self {
            repo,
            llm: Some(llm),
            asset_compile_validator: Arc::new(DotnetAssetCompileValidator),
        }
    }

    /// Construct a service for runs whose execution does not use an LLM.
    #[must_use]
    pub fn without_llm(repo: Arc<dyn RunRepository>) -> Self {
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

    pub async fn get(&self, id: &RunId) -> RunRepositoryResult<RunRecord> {
        self.repo.get(id).await
    }

    pub async fn list(&self) -> RunRepositoryResult<Vec<RunSummary>> {
        self.repo.list().await
    }

    /// 提交 text_generate 任务：保存 Pending → spawn 后台 tokio 任务跑 LLM。
    /// 立刻返回 RunId；调用方通过 `get` / `list` / progress 事件观察进度。
    pub async fn submit_text_generate(
        &self,
        request: SubmitTextGenerateRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> RunRepositoryResult<SpawnedRun> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| RunError::Storage(format!("serialize request: {e}")))?;
        let run = RunRecord::new(RunKind::TextGenerate, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_text_generate(repo, llm, sink, id_for_task, request, task_cancellation).await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
    }

    /// 提交 code_generate 任务：asset 模式生成结构化 C# + 本地化 bundle 并编译验证；
    /// custom_code 模式保持单 C# fence 写入流程。
    ///
    /// `game_context` fixes the registry Pack and verified current Snapshot before
    /// the run record is created. Original model output and C# artifacts land in `artifacts/<name>/`;
    /// asset 正式文件只在 compile gate 通过后保留。
    pub async fn submit_code_generate(
        &self,
        request: SubmitCodeGenerateRequest,
        game_context: VerifiedGameContext,
        artifacts_dir: PathBuf,
        sink: Arc<dyn ProgressSink>,
    ) -> RunRepositoryResult<SpawnedRun> {
        let payload = request_payload_with_context(&request, &game_context)?;
        let run = RunRecord::new(RunKind::CodeGenerate, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let compile_validator = Arc::clone(&self.asset_compile_validator);
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_code_generate(
                repo,
                llm,
                sink,
                id_for_task,
                request,
                game_context,
                artifacts_dir,
                compile_validator,
                task_cancellation,
            )
            .await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
    }

    /// Submit a Pack-driven refresh. The caller must resolve the Pack and local input
    /// bindings from the active project before the Pending run is persisted.
    pub async fn submit_truth_snapshot_refresh(
        &self,
        request: SubmitTruthSnapshotRefreshRequest,
        pack: LoadedGamePack,
        store: TruthSnapshotStore,
        local_inputs: BTreeMap<String, PathBuf>,
        refresher: TruthSnapshotRefresher,
        sink: Arc<dyn ProgressSink>,
    ) -> RunRepositoryResult<SpawnedRun> {
        let mut payload = serde_json::to_value(&request)
            .map_err(|e| RunError::Storage(format!("serialize request: {e}")))?;
        payload
            .as_object_mut()
            .ok_or_else(|| RunError::Storage("refresh payload must be an object".into()))?
            .insert(
                "gamePackId".into(),
                serde_json::Value::String(pack.id.clone()),
            );
        let run = RunRecord::new(RunKind::TruthSnapshotRefresh, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_truth_snapshot_refresh(
                repo,
                sink,
                id_for_task,
                request,
                pack,
                store,
                local_inputs,
                refresher,
                task_cancellation,
            )
            .await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
    }

    /// 提交 single_asset_plan 任务：自然语言需求 → LLM 出 JSON → 解析成 PlanItem。
    /// 结果落到 run.result.item；当 `items_dir` 提供时，还会把 PlanItem 序列化到
    /// `<items_dir>/<item_id>.json`，让用户在工程目录里看到 plan 的产物。
    pub async fn submit_single_asset_plan(
        &self,
        request: SubmitSingleAssetPlanRequest,
        pack: LoadedGamePack,
        items_dir: Option<PathBuf>,
        sink: Arc<dyn ProgressSink>,
    ) -> RunRepositoryResult<SpawnedRun> {
        let mut payload = serde_json::to_value(&request)
            .map_err(|e| RunError::Storage(format!("serialize request: {e}")))?;
        payload
            .as_object_mut()
            .ok_or_else(|| RunError::Storage("single asset plan payload must be an object".into()))?
            .insert(
                "_gamePack".into(),
                serde_json::json!({
                    "id": pack.id.clone(),
                    "schemaVersion": pack.schema_version,
                    "sha256": pack.content_sha256.clone(),
                }),
            );
        let run = RunRecord::new(RunKind::SingleAssetPlan, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_single_asset_plan(
                repo,
                llm,
                sink,
                id_for_task,
                request,
                pack,
                items_dir,
                task_cancellation,
            )
            .await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
    }

    /// 提交 log_analysis 任务：读 build log → LLM 出诊断 markdown。
    /// 结果直接落到 run.result.report，不写工程目录。
    pub async fn submit_log_analysis(
        &self,
        request: SubmitLogAnalysisRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> RunRepositoryResult<SpawnedRun> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| RunError::Storage(format!("serialize request: {e}")))?;
        let run = RunRecord::new(RunKind::LogAnalysis, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_log_analysis(repo, llm, sink, id_for_task, request, task_cancellation).await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
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
    ) -> RunRepositoryResult<SpawnedRun> {
        let payload = request_payload_with_context(&request, &game_context)?;
        let run = RunRecord::new(RunKind::AssetGenerate, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let compile_validator = Arc::clone(&self.asset_compile_validator);
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
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
                task_cancellation,
            )
            .await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
    }

    /// 提交 batch_custom_code 任务：N 个 CustomCodegenRequest 顺序处理。
    /// 单 item 失败默认继续；request.fail_fast = true 时首失立停。
    pub async fn submit_batch_custom_code(
        &self,
        request: SubmitBatchCustomCodeRequest,
        game_context: VerifiedGameContext,
        artifacts_dir: PathBuf,
        sink: Arc<dyn ProgressSink>,
    ) -> RunRepositoryResult<SpawnedRun> {
        let payload = request_payload_with_context(&request, &game_context)?;
        let run = RunRecord::new(RunKind::BatchCustomCode, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let llm = self.require_llm()?;
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_batch_custom_code(
                repo,
                llm,
                sink,
                id_for_task,
                request,
                game_context,
                artifacts_dir,
                task_cancellation,
            )
            .await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
    }

    /// 提交 package_project 任务：把 source_dir 整个 zip 到 output_path。
    /// 同步 IO 操作通过 spawn_blocking 包住，不阻塞 tokio 调度线程。
    pub async fn submit_package_project(
        &self,
        request: SubmitPackageProjectRequest,
        game_context: VerifiedGameContext,
        project_root: PathBuf,
        mod_id: String,
        sink: Arc<dyn ProgressSink>,
    ) -> RunRepositoryResult<SpawnedRun> {
        let layout = game_context.pack().package_layout.clone().ok_or_else(|| {
            RunError::Storage(format!(
                "game pack `{}` has no package layout",
                game_context.game_pack_id()
            ))
        })?;
        let payload = request_payload_with_context(&request, &game_context)?;
        let run = RunRecord::new(RunKind::PackageProject, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_package_project(
                repo,
                sink,
                id_for_task,
                request,
                layout,
                mod_id,
                game_context,
                project_root,
                task_cancellation,
            )
            .await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
    }

    /// 提交 build_project 任务：在 `request.project_root` 下跑 `dotnet publish`，
    /// 捕获 stdout/stderr，按 exit_code + "X Error(s)" 启发判断成功。
    pub async fn submit_build_project(
        &self,
        request: SubmitBuildProjectRequest,
        pack: LoadedGamePack,
        sink: Arc<dyn ProgressSink>,
    ) -> RunRepositoryResult<SpawnedRun> {
        let recipe = pack.build_recipe.clone().ok_or_else(|| {
            RunError::Storage(format!("game pack `{}` has no build recipe", pack.id))
        })?;
        let payload = request_payload_with_pack(&request, &pack)?;
        let run = RunRecord::new(RunKind::BuildProject, payload);
        let run_id = run.id.clone();
        self.repo.create(&run).await?;

        let repo = Arc::clone(&self.repo);
        let id_for_task = run_id.clone();
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            run_build_project(repo, sink, id_for_task, request, recipe, task_cancellation).await;
        });

        Ok(SpawnedRun {
            run_id,
            cancellation,
            task,
        })
    }

    fn require_llm(&self) -> RunRepositoryResult<Arc<dyn LlmClient>> {
        self.llm
            .as_ref()
            .map(Arc::clone)
            .ok_or_else(|| RunError::Storage("this run service has no LLM client".into()))
    }
}

fn request_payload_with_context<T: Serialize>(
    request: &T,
    context: &VerifiedGameContext,
) -> RunRepositoryResult<serde_json::Value> {
    let mut payload = serde_json::to_value(request)
        .map_err(|error| RunError::Storage(format!("serialize request: {error}")))?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| RunError::Storage("run request payload must be a JSON object".into()))?;
    object.insert(
        "_gameContext".into(),
        serde_json::to_value(context.evidence()).map_err(|error| {
            RunError::Storage(format!("serialize verified game context: {error}"))
        })?,
    );
    Ok(payload)
}

fn request_payload_with_pack<T: Serialize>(
    request: &T,
    pack: &LoadedGamePack,
) -> RunRepositoryResult<serde_json::Value> {
    let mut payload = serde_json::to_value(request)
        .map_err(|error| RunError::Storage(format!("serialize request: {error}")))?;
    let object = payload
        .as_object_mut()
        .ok_or_else(|| RunError::Storage("run request payload must be a JSON object".into()))?;
    object.insert(
        "_gamePack".into(),
        serde_json::json!({
            "id": pack.id,
            "schemaVersion": pack.schema_version,
            "sha256": pack.content_sha256,
        }),
    );
    Ok(payload)
}
