//! package_project handler：按 Game Pack 的有限发布布局打包。
//!
//! 设计取向：
//! - 只收集 Pack 声明的必需文件，不递归包含未声明内容
//! - 全程同步阻塞——zip crate 不是 async，用 spawn_blocking 包住
//! - 输出路径：用户没传则落 `<source_dir 父目录>/<source_dir 名>-<ts>.zip`
//! - 压缩方法固定 Deflated（zip crate 默认 store，但我们要体积小）

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, finalize_with_cancellation,
    finalize_with_failure, finalize_with_success, transition_to_running,
};
use crate::failure::FailureNormalizer;
use crate::game_pack::{PackageLayout, VerifiedGameContext};
use crate::platform::application::CancellationToken;
use crate::platform::artifact::{
    ArtifactFileInput, ArtifactGameContext, ArtifactGeneration, ArtifactPublishRequest,
    ArtifactStore, sha256_bytes, snapshot_evidence,
};
use crate::platform::contracts::SubmitPackageProjectRequest;
use crate::platform::domain::{PackageError, RunId, RunRepository, RunResult};

#[allow(clippy::too_many_arguments)] // Package execution exposes its finite Run and Game Pack inputs directly.
pub async fn run_package_project(
    repo: Arc<dyn RunRepository>,
    sink: Arc<dyn ProgressSink>,
    run_id: RunId,
    request: SubmitPackageProjectRequest,
    layout: PackageLayout,
    mod_id: String,
    game_context: VerifiedGameContext,
    project_root: PathBuf,
    cancellation: CancellationToken,
) {
    if !matches!(
        transition_to_running(&repo, &run_id, &sink, &cancellation).await,
        Ok(true)
    ) {
        return;
    }

    let inputs_sha256 = serde_json::to_vec(&request)
        .map(|bytes| sha256_bytes(&bytes))
        .unwrap_or_else(|_| sha256_bytes(b"package request"));
    let source = request.source_dir.clone();
    if !source.is_dir() {
        fail_package(&repo, &run_id, &sink, PackageError::SourceMissing).await;
        return;
    }

    let output = resolve_output_path(&request);
    if output.is_dir() {
        fail_package(&repo, &run_id, &sink, PackageError::OutputInvalid).await;
        return;
    }
    sink.emit(ProgressEvent {
        run_id: run_id.clone(),
        stage: "zipping".into(),
        percent: Some(0.1),
        message: Some(format!("zip → {}", output.display())),
        delta: None,
    })
    .await;

    let level = request.compression_level;
    let output_for_task = output.clone();
    let source_for_task = source.clone();
    let mod_id_for_task = mod_id.clone();
    let run_id_for_task = run_id.clone();
    let task_cancellation = cancellation.clone();

    let result = tokio::task::spawn_blocking(move || {
        zip_package_layout(
            &source_for_task,
            &output_for_task,
            &layout,
            &mod_id_for_task,
            level,
            &run_id_for_task,
            &task_cancellation,
        )
    })
    .await;

    if let Some(reason) = cancellation.reason() {
        finalize_with_cancellation(&repo, &run_id, &sink, reason).await;
        return;
    }

    let (stats, output_transaction) = match result {
        Ok(Ok(s)) => s,
        Ok(Err(err)) => {
            fail_package(&repo, &run_id, &sink, err).await;
            return;
        }
        Err(_) => {
            fail_package(&repo, &run_id, &sink, PackageError::Worker).await;
            return;
        }
    };

    let store = ArtifactStore::new(project_root);
    let artifact_id = format!("package-{mod_id}");
    let published_relative_path = store.project_relative_ref(&output).ok();
    let published = match store
        .publish_async(ArtifactPublishRequest {
            artifact_id: artifact_id.clone(),
            artifact_kind: "package".into(),
            run_id: run_id.clone(),
            game_context: ArtifactGameContext::from(&game_context),
            evidence: snapshot_evidence(&game_context),
            generation: ArtifactGeneration {
                provider: "local".into(),
                model: "zip".into(),
                inputs_sha256,
            },
            image_processing: None,
            files: vec![ArtifactFileInput {
                role: "package_zip".into(),
                source_path: output.clone(),
                published_relative_path,
            }],
        })
        .await
    {
        Ok(published) => published,
        Err(error) => {
            let _ = output_transaction.rollback();
            finalize_with_failure(
                &repo,
                &run_id,
                &sink,
                FailureNormalizer::artifact("package.publish", &error),
            )
            .await;
            return;
        }
    };
    let legacy_cleanup = match store.begin_legacy_cleanup(&artifact_id, &run_id) {
        Ok(cleanup) => cleanup,
        Err(error) => {
            let _ = store.remove_published_run(&artifact_id, &run_id);
            let _ = output_transaction.rollback();
            finalize_with_failure(
                &repo,
                &run_id,
                &sink,
                FailureNormalizer::artifact("package.cleanup", &error),
            )
            .await;
            return;
        }
    };
    let result = RunResult::Package {
        artifact_manifest_ref: published.artifact_manifest_ref,
        manifest_sha256: published.manifest_sha256,
        artifact_id: artifact_id.clone(),
        files_added: stats.files as usize,
        uncompressed_bytes: stats.uncompressed_bytes,
        package_bytes: stats.zip_bytes,
    };
    if let Some(reason) = cancellation.reason() {
        let legacy_rollback = legacy_cleanup.rollback();
        let artifact_rollback = store.remove_published_run(&artifact_id, &run_id);
        let output_rollback = output_transaction.rollback();
        let _ = (legacy_rollback, artifact_rollback, output_rollback);
        finalize_with_cancellation(&repo, &run_id, &sink, reason).await;
        return;
    }
    if !matches!(
        finalize_with_success(&repo, &run_id, result).await,
        FinalizeOutcome::Succeeded
    ) {
        let legacy_rollback = legacy_cleanup.rollback();
        let artifact_rollback = store.remove_published_run(&artifact_id, &run_id);
        let output_rollback = output_transaction.rollback();
        let _ = (legacy_rollback, artifact_rollback, output_rollback);
        return;
    }
    let _ = legacy_cleanup.commit();
    let _ = output_transaction.commit();

    sink.emit(ProgressEvent {
        run_id,
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

async fn fail_package(
    repo: &Arc<dyn RunRepository>,
    run_id: &RunId,
    sink: &Arc<dyn ProgressSink>,
    error: PackageError,
) {
    finalize_with_failure(
        repo,
        run_id,
        sink,
        FailureNormalizer::package("package", &error),
    )
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

/// 原子打包：把 zip 写到同目录临时文件，全部成功后再 rename 到最终路径。
/// 中途失败 / rename 失败都会清理临时文件——目标位置在 rename 前完全不被触碰，
/// 因此构建失败或取消绝不会留下半截 .zip，也不会覆盖上一份好包。
fn zip_package_layout(
    source_dir: &Path,
    output_path: &Path,
    layout: &PackageLayout,
    mod_id: &str,
    compression_level: Option<i32>,
    run_id: &RunId,
    cancellation: &CancellationToken,
) -> Result<(ZipStats, PackageOutputTransaction), PackageError> {
    let tmp_path = sibling_work_path(output_path, "partial", run_id)?;
    let backup_path = sibling_work_path(output_path, "previous", run_id)?;
    if backup_path.exists() {
        return Err(PackageError::OutputInvalid);
    }
    match zip_to_tmp(
        source_dir,
        &tmp_path,
        layout,
        mod_id,
        compression_level,
        cancellation,
    ) {
        Ok(stats) => {
            if cancellation.is_cancelled() {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(PackageError::Worker);
            }
            let backup = match std::fs::symlink_metadata(output_path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    let _ = std::fs::remove_file(&tmp_path);
                    return Err(PackageError::OutputInvalid);
                }
                Ok(metadata) if metadata.is_file() => {
                    std::fs::rename(output_path, &backup_path).map_err(|source| {
                        let _ = std::fs::remove_file(&tmp_path);
                        PackageError::Io {
                            operation: "backup_output",
                            source,
                        }
                    })?;
                    Some(backup_path)
                }
                Ok(_) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    return Err(PackageError::OutputInvalid);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(source) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    return Err(PackageError::Io {
                        operation: "inspect_output",
                        source,
                    });
                }
            };
            if let Err(source) = std::fs::rename(&tmp_path, output_path) {
                if let Some(previous) = &backup {
                    let _ = std::fs::rename(previous, output_path);
                }
                let _ = std::fs::remove_file(&tmp_path);
                return Err(PackageError::Io {
                    operation: "publish_output",
                    source,
                });
            }
            Ok((
                stats,
                PackageOutputTransaction {
                    output_path: output_path.to_path_buf(),
                    backup_path: backup,
                },
            ))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(e)
        }
    }
}

struct PackageOutputTransaction {
    output_path: PathBuf,
    backup_path: Option<PathBuf>,
}

impl PackageOutputTransaction {
    fn rollback(self) -> Result<(), PackageError> {
        match std::fs::remove_file(&self.output_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(PackageError::Io {
                    operation: "remove_output",
                    source,
                });
            }
        }
        if let Some(backup_path) = self.backup_path {
            std::fs::rename(&backup_path, &self.output_path).map_err(|source| {
                PackageError::Io {
                    operation: "restore_output",
                    source,
                }
            })?;
        }
        Ok(())
    }

    fn commit(self) -> Result<(), PackageError> {
        if let Some(backup_path) = self.backup_path {
            std::fs::remove_file(&backup_path).map_err(|source| PackageError::Io {
                operation: "remove_backup",
                source,
            })?;
        }
        Ok(())
    }
}

fn sibling_work_path(
    output_path: &Path,
    role: &str,
    run_id: &RunId,
) -> Result<PathBuf, PackageError> {
    let file_name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(PackageError::OutputInvalid)?;
    Ok(output_path.with_file_name(format!(".{file_name}.{role}-{}", run_id.0)))
}

fn zip_to_tmp(
    source_dir: &Path,
    output_path: &Path,
    layout: &PackageLayout,
    mod_id: &str,
    compression_level: Option<i32>,
    cancellation: &CancellationToken,
) -> Result<ZipStats, PackageError> {
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|source| PackageError::Io {
            operation: "create_output_parent",
            source,
        })?;
    }
    let file = File::create(output_path).map_err(|source| PackageError::Io {
        operation: "create_output",
        source,
    })?;
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
    let mut buffer = [0_u8; 64 * 1024];

    let canonical_source =
        std::fs::canonicalize(source_dir).map_err(|source| PackageError::Io {
            operation: "resolve_source",
            source,
        })?;
    for declared in &layout.required_files {
        if cancellation.is_cancelled() {
            return Err(PackageError::Worker);
        }
        let rel_str = declared.replace("{mod_id}", mod_id);
        let path = source_dir.join(Path::new(&rel_str));
        let metadata = std::fs::symlink_metadata(&path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                PackageError::RequiredFileMissing {
                    relative_path: rel_str.clone(),
                }
            } else {
                PackageError::Io {
                    operation: "inspect_required_file",
                    source,
                }
            }
        })?;
        if metadata.file_type().is_symlink() {
            return Err(PackageError::Symlink {
                relative_path: rel_str,
            });
        }
        if !metadata.is_file() {
            return Err(PackageError::NotRegularFile {
                relative_path: rel_str,
            });
        }
        let canonical = std::fs::canonicalize(&path).map_err(|source| PackageError::Io {
            operation: "resolve_required_file",
            source,
        })?;
        if !canonical.starts_with(&canonical_source) {
            return Err(PackageError::PathEscape {
                relative_path: rel_str,
            });
        }
        writer
            .start_file(rel_str.clone(), options)
            .map_err(|_| PackageError::Zip)?;
        let mut input = File::open(&canonical).map_err(|source| PackageError::Io {
            operation: "open_required_file",
            source,
        })?;
        let mut file_bytes = 0_u64;
        loop {
            if cancellation.is_cancelled() {
                return Err(PackageError::Worker);
            }
            let read = input.read(&mut buffer).map_err(|source| PackageError::Io {
                operation: "read_required_file",
                source,
            })?;
            if read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..read])
                .map_err(|source| PackageError::Io {
                    operation: "write_zip_entry",
                    source,
                })?;
            file_bytes = file_bytes.saturating_add(read as u64);
        }
        stats.files += 1;
        stats.uncompressed_bytes = stats.uncompressed_bytes.saturating_add(file_bytes);
    }

    let final_file = writer.finish().map_err(|_| PackageError::Zip)?;
    stats.zip_bytes = final_file.metadata().map(|m| m.len()).unwrap_or(0);
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    use crate::game_pack::{
        GamePackLoadPolicy, GamePackLoader, GamePackRegistry, LoadedGamePack, TruthSnapshotStore,
    };
    use crate::llm::{
        CompletionRequest, CompletionResponse, CompletionStream, LlmClient, LlmError,
    };
    use crate::platform::application::RunApplicationService;
    use crate::platform::domain::{RunRepository, RunStatus};
    use crate::platform::infra::FileRunRepository;
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

    async fn wait_terminal(service: &RunApplicationService, id: &RunId) {
        for _ in 0..100 {
            let run = service.get(id).await.unwrap();
            if run.status.is_terminal() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    const MOD_ID: &str = "FixtureMod";

    fn fixture_pack() -> LoadedGamePack {
        GamePackLoader::new(GamePackLoadPolicy::new(
            ["package_layout"],
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
        ))
        .load_str(
            "fixture",
            r#"{
                  "schema_version": 1,
                  "id": "fixture-game",
                  "display_name": "Fixture Game",
                  "capabilities": ["package_layout"],
                  "package_layout": {
                    "required_files": [
                      "runtime/core.bin",
                      "payload/{mod_id}.bundle"
                    ]
                  }
                }"#,
        )
        .unwrap()
    }

    fn fixture_context(runtime_root: &Path) -> VerifiedGameContext {
        let pack = fixture_pack();
        let store = TruthSnapshotStore::new(runtime_root, &pack);
        store
            .begin(&pack)
            .unwrap()
            .finalize(BTreeMap::from([("fixture".into(), "1".into())]))
            .unwrap();
        let registry = GamePackRegistry::from_packs([pack]).unwrap();
        VerifiedGameContext::open_current(runtime_root, &registry, "fixture-game").unwrap()
    }

    fn populate_sample_tree(root: &Path) {
        std::fs::create_dir_all(root.join("runtime")).unwrap();
        std::fs::create_dir_all(root.join("payload")).unwrap();
        std::fs::write(root.join("runtime/core.bin"), b"core").unwrap();
        std::fs::write(
            root.join("payload").join(format!("{MOD_ID}.bundle")),
            b"mod",
        )
        .unwrap();
        std::fs::write(root.join("not-declared.txt"), b"must not ship").unwrap();
    }

    #[test]
    fn zip_package_layout_writes_atomically_and_leaves_no_partial() {
        let td = tempfile::TempDir::new().unwrap();
        let src = td.path().join("src");
        populate_sample_tree(&src);
        let out = td.path().join("pkg.zip");

        let pack = fixture_pack();
        let run_id = RunId::new();
        let (stats, transaction) = zip_package_layout(
            &src,
            &out,
            pack.package_layout.as_ref().unwrap(),
            MOD_ID,
            None,
            &run_id,
            &CancellationToken::new(),
        )
        .unwrap();
        transaction.commit().unwrap();

        assert!(out.exists(), "final zip should exist");
        assert_eq!(stats.files, 2);
        assert!(stats.zip_bytes > 0);
        // 临时包不应残留
        assert!(
            !sibling_work_path(&out, "partial", &run_id)
                .unwrap()
                .exists(),
            "no .partial temp should remain after success"
        );
    }

    fn list_zip_entries(zip_path: &Path) -> Vec<String> {
        let f = File::open(zip_path).unwrap();
        let archive = zip::ZipArchive::new(f).unwrap();
        archive.file_names().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn package_project_uses_only_declared_layout() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let source = td.path().join("artifacts");
        std::fs::create_dir_all(&source).unwrap();
        populate_sample_tree(&source);

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let out = td.path().join("out.zip");
        let req = SubmitPackageProjectRequest {
            source_dir: source.clone(),
            output_path: Some(out.clone()),
            compression_level: Some(5),
        };
        let id = service
            .submit_package_project(
                req,
                fixture_context(td.path()),
                td.path().to_path_buf(),
                MOD_ID.into(),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        let res = serde_json::to_value(run.result.expect("result")).unwrap();
        assert_eq!(res["filesAdded"], 2);
        assert!(out.exists());

        let entries = list_zip_entries(&out);
        assert_eq!(
            entries,
            vec!["runtime/core.bin", "payload/FixtureMod.bundle"]
        );
    }

    #[tokio::test]
    async fn manifest_publish_failure_restores_previous_package() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let source = td.path().join("artifacts");
        std::fs::create_dir_all(&source).unwrap();
        populate_sample_tree(&source);
        std::fs::create_dir_all(source.join(format!("package-{MOD_ID}"))).unwrap();
        std::fs::write(
            source.join(format!("package-{MOD_ID}/runs")),
            b"force directory conflict",
        )
        .unwrap();
        let output = td.path().join("out.zip");
        std::fs::write(&output, b"previous package").unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let service = RunApplicationService::new(repo, Arc::new(DummyLlm));
        let id = service
            .submit_package_project(
                SubmitPackageProjectRequest {
                    source_dir: source,
                    output_path: Some(output.clone()),
                    compression_level: None,
                },
                fixture_context(td.path()),
                td.path().to_path_buf(),
                MOD_ID.into(),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.result.is_none());
        assert_eq!(run.failure.as_ref().unwrap().code, "artifact.path_invalid");
        assert_eq!(run.failure.as_ref().unwrap().stage, "package.publish");
        assert_eq!(std::fs::read(output).unwrap(), b"previous package");
    }

    #[tokio::test]
    async fn package_project_default_output_path() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let source = td.path().join("artifacts");
        std::fs::create_dir_all(&source).unwrap();
        populate_sample_tree(&source);

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let req = SubmitPackageProjectRequest {
            source_dir: source.clone(),
            output_path: None,
            compression_level: None,
        };
        let id = service
            .submit_package_project(
                req,
                fixture_context(td.path()),
                td.path().to_path_buf(),
                MOD_ID.into(),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        let res = serde_json::to_value(run.result.expect("result")).unwrap();
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(td.path().join(res["artifactManifestRef"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        let out_path = td.path().join(
            manifest["files"][0]["publishedRelativePath"]
                .as_str()
                .unwrap(),
        );
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
    async fn package_project_rejects_missing_required_files() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let source = td.path().join("empty");
        std::fs::create_dir_all(&source).unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let out = td.path().join("empty.zip");
        let req = SubmitPackageProjectRequest {
            source_dir: source,
            output_path: Some(out.clone()),
            compression_level: None,
        };
        let id = service
            .submit_package_project(
                req,
                fixture_context(td.path()),
                td.path().to_path_buf(),
                MOD_ID.into(),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "package.required_file_missing");
        assert_eq!(
            failure
                .context
                .as_ref()
                .and_then(|context| context.project_relative_path.as_deref()),
            Some("runtime/core.bin")
        );
        assert!(!out.exists());
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
        populate_sample_tree(&source);

        // 故意把 output_path 指向一个已存在的目录
        let out_dir = td.path().join("existing-output-dir");
        std::fs::create_dir_all(&out_dir).unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let req = SubmitPackageProjectRequest {
            source_dir: source,
            output_path: Some(out_dir.clone()),
            compression_level: None,
        };
        let id = service
            .submit_package_project(
                req,
                fixture_context(td.path()),
                td.path().to_path_buf(),
                MOD_ID.into(),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.failure.as_ref().unwrap().code, "package.output_invalid");
    }

    #[tokio::test]
    async fn package_project_fails_when_source_is_not_dir() {
        let td = tempfile::TempDir::new().unwrap();
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let phantom = td.path().join("does-not-exist");

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(DummyLlm);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = RunApplicationService::new(repo, llm);

        let req = SubmitPackageProjectRequest {
            source_dir: phantom,
            output_path: Some(td.path().join("x.zip")),
            compression_level: None,
        };
        let id = service
            .submit_package_project(
                req,
                fixture_context(td.path()),
                td.path().to_path_buf(),
                MOD_ID.into(),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.failure.as_ref().unwrap().code, "package.source_missing");
    }
}
