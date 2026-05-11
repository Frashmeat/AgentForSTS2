//! knowledge_refresh handler：跑 ilspycmd 把 sts2.dll 反编译到知识库 game 目录，
//! 可选同时从 GitHub Releases 拉 BaseLib.dll 并反编译。
//!
//! 决策流：
//! 1. 解析 ilspycmd（显式 > 自动发现）
//! 2. 校验 sts2.dll 存在
//! 3. ensure_dirs + 读 manifest
//! 4. Game 分支：若 manifest 命中且非 force → 复用旧 record；否则跑
//!    run_decompile_project → 新 record
//! 5. Baselib 分支：若 include_baselib → fetch + run_decompile_file → 新 record；
//!    否则保留 manifest 中原 baselib 记录
//! 6. 写 manifest，落 Completed
//!
//! 任一分支失败即任务 Failed；baselib 失败时 game 结果不写入 manifest（避免半状态）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::common::{ProgressEvent, ProgressSink, finalize_with_error, transition_to_running};
use crate::knowledge::{
    BaselibSource, DecompileRecord, DecompileStats, KnowledgePaths, build_record,
    build_record_with_tag, default_dotnet_tools_dirs, discover_ilspycmd, ensure_dirs,
    read_manifest, run_decompile_file, run_decompile_project, write_manifest,
};
use crate::platform::contracts::SubmitKnowledgeRefreshRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};
use crate::project_utils::to_extended_length_path;

pub async fn run_knowledge_refresh(
    repo: Arc<dyn JobRepository>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitKnowledgeRefreshRequest,
    knowledge_paths: KnowledgePaths,
    baselib_source: Arc<dyn BaselibSource>,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    // 1-2. 入参校验
    let ilspycmd = match resolve_ilspycmd(&request) {
        Ok(p) => p,
        Err(msg) => {
            finalize_with_error(&repo, &job_id, &msg).await;
            return;
        }
    };
    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "ilspycmd-resolved".into(),
        percent: Some(0.02),
        message: Some(format!("ilspycmd: {}", ilspycmd.display())),
        delta: None,
    })
    .await;

    if !request.sts2_dll_path.is_file() {
        finalize_with_error(
            &repo,
            &job_id,
            &format!(
                "sts2_dll_path not found: {}",
                request.sts2_dll_path.display()
            ),
        )
        .await;
        return;
    }

    // 3. 准备目录 + 读 manifest
    if let Err(err) = ensure_dirs(&knowledge_paths) {
        finalize_with_error(&repo, &job_id, &format!("ensure_dirs: {err}")).await;
        return;
    }
    let cached_manifest = match read_manifest(&knowledge_paths.manifest_path) {
        Ok(m) => m,
        Err(err) => {
            sink.emit(ProgressEvent {
                job_id: job_id.clone(),
                stage: "manifest-read-warn".into(),
                percent: None,
                message: Some(format!("manifest read failed (will overwrite): {err}")),
                delta: None,
            })
            .await;
            None
        }
    };

    // 4. Game 分支
    let game_outcome = run_game_step(
        &sink,
        &job_id,
        &ilspycmd,
        &request,
        &knowledge_paths,
        cached_manifest.as_ref().and_then(|m| m.game.as_ref()),
    )
    .await;
    let (game_record, game_cache_hit, game_stats) = match game_outcome {
        Ok(t) => t,
        Err(msg) => {
            finalize_with_error(&repo, &job_id, &msg).await;
            return;
        }
    };

    // 5. Baselib 分支
    let baselib_outcome = if request.include_baselib {
        match run_baselib_step(
            &sink,
            &job_id,
            &ilspycmd,
            &knowledge_paths,
            baselib_source.as_ref(),
        )
        .await
        {
            Ok(record) => Some(record),
            Err(msg) => {
                finalize_with_error(&repo, &job_id, &format!("baselib: {msg}")).await;
                return;
            }
        }
    } else {
        cached_manifest.as_ref().and_then(|m| m.baselib.clone())
    };

    // 6. 写 manifest
    let mut manifest = cached_manifest.unwrap_or_default();
    manifest.game = Some(game_record.clone());
    manifest.baselib = baselib_outcome.clone();
    if let Err(err) = write_manifest(&knowledge_paths.manifest_path, &manifest) {
        finalize_with_error(&repo, &job_id, &format!("write manifest: {err}")).await;
        return;
    }

    // 收口
    let job = match repo.get(&job_id).await {
        Ok(j) => j,
        Err(_) => return,
    };
    if matches!(job.status, JobStatus::Cancelled) {
        return;
    }
    let mut job = job;
    job.status = JobStatus::Completed;
    job.completed_at = Some(chrono::Utc::now());
    job.result = Some(serde_json::json!({
        "gameCacheHit": game_cache_hit,
        "ilspycmd": ilspycmd.display().to_string(),
        "sourceDll": request.sts2_dll_path.display().to_string(),
        "gameCsFileCount": game_record.cs_file_count,
        "gameTotalBytes": game_record.total_bytes,
        "gameExitCode": game_stats.as_ref().map(|s| s.exit_code),
        "baselibIncluded": request.include_baselib,
        "baselibReleaseTag": baselib_outcome.as_ref().and_then(|r| r.release_tag.clone()),
        "baselibFile": baselib_outcome
            .as_ref()
            .map(|r| r.source_path.display().to_string()),
        "manifestPath": knowledge_paths.manifest_path.display().to_string(),
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!(
            "game {} files; baselib {}",
            game_record.cs_file_count,
            if let Some(b) = &baselib_outcome {
                format!("@{}", b.release_tag.as_deref().unwrap_or("?"))
            } else {
                "skipped".into()
            }
        )),
        delta: None,
    })
    .await;
}

