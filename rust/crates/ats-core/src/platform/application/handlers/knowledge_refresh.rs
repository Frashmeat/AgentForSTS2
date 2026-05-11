//! knowledge_refresh handler：跑 ilspycmd 把 sts2.dll 反编译到知识库 game 目录。
//!
//! 决策流：
//! 1. 解析 ilspycmd 路径：显式 > 自动发现（PATH + ~/.dotnet/tools）
//! 2. 解析 sts2.dll：必须存在
//! 3. 读 manifest：如 game 记录与当前 dll 元数据匹配且非 force，跳过子进程
//! 4. 否则 spawn_blocking 跑 run_decompile
//! 5. 写 manifest，落到 paths.manifest_path
//!
//! 整体落到现有 Job 框架（progress 通过 ProgressSink；状态机走 transition_to_running
//! / finalize_with_error / Completed），与其它 handler 一致。

use std::path::PathBuf;
use std::sync::Arc;

use super::common::{ProgressEvent, ProgressSink, finalize_with_error, transition_to_running};
use crate::knowledge::{
    DecompileStats, KnowledgePaths, build_record, default_dotnet_tools_dirs, discover_ilspycmd,
    ensure_dirs, read_manifest, run_decompile, write_manifest,
};
use crate::platform::contracts::SubmitKnowledgeRefreshRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};

pub async fn run_knowledge_refresh(
    repo: Arc<dyn JobRepository>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitKnowledgeRefreshRequest,
    knowledge_paths: KnowledgePaths,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    // 1. 解析 ilspycmd 路径
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
        percent: Some(0.05),
        message: Some(format!("ilspycmd: {}", ilspycmd.display())),
        delta: None,
    })
    .await;

    // 2. 校验 sts2.dll
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

    // 3. 确保知识库目录存在
    if let Err(err) = ensure_dirs(&knowledge_paths) {
        finalize_with_error(&repo, &job_id, &format!("ensure_dirs: {err}")).await;
        return;
    }

    // 4. 检查 manifest 缓存命中
    let cached_manifest = match read_manifest(&knowledge_paths.manifest_path) {
        Ok(m) => m,
        Err(err) => {
            // 损坏的 manifest 不致命，但记录到 progress；继续按 force 处理。
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

    if !request.force {
        if let Some(m) = &cached_manifest {
            if let Some(game) = &m.game {
                if game.matches_current_source(&request.sts2_dll_path) {
                    // 缓存命中：直接完成，不调 ilspycmd
                    complete_with_cache_hit(&repo, &sink, &job_id, game).await;
                    return;
                }
            }
        }
    }

    // 5. 跑反编译
    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "spawning-ilspycmd".into(),
        percent: Some(0.2),
        message: Some(format!(
            "decompiling {} → {}",
            request.sts2_dll_path.display(),
            knowledge_paths.game_dir.display()
        )),
        delta: None,
    })
    .await;

    let dll = request.sts2_dll_path.clone();
    let output = knowledge_paths.game_dir.clone();
    let ilspycmd_for_task = ilspycmd.clone();
    let stats_result =
        tokio::task::spawn_blocking(move || run_decompile(&ilspycmd_for_task, &dll, &output))
            .await;
    let stats: DecompileStats = match stats_result {
        Ok(Ok(s)) => s,
        Ok(Err(err)) => {
            finalize_with_error(&repo, &job_id, &format!("decompile: {err}")).await;
            return;
        }
        Err(err) => {
            finalize_with_error(&repo, &job_id, &format!("join blocking: {err}")).await;
            return;
        }
    };

    // 6. 写 manifest
    let mut manifest = cached_manifest.unwrap_or_default();
    let record = match build_record(
        &request.sts2_dll_path,
        stats.cs_file_count,
        stats.total_bytes,
    ) {
        Ok(r) => r,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &format!("build manifest record: {err}")).await;
            return;
        }
    };
    manifest.game = Some(record.clone());
    if let Err(err) = write_manifest(&knowledge_paths.manifest_path, &manifest) {
        finalize_with_error(&repo, &job_id, &format!("write manifest: {err}")).await;
        return;
    }

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
        "cacheHit": false,
        "ilspycmd": ilspycmd.display().to_string(),
        "sourceDll": request.sts2_dll_path.display().to_string(),
        "csFileCount": stats.cs_file_count,
        "totalBytes": stats.total_bytes,
        "exitCode": stats.exit_code,
        "manifestPath": knowledge_paths.manifest_path.display().to_string(),
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!(
            "decompiled {} files, {} bytes",
            stats.cs_file_count, stats.total_bytes
        )),
        delta: None,
    })
    .await;
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

