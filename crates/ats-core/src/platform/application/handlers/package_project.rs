//! package_project handler：把 `source_dir` 整目录打成 zip。
//!
//! 设计取向：
//! - 用 walkdir 递归，按相对路径写 zip 条目，保留目录结构
//! - 全程同步阻塞——zip crate 不是 async，用 spawn_blocking 包住
//! - 输出路径：用户没传则落 `<source_dir 父目录>/<source_dir 名>-<ts>.zip`
//! - 压缩方法固定 Deflated（zip crate 默认 store，但我们要体积小）

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use walkdir::WalkDir;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use super::common::{ProgressEvent, ProgressSink, finalize_with_error, transition_to_running};
use crate::platform::contracts::SubmitPackageProjectRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};

pub async fn run_package_project(
    repo: Arc<dyn JobRepository>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitPackageProjectRequest,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    let source = request.source_dir.clone();
    if !source.is_dir() {
        finalize_with_error(
            &repo,
            &job_id,
            &format!("source_dir is not a directory: {}", source.display()),
        )
        .await;
        return;
    }

    let output = resolve_output_path(&request);
    if output.is_dir() {
        finalize_with_error(
            &repo,
            &job_id,
            &format!(
                "output_path 指向已存在的目录: {} —— 应该传完整 .zip 文件路径，如 {}\\release.zip",
                output.display(),
                output.display()
            ),
        )
        .await;
        return;
    }
    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "zipping".into(),
        percent: Some(0.1),
        message: Some(format!("zip → {}", output.display())),
        delta: None,
    })
    .await;

    let level = request.compression_level;
    let output_for_task = output.clone();
    let source_for_task = source.clone();

    let result = tokio::task::spawn_blocking(move || {
        zip_directory(&source_for_task, &output_for_task, level)
    })
    .await;

    let stats = match result {
        Ok(Ok(s)) => s,
        Ok(Err(err)) => {
            finalize_with_error(&repo, &job_id, &format!("zip: {err}")).await;
            return;
        }
        Err(err) => {
            finalize_with_error(&repo, &job_id, &format!("join blocking: {err}")).await;
            return;
        }
    };

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
        "sourceDir": source.display().to_string(),
        "outputPath": output.display().to_string(),
        "filesAdded": stats.files,
        "uncompressedBytes": stats.uncompressed_bytes,
        "zipBytes": stats.zip_bytes,
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!(
            "{} files, {} bytes → {} bytes",
            stats.files, stats.uncompressed_bytes, stats.zip_bytes
        )),
        delta: None,
    })
    .await;
}

fn resolve_output_path(request: &SubmitPackageProjectRequest) -> PathBuf {
    if let Some(p) = &request.output_path {
        return p.clone();
    }
    let ts = Utc::now().format("%Y%m%d-%H%M%S");
    let name = request
        .source_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("package");
    let parent = request
        .source_dir
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    parent.join(format!("{name}-{ts}.zip"))
}

struct ZipStats {
    files: u32,
    uncompressed_bytes: u64,
    zip_bytes: u64,
}