/// 跑 game 分支：缓存命中走快路径；否则 ilspycmd -p。
/// 返回 (game_record, cache_hit, decompile_stats_if_ran)。
async fn run_game_step(
    sink: &Arc<dyn ProgressSink>,
    job_id: &JobId,
    ilspycmd: &Path,
    request: &SubmitKnowledgeRefreshRequest,
    knowledge_paths: &KnowledgePaths,
    cached_game: Option<&DecompileRecord>,
) -> Result<(DecompileRecord, bool, Option<DecompileStats>), String> {
    if !request.force
        && let Some(g) = cached_game
        && g.matches_current_source(&request.sts2_dll_path)
    {
        sink.emit(ProgressEvent {
            job_id: job_id.clone(),
            stage: "game-cache-hit".into(),
            percent: Some(0.4),
            message: Some(format!("game cache hit: {} files", g.cs_file_count)),
            delta: None,
        })
        .await;
        return Ok((g.clone(), true, None));
    }

    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "game-decompile".into(),
        percent: Some(0.1),
        message: Some(format!(
            "decompiling {} → {}",
            request.sts2_dll_path.display(),
            knowledge_paths.game_dir.display()
        )),
        delta: None,
    })
    .await;

    // Windows 长路径 / 中文路径保护：subprocess 调用前加 \\?\ 前缀（短路径无效果）
    let dll = to_extended_length_path(&request.sts2_dll_path);
    let out = to_extended_length_path(&knowledge_paths.game_dir);
    let cmd = ilspycmd.to_path_buf();
    let stats_result =
        tokio::task::spawn_blocking(move || run_decompile_project(&cmd, &dll, &out)).await;
    let stats = match stats_result {
        Ok(Ok(s)) => s,
        Ok(Err(err)) => return Err(format!("decompile: {err}")),
        Err(err) => return Err(format!("join blocking: {err}")),
    };

    let record = build_record(
        &request.sts2_dll_path,
        stats.cs_file_count,
        stats.total_bytes,
    )
    .map_err(|e| format!("build game record: {e}"))?;
    Ok((record, false, Some(stats)))
}