async fn complete_with_cache_hit(
    repo: &Arc<dyn JobRepository>,
    sink: &Arc<dyn ProgressSink>,
    job_id: &JobId,
    record: &crate::knowledge::DecompileRecord,
) {
    let job = match repo.get(job_id).await {
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
        "cacheHit": true,
        "sourceDll": record.source_path.display().to_string(),
        "csFileCount": record.cs_file_count,
        "totalBytes": record.total_bytes,
        "decompiledAt": record.decompiled_at,
    }));
    let _ = repo.update(&job).await;
    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!(
            "cache hit: {} files (from {})",
            record.cs_file_count, record.decompiled_at
        )),
        delta: None,
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::{KnowledgeManifest, build_record, write_manifest};
    use crate::platform::application::JobApplicationService;
    use crate::platform::domain::JobRepository;
    use crate::platform::infra::FileJobRepository;
    use crate::llm::{CompletionRequest, CompletionResponse, CompletionStream, LlmClient, LlmError};
    use async_trait::async_trait;
    use futures_util::stream;

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

        // 准备一个假的 ilspycmd 文件让 resolve 通过；后续会因 dll 缺失早退
        let bin = td.path().join(if cfg!(windows) { "ilspycmd.exe" } else { "ilspycmd" });
        std::fs::write(&bin, b"fake").unwrap();

        let service = make_service(history);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: td.path().join("nope.dll"),
            ilspycmd_path: Some(bin),
            force: false,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.unwrap_or_default().contains("sts2_dll_path not found"));
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
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: dll,
            ilspycmd_path: Some(td.path().join("does-not-exist")),
            force: false,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.unwrap_or_default().contains("ilspycmd_path does not exist"));
    }

    #[tokio::test]
    async fn refresh_cache_hits_when_manifest_matches() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let runtime = td.path().join("runtime");
        let paths = KnowledgePaths::from_runtime_dir(&runtime);
        ensure_dirs(&paths).unwrap();

        let dll = td.path().join("sts2.dll");
        std::fs::write(&dll, b"MZ--placeholder").unwrap();

        // 预写 manifest 模拟"上次反编译"的记录
        let mut manifest = KnowledgeManifest::default();
        manifest.game = Some(build_record(&dll, 42, 12345).unwrap());
        write_manifest(&paths.manifest_path, &manifest).unwrap();

        // 假的 ilspycmd，不应被调起（缓存命中早退）
        let bin = td.path().join(if cfg!(windows) { "ilspycmd.exe" } else { "ilspycmd" });
        std::fs::write(&bin, b"fake").unwrap();

        let service = make_service(history);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: dll,
            ilspycmd_path: Some(bin),
            force: false,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.unwrap();
        assert_eq!(res["cacheHit"], true);
        assert_eq!(res["csFileCount"], 42);
    }

    #[tokio::test]
    async fn refresh_force_bypasses_cache_then_fails_on_invalid_ilspycmd() {
        // force=true 必须穿过缓存逻辑去 spawn 子进程；这里"假 ilspycmd"是非 ELF/PE
        // 文本文件，spawn 会失败（即便发现），从而 handler 报 ProcessFailed/Spawn 类错。
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let runtime = td.path().join("runtime");
        let paths = KnowledgePaths::from_runtime_dir(&runtime);
        ensure_dirs(&paths).unwrap();

        let dll = td.path().join("sts2.dll");
        std::fs::write(&dll, b"MZ--placeholder").unwrap();

        let mut manifest = KnowledgeManifest::default();
        manifest.game = Some(build_record(&dll, 42, 12345).unwrap());
        write_manifest(&paths.manifest_path, &manifest).unwrap();

        let bogus_bin = td.path().join("not-real-binary");
        std::fs::write(&bogus_bin, b"#!/bin/sh\nexit 0").unwrap();

        let service = make_service(history);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let req = SubmitKnowledgeRefreshRequest {
            sts2_dll_path: dll,
            ilspycmd_path: Some(bogus_bin),
            force: true,
        };
        let id = service
            .submit_knowledge_refresh(req, paths, sink)
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        // force=true 一定会跑到 decompile 阶段；可能 Spawn 或 ProcessFailed 任一
        assert_eq!(job.status, JobStatus::Failed);
        let err = job.error.unwrap_or_default();
        assert!(
            err.contains("decompile"),
            "expected decompile error, got: {err}"
        );
    }
}