fn zip_directory(
    source_dir: &Path,
    output_path: &Path,
    compression_level: Option<i32>,
) -> Result<ZipStats, String> {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create output parent {}: {e}", parent.display()))?;
    }
    let file =
        File::create(output_path).map_err(|e| format!("create {}: {e}", output_path.display()))?;
    let mut writer = zip::ZipWriter::new(file);

    let mut options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    if let Some(lvl) = compression_level {
        // zip crate 的有效区间是 0..=9（Deflated 实际范围 0..=9），传入越界视为默认
        if (0..=9).contains(&lvl) {
            options = options.compression_level(Some(lvl.into()));
        }
    }

    let mut stats = ZipStats {
        files: 0,
        uncompressed_bytes: 0,
        zip_bytes: 0,
    };
    let mut buffer = Vec::with_capacity(8192);

    for entry in WalkDir::new(source_dir).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        let rel = match path.strip_prefix(source_dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if rel.as_os_str().is_empty() {
            continue; // 跳过根目录自身
        }
        let rel_str = rel
            .to_str()
            .ok_or_else(|| format!("non-utf8 path: {}", rel.display()))?
            .replace('\\', "/");

        if entry.file_type().is_dir() {
            // ZipWriter::add_directory 让 unzip 工具能识别空目录
            writer
                .add_directory(format!("{rel_str}/"), options)
                .map_err(|e| format!("add dir {rel_str}: {e}"))?;
        } else if entry.file_type().is_file() {
            writer
                .start_file(rel_str.clone(), options)
                .map_err(|e| format!("start file {rel_str}: {e}"))?;
            buffer.clear();
            File::open(path)
                .map_err(|e| format!("open {}: {e}", path.display()))?
                .read_to_end(&mut buffer)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            writer
                .write_all(&buffer)
                .map_err(|e| format!("write entry {rel_str}: {e}"))?;
            stats.files += 1;
            stats.uncompressed_bytes += buffer.len() as u64;
        }
        // symlink / 其它类型忽略
    }

    let final_file = writer.finish().map_err(|e| format!("finish zip: {e}"))?;
    stats.zip_bytes = final_file.metadata().map(|m| m.len()).unwrap_or(0);
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionRequest, CompletionResponse, CompletionStream, LlmClient, LlmError,
    };
    use crate::platform::application::JobApplicationService;
    use crate::platform::domain::JobRepository;
    use crate::platform::infra::FileJobRepository;
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

    fn populate_sample_tree(root: &Path) {
        std::fs::create_dir_all(root.join("nested/deep")).unwrap();
        std::fs::write(root.join("a.txt"), b"hello").unwrap();
        std::fs::write(root.join("nested/b.cs"), b"public class B {}").unwrap();
        std::fs::write(root.join("nested/deep/c.json"), b"{\"k\":1}").unwrap();
    }

    fn list_zip_entries(zip_path: &Path) -> Vec<String> {
        let f = File::open(zip_path).unwrap();
        let archive = zip::ZipArchive::new(f).unwrap();
        archive.file_names().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn package_project_zips_nested_tree() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let source = td.path().join("artifacts");
        std::fs::create_dir_all(&source).unwrap();
        populate_sample_tree(&source);

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let out = td.path().join("out.zip");
        let req = SubmitPackageProjectRequest {
            source_dir: source.clone(),
            output_path: Some(out.clone()),
            compression_level: Some(5),
        };
        let id = service.submit_package_project(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.expect("result");
        assert_eq!(res["filesAdded"], 3);
        assert!(out.exists());

        let entries = list_zip_entries(&out);
        assert!(entries.iter().any(|e| e == "a.txt"));
        assert!(entries.iter().any(|e| e == "nested/b.cs"));
        assert!(entries.iter().any(|e| e == "nested/deep/c.json"));
    }

    #[tokio::test]
    async fn package_project_default_output_path() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let source = td.path().join("artifacts");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("only.txt"), b"x").unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitPackageProjectRequest {
            source_dir: source.clone(),
            output_path: None,
            compression_level: None,
        };
        let id = service.submit_package_project(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.expect("result");
        let out_path = PathBuf::from(res["outputPath"].as_str().unwrap());
        assert!(
            out_path.exists(),
            "default output should exist: {}",
            out_path.display()
        );
        // 默认名应在 source 父目录下，以 "artifacts-" 开头
        assert_eq!(out_path.parent().unwrap(), td.path());
        let fname = out_path.file_name().unwrap().to_string_lossy().to_string();
        assert!(fname.starts_with("artifacts-") && fname.ends_with(".zip"));
    }

    #[tokio::test]
    async fn package_project_empty_dir_succeeds_with_zero_files() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let source = td.path().join("empty");
        std::fs::create_dir_all(&source).unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let out = td.path().join("empty.zip");
        let req = SubmitPackageProjectRequest {
            source_dir: source,
            output_path: Some(out.clone()),
            compression_level: None,
        };
        let id = service.submit_package_project(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let res = job.result.unwrap();
        assert_eq!(res["filesAdded"], 0);
        assert!(out.exists());
    }

    #[tokio::test]
    async fn package_project_fails_when_output_path_is_existing_dir() {
        // 用户在 UI 里把"输出 zip 路径"填成一个已存在的目录（如 E:\mods\output），
        // Windows 下 File::create 会直接返回 os error 5（拒绝访问），错误信息很迷惑。
        // 这里前置校验：明确告诉用户路径必须是 .zip 文件，而不是目录。
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let source = td.path().join("artifacts");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("only.txt"), b"x").unwrap();

        // 故意把 output_path 指向一个已存在的目录
        let out_dir = td.path().join("existing-output-dir");
        std::fs::create_dir_all(&out_dir).unwrap();

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitPackageProjectRequest {
            source_dir: source,
            output_path: Some(out_dir.clone()),
            compression_level: None,
        };
        let id = service.submit_package_project(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        let err = job.error.unwrap_or_default();
        assert!(
            err.contains("已存在的目录"),
            "expected dir hint, got: {err}"
        );
    }

    #[tokio::test]
    async fn package_project_fails_when_source_is_not_dir() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let phantom = td.path().join("does-not-exist");

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitPackageProjectRequest {
            source_dir: phantom,
            output_path: Some(td.path().join("x.zip")),
            compression_level: None,
        };
        let id = service.submit_package_project(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.unwrap_or_default().contains("not a directory"));
    }
}