/// 跑 baselib 分支：用 BaselibSource 拉 .dll → run_decompile_file → 写 record。
/// 不做缓存命中检查（需要先打 GitHub API 才能知道 tag）。
async fn run_baselib_step(
    sink: &Arc<dyn ProgressSink>,
    job_id: &JobId,
    ilspycmd: &Path,
    knowledge_paths: &KnowledgePaths,
    source: &dyn BaselibSource,
) -> Result<DecompileRecord, String> {
    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "baselib-fetch".into(),
        percent: Some(0.6),
        message: Some("fetching latest BaseLib.dll".into()),
        delta: None,
    })
    .await;

    let cache_dir = knowledge_paths.cache_dir.join("baselib-source");
    let fetched = source
        .fetch_baselib_dll(&cache_dir)
        .await
        .map_err(|e| format!("fetch: {e}"))?;

    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "baselib-decompile".into(),
        percent: Some(0.8),
        message: Some(format!(
            "decompiling {} (release {})",
            fetched.asset_name, fetched.release_tag
        )),
        delta: None,
    })
    .await;

    let target = knowledge_paths.baselib_decompiled_file();
    let cmd = ilspycmd.to_path_buf();
    let dll = to_extended_length_path(&fetched.dll_path);
    let out = to_extended_length_path(&target);
    let stats_result =
        tokio::task::spawn_blocking(move || run_decompile_file(&cmd, &dll, &out)).await;
    let stats = match stats_result {
        Ok(Ok(s)) => s,
        Ok(Err(err)) => return Err(format!("decompile: {err}")),
        Err(err) => return Err(format!("join blocking: {err}")),
    };

    let record = build_record_with_tag(
        &target,
        stats.cs_file_count,
        stats.total_bytes,
        Some(fetched.release_tag.clone()),
    )
    .map_err(|e| format!("build baselib record: {e}"))?;
    Ok(record)
}

fn resolve_ilspycmd(request: &SubmitKnowledgeRefreshRequest) -> Result<PathBuf, String> {
    if let Some(explicit) = &request.ilspycmd_path {
        if !explicit.is_file() {
            return Err(format!(
                "ilspycmd_path does not exist: {}",
                explicit.display()
            ));
        }
        return Ok(explicit.clone());
    }
    let extras = default_dotnet_tools_dirs();
    discover_ilspycmd(&extras).ok_or_else(|| {
        "ilspycmd not found on PATH or in ~/.dotnet/tools; install via \
        `dotnet tool install -g ilspycmd` or pass an explicit path"
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{
        BaselibError, FetchedBaselib, KnowledgeManifest, build_record, write_manifest,
    };
    use crate::llm::{CompletionRequest, CompletionResponse, CompletionStream, LlmClient, LlmError};
    use crate::platform::application::JobApplicationService;
    use crate::platform::domain::JobRepository;
    use crate::platform::infra::FileJobRepository;
    use async_trait::async_trait;
    use futures_util::stream;
    use std::path::Path;
    use std::sync::Mutex;

    struct DummyLlm;

    #[async_trait]
    impl LlmClient for DummyLlm {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            unimplemented!()
        }
        async fn stream(&self, _: CompletionRequest) -> Result<CompletionStream, LlmError> {
            Ok(Box::pin(stream::iter(vec![])))
        }
    }

    /// Mock baselib source：把预设的 bytes 写到 dest_dir/<asset_name>。
    struct MockBaselibSource {
        asset_name: String,
        release_tag: String,
        bytes: Vec<u8>,
        called: Mutex<u32>,
    }

    impl MockBaselibSource {
        fn new(asset_name: &str, tag: &str, bytes: &[u8]) -> Self {
            Self {
                asset_name: asset_name.into(),
                release_tag: tag.into(),
                bytes: bytes.to_vec(),
                called: Mutex::new(0),
            }
        }
        fn call_count(&self) -> u32 {
            *self.called.lock().unwrap()
        }
    }

    #[async_trait]
    impl BaselibSource for MockBaselibSource {
        async fn fetch_baselib_dll(
            &self,
            dest_dir: &Path,
        ) -> Result<FetchedBaselib, BaselibError> {
            *self.called.lock().unwrap() += 1;
            std::fs::create_dir_all(dest_dir).map_err(|e| {
                BaselibError::Write(dest_dir.to_path_buf(), e.to_string())
            })?;
            let path = dest_dir.join(&self.asset_name);
            std::fs::write(&path, &self.bytes)
                .map_err(|e| BaselibError::Write(path.clone(), e.to_string()))?;
            Ok(FetchedBaselib {
                dll_path: path,
                release_tag: self.release_tag.clone(),
                asset_name: self.asset_name.clone(),
                source_url: format!("mock://{}", self.asset_name),
                bytes: self.bytes.len() as u64,
            })
        }
    }

    /// 永远报错的 source，用来测 baselib 分支失败回报路径。
    struct FailingBaselibSource;

    #[async_trait]
    impl BaselibSource for FailingBaselibSource {
        async fn fetch_baselib_dll(
            &self,
            _dest_dir: &Path,
        ) -> Result<FetchedBaselib, BaselibError> {
            Err(BaselibError::Http("simulated network failure".into()))
        }
    }

    /// Noop baselib source（用于 include_baselib=false 的测试）。
    struct NoopBaselib;

    #[async_trait]
    impl BaselibSource for NoopBaselib {
        async fn fetch_baselib_dll(
            &self,
            _dest_dir: &Path,
        ) -> Result<FetchedBaselib, BaselibError> {
            panic!("NoopBaselib should not be called");
        }
    }

    async fn wait_terminal(service: &JobApplicationService, id: &JobId) {
        for _ in 0..100 {
            let job = service.get(id).await.unwrap();
            if job.status.is_terminal() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    fn make_service(history: PathBuf) -> JobApplicationService {
        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        JobApplicationService::new(repo, llm)
    }

    #[tokio::test]
    async fn refresh_fails_when_dll_missing() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let runtime = td.path().join("runtime");
        let paths = KnowledgePaths::from_runtime_dir(&runtime);
        let bin = td
            .path()
            .join(if cfg!(windows) { "ilspycmd.exe" } else { "ilspycmd" });
        std::fs::write(&bin, b"fake").unwrap();

        let service = make_service(history);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let source: Arc<dyn BaselibSource> = Arc::new(NoopBaselib);
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: td.path().join("nope.dll"),
            ilspycmd_path: Some(bin),
            force: false,
            include_baselib: false,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, source, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error
                .unwrap_or_default()
                .contains("sts2_dll_path not found")
        );
    }

    #[tokio::test]
    async fn refresh_fails_when_ilspycmd_path_missing() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let runtime = td.path().join("runtime");
        let paths = KnowledgePaths::from_runtime_dir(&runtime);
        let dll = td.path().join("sts2.dll");
        std::fs::write(&dll, b"MZ--placeholder").unwrap();

        let service = make_service(history);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let source: Arc<dyn BaselibSource> = Arc::new(NoopBaselib);
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: dll,
            ilspycmd_path: Some(td.path().join("does-not-exist")),
            force: false,
            include_baselib: false,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, source, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error
                .unwrap_or_default()
                .contains("ilspycmd_path does not exist")
        );
    }

    #[tokio::test]
    async fn refresh_game_cache_hit_skips_baselib_when_not_requested() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let runtime = td.path().join("runtime");
        let paths = KnowledgePaths::from_runtime_dir(&runtime);
        ensure_dirs(&paths).unwrap();

        let dll = td.path().join("sts2.dll");
        std::fs::write(&dll, b"MZ--placeholder").unwrap();

        let manifest = KnowledgeManifest {
            game: Some(build_record(&dll, 42, 12345).unwrap()),
            ..KnowledgeManifest::default()
        };
        write_manifest(&paths.manifest_path, &manifest).unwrap();

        let bin = td
            .path()
            .join(if cfg!(windows) { "ilspycmd.exe" } else { "ilspycmd" });
        std::fs::write(&bin, b"fake").unwrap();

        let service = make_service(history);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let source: Arc<dyn BaselibSource> = Arc::new(NoopBaselib);
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: dll,
            ilspycmd_path: Some(bin),
            force: false,
            include_baselib: false,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, source, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.unwrap();
        assert_eq!(res["gameCacheHit"], true);
        assert_eq!(res["gameCsFileCount"], 42);
        assert_eq!(res["baselibIncluded"], false);
    }

    #[tokio::test]
    async fn refresh_baselib_failure_reports_baselib_prefix() {
        // game 走缓存命中早退；include_baselib=true 触发 baselib 步骤，被 mock
        // 故意失败的 source 拦下，handler 应在错误里挂 "baselib:" 前缀。
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let runtime = td.path().join("runtime");
        let paths = KnowledgePaths::from_runtime_dir(&runtime);
        ensure_dirs(&paths).unwrap();

        let dll = td.path().join("sts2.dll");
        std::fs::write(&dll, b"MZ--placeholder").unwrap();
        let manifest = KnowledgeManifest {
            game: Some(build_record(&dll, 42, 12345).unwrap()),
            ..KnowledgeManifest::default()
        };
        write_manifest(&paths.manifest_path, &manifest).unwrap();

        let bin = td
            .path()
            .join(if cfg!(windows) { "ilspycmd.exe" } else { "ilspycmd" });
        std::fs::write(&bin, b"fake").unwrap();

        let service = make_service(history);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let source: Arc<dyn BaselibSource> = Arc::new(FailingBaselibSource);
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: dll,
            ilspycmd_path: Some(bin),
            force: false,
            include_baselib: true,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, source, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        let err = job.error.unwrap_or_default();
        assert!(err.starts_with("baselib:"), "expected baselib: prefix in: {err}");
        assert!(err.contains("simulated network failure"));
    }

    #[tokio::test]
    async fn refresh_baselib_decompile_fails_when_ilspycmd_invalid() {
        // game cache hit + 真实 mock baselib（写入文件成功）+ 假 ilspycmd → baselib
        // decompile 阶段必然失败。验证 mock source 被调用一次。
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let runtime = td.path().join("runtime");
        let paths = KnowledgePaths::from_runtime_dir(&runtime);
        ensure_dirs(&paths).unwrap();

        let dll = td.path().join("sts2.dll");
        std::fs::write(&dll, b"MZ--placeholder").unwrap();
        let manifest = KnowledgeManifest {
            game: Some(build_record(&dll, 42, 12345).unwrap()),
            ..KnowledgeManifest::default()
        };
        write_manifest(&paths.manifest_path, &manifest).unwrap();

        // 假 ilspycmd（不可执行的文本文件）—— 文件存在让 resolve 通过，但 spawn 会失败
        let bin = td
            .path()
            .join(if cfg!(windows) { "ilspycmd.exe" } else { "ilspycmd" });
        std::fs::write(&bin, b"#!/bin/sh\nexit 0").unwrap();

        let mock = Arc::new(MockBaselibSource::new(
            "BaseLib.dll",
            "v0.5.2",
            b"FAKE-PE-CONTENT",
        ));

        let service = make_service(history);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let source: Arc<dyn BaselibSource> = mock.clone();
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: dll,
            ilspycmd_path: Some(bin),
            force: false,
            include_baselib: true,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, source, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert_eq!(mock.call_count(), 1, "baselib source should have been called");
        let err = job.error.unwrap_or_default();
        assert!(err.contains("baselib"), "expected baselib error, got: {err}");
        assert!(err.contains("decompile"), "expected decompile error stage: {err}");
    }
}
