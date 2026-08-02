//! asset_generate handler：先 image_gen 出图，再 code_generate 出 .cs。
//!
//! 决策流：
//! 1. transition_to_running
//! 2. 如 image_prompt 非空 → 调 ImageGenClient → 背景去除 → 质量门禁
//!    → 写诊断原图、处理图和质量报告 → 把正式资源路径塞回 image_paths
//! 3. 用 PromptAssembler.assemble_asset_prompt 装 prompt
//! 4. 生成结构化 C# + 双语本地化 bundle，并通过隔离编译门禁
//! 5. 门禁通过后落 run.result；失败时回滚正式生成目录
//!
//! image_gen 失败 → 整任务 Failed（不继续 code）。
//! image_prompt 留空 → 跳过 image_gen，仅跑 code_generate（与 code_generate(asset)
//! 等价但保留原 image_paths）。

use std::path::PathBuf;
use std::sync::Arc;

use tokio::fs;

use super::asset_bundle::{AssetBundleGeneration, runtime_image_paths_for, validate_project_scope};
use super::asset_compile::AssetCompileValidator;
use super::code_generate::{
    finalize_asset_bundle_error, publish_generated_artifact, sanitize_entity_name,
};
use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, finalize_with_cancellation,
    finalize_with_failure, finalize_with_success, transition_to_running,
};
use crate::codegen::PromptAssembler;
use crate::failure::{ActionableFailure, FailureDiagnostic, FailureNormalizer};
use crate::game_pack::VerifiedGameContext;
use crate::image_gen::{ImageGenClient, ImageGenRequest};
use crate::image_proc::{ImageProcClient, ImageProcError, ImageQualitySpec, analyze_png_quality};
use crate::llm::LlmClient;
use crate::platform::application::CancellationToken;
use crate::platform::artifact::sha256_bytes;
use crate::platform::contracts::SubmitAssetGenerateRequest;
use crate::platform::domain::{RunId, RunRepository, TokenUsage};

#[allow(clippy::too_many_arguments)] // handler 注入 run 与外部 adapter，避免为单一调用引入浅 Context 结构体
pub(crate) async fn run_asset_generate(
    repo: Arc<dyn RunRepository>,
    llm: Arc<dyn LlmClient>,
    image_gen: Arc<dyn ImageGenClient>,
    image_proc: Arc<dyn ImageProcClient>,
    sink: Arc<dyn ProgressSink>,
    run_id: RunId,
    request: SubmitAssetGenerateRequest,
    game_context: VerifiedGameContext,
    artifacts_dir: PathBuf,
    compile_validator: Arc<dyn AssetCompileValidator>,
    cancellation: CancellationToken,
) {
    if !matches!(
        transition_to_running(&repo, &run_id, &sink, &cancellation).await,
        Ok(true)
    ) {
        return;
    }

    let mut asset_request = request.asset_request.clone();
    if validate_project_scope(&asset_request.project_root, &artifacts_dir).is_err() {
        finalize_with_failure(
            &repo,
            &run_id,
            &sink,
            ActionableFailure::invalid_input(
                "asset_generate.project_scope",
                "The asset project path is outside the active project.",
            ),
        )
        .await;
        return;
    }
    if game_context
        .pack()
        .resource_spec(&asset_request.asset_type)
        .is_none()
    {
        finalize_with_failure(
            &repo,
            &run_id,
            &sink,
            ActionableFailure::invalid_input(
                "asset_generate.asset_type",
                "The selected Game Pack does not support this asset type.",
            ),
        )
        .await;
        return;
    }
    let entity_name = sanitize_entity_name(&asset_request.asset_name);
    let project_root = artifacts_dir
        .parent()
        .expect("validated artifacts directory has a project parent");
    let diagnostic_ref = format!(".ats/diagnostics/{}", run_id.0);
    let target_dir = project_root.join(&diagnostic_ref);

    // 1. 出图（若提供 image_prompt）
    let mut png_path: Option<PathBuf> = None;
    let mut runtime_image_source: Option<PathBuf> = None;
    let mut quality_path: Option<PathBuf> = None;
    let mut image_processing = None;
    let mut diagnostics_written = false;

    if let Some(prompt) = &request.image_prompt {
        let prompt = prompt.trim();
        if !prompt.is_empty() {
            sink.emit(ProgressEvent {
                run_id: run_id.clone(),
                stage: "image-gen-start".into(),
                percent: Some(0.05),
                message: Some(format!("generating image: {} chars prompt", prompt.len())),
                delta: None,
            })
            .await;

            let img_req = ImageGenRequest {
                prompt: prompt.to_string(),
                n: 1,
                size: request.image_size.clone(),
                model: None,
            };
            let image_result = tokio::select! {
                reason = cancellation.cancelled() => {
                    finalize_with_cancellation(&repo, &run_id, &sink, reason).await;
                    return;
                }
                result = image_gen.generate(img_req) => result,
            };
            let img_resp = match image_result {
                Ok(r) => r,
                Err(err) => {
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        FailureNormalizer::image("asset_generate.image", &err),
                    )
                    .await;
                    return;
                }
            };
            let first = match img_resp.images.first() {
                Some(i) => i,
                None => {
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        FailureNormalizer::image(
                            "asset_generate.image",
                            &crate::image_gen::ImageGenError::Empty,
                        ),
                    )
                    .await;
                    return;
                }
            };
            let ext = if first.format_hint.is_empty() {
                "png".to_string()
            } else {
                first.format_hint.clone()
            };
            let path = target_dir.join(format!("{entity_name}.{ext}"));
            if let Err(err) = fs::create_dir_all(&target_dir).await {
                finalize_with_failure(
                    &repo,
                    &run_id,
                    &sink,
                    FailureNormalizer::io(
                        "run.storage_failed",
                        "asset_generate.diagnostics_create",
                        "Image diagnostics could not be created.",
                        &err,
                    ),
                )
                .await;
                return;
            }
            if let Err(err) = fs::write(&path, &first.bytes).await {
                finalize_with_failure(
                    &repo,
                    &run_id,
                    &sink,
                    FailureNormalizer::io(
                        "run.storage_failed",
                        "asset_generate.diagnostics_write",
                        "The generated image diagnostic could not be saved.",
                        &err,
                    ),
                )
                .await;
                return;
            }
            diagnostics_written = true;
            // 背景去除：调注入的 ImageProcClient（生产路径是 BgRemoverChain
            // ML→Simple 回退）。处理失败或质量门禁失败时保留诊断文件，但不交付原图。
            let raw_bytes = first.bytes.clone();
            let rembg_path = target_dir.join(format!("{entity_name}.rembg.png"));
            let processed_result = image_proc
                .remove_background(&raw_bytes, &cancellation)
                .await;
            if let Some(reason) = cancellation.reason() {
                finalize_asset_cancellation(
                    &repo,
                    &run_id,
                    &sink,
                    &target_dir,
                    diagnostics_written,
                    reason,
                )
                .await;
                return;
            }
            match processed_result {
                Ok(outcome) => {
                    let processed = outcome.png;
                    image_processing = Some(outcome.provenance);
                    if let Err(err) = crate::fs_atomic::write_atomic(&rembg_path, &processed).await
                    {
                        finalize_with_failure(
                            &repo,
                            &run_id,
                            &sink,
                            with_run_diagnostic(
                                FailureNormalizer::io(
                                    "run.storage_failed",
                                    "asset_generate.processed_write",
                                    "The processed image diagnostic could not be saved.",
                                    &err,
                                ),
                                &run_id,
                            ),
                        )
                        .await;
                        return;
                    }

                    let report = match analyze_png_quality(&processed, ImageQualitySpec::default())
                    {
                        Ok(report) => report,
                        Err(err) => {
                            finalize_with_failure(
                                &repo,
                                &run_id,
                                &sink,
                                with_run_diagnostic(
                                    FailureNormalizer::image_proc(
                                        "asset_generate.quality_analyze",
                                        &err,
                                    ),
                                    &run_id,
                                ),
                            )
                            .await;
                            return;
                        }
                    };
                    let report_path = target_dir.join("image-quality.json");
                    let report_bytes = match serde_json::to_vec_pretty(&report) {
                        Ok(bytes) => bytes,
                        Err(_) => {
                            finalize_with_failure(
                                &repo,
                                &run_id,
                                &sink,
                                with_run_diagnostic(
                                    ActionableFailure::unclassified(
                                        "asset_generate.quality_serialize",
                                    ),
                                    &run_id,
                                ),
                            )
                            .await;
                            return;
                        }
                    };
                    if let Err(err) =
                        crate::fs_atomic::write_atomic(&report_path, &report_bytes).await
                    {
                        finalize_with_failure(
                            &repo,
                            &run_id,
                            &sink,
                            with_run_diagnostic(
                                FailureNormalizer::io(
                                    "run.storage_failed",
                                    "asset_generate.quality_write",
                                    "The image quality diagnostic could not be saved.",
                                    &err,
                                ),
                                &run_id,
                            ),
                        )
                        .await;
                        return;
                    }
                    quality_path = Some(report_path.clone());
                    if !report.accepted {
                        let quality_error = ImageProcError::Quality(report.rejection_summary());
                        finalize_with_failure(
                            &repo,
                            &run_id,
                            &sink,
                            with_run_diagnostic(
                                FailureNormalizer::image_proc(
                                    "asset_generate.quality_gate",
                                    &quality_error,
                                ),
                                &run_id,
                            ),
                        )
                        .await;
                        return;
                    }
                    runtime_image_source = Some(rembg_path.clone());
                }
                Err(ImageProcError::Cancelled) => {
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        ActionableFailure::unclassified("asset_generate.cancel_without_reason"),
                    )
                    .await;
                    return;
                }
                Err(err) => {
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        with_run_diagnostic(
                            FailureNormalizer::image_proc("asset_generate.remove_background", &err),
                            &run_id,
                        ),
                    )
                    .await;
                    return;
                }
            }
            if let Some(reason) = cancellation.reason() {
                finalize_asset_cancellation(
                    &repo,
                    &run_id,
                    &sink,
                    &target_dir,
                    diagnostics_written,
                    reason,
                )
                .await;
                return;
            }
            png_path = Some(path);

            asset_request.image_paths =
                match runtime_image_paths_for(&asset_request, game_context.pack()) {
                    Ok(paths) => paths,
                    Err(_) => {
                        finalize_with_failure(
                            &repo,
                            &run_id,
                            &sink,
                            with_optional_run_diagnostic(
                                ActionableFailure::unclassified(
                                    "asset_generate.runtime_image_plan",
                                ),
                                &run_id,
                                diagnostics_written,
                            ),
                        )
                        .await;
                        return;
                    }
                };

            sink.emit(ProgressEvent {
                run_id: run_id.clone(),
                stage: "image-gen-done".into(),
                percent: Some(0.4),
                message: Some(format!("image written ({} bytes)", first.bytes.len())),
                delta: None,
            })
            .await;
        }
    }

    // 2. 装 codegen prompt
    let assembler = PromptAssembler::built_in();
    let prompt_assembly =
        match assembler.assemble_asset_prompt_with_evidence(&asset_request, &game_context) {
            Ok(assembly) => assembly,
            Err(_) => {
                finalize_with_failure(
                    &repo,
                    &run_id,
                    &sink,
                    with_optional_run_diagnostic(
                        ActionableFailure::unclassified("asset_generate.prompt"),
                        &run_id,
                        diagnostics_written,
                    ),
                )
                .await;
                return;
            }
        };

    sink.emit(ProgressEvent {
        run_id: run_id.clone(),
        stage: "code-gen-start".into(),
        percent: Some(0.45),
        message: Some(format!("LLM prompt {} chars", prompt_assembly.prompt.len())),
        delta: None,
    })
    .await;
    let inputs_sha256 = sha256_bytes(prompt_assembly.prompt.as_bytes());

    // 3. 生成结构化 bundle，写入后执行隔离编译门禁
    let generator = AssetBundleGeneration::new(
        Arc::clone(&llm),
        compile_validator,
        Arc::clone(&sink),
        cancellation.clone(),
    );
    let mut artifact = match generator
        .generate(
            &run_id,
            prompt_assembly.prompt,
            game_context.pack(),
            &asset_request,
            runtime_image_source.as_deref(),
        )
        .await
    {
        Ok(artifact) => artifact,
        Err(super::asset_bundle::AssetBundleError::Cancelled) => {
            if let Some(reason) = cancellation.reason() {
                finalize_asset_cancellation(
                    &repo,
                    &run_id,
                    &sink,
                    &target_dir,
                    diagnostics_written,
                    reason,
                )
                .await;
            }
            return;
        }
        Err(err) => {
            finalize_asset_bundle_error(
                &repo,
                &run_id,
                &sink,
                err,
                diagnostics_written.then(|| diagnostic_ref.clone()),
            )
            .await;
            return;
        }
    };

    // 4. Publish an immutable snapshot before the Run succeeds.
    let mut files = vec![("csharp".to_string(), artifact.cs_path.clone())];
    files.extend(
        artifact
            .localization_paths
            .iter()
            .cloned()
            .map(|path| ("localization".to_string(), path)),
    );
    files.extend(
        artifact
            .runtime_image_paths
            .iter()
            .cloned()
            .map(|path| ("runtime_image".to_string(), path)),
    );
    let mut diagnostic_files = Vec::new();
    if let Some(path) = &png_path {
        diagnostic_files.push(("source_image".to_string(), path.clone()));
    }
    if let Some(path) = &runtime_image_source {
        diagnostic_files.push(("processed_image".to_string(), path.clone()));
    }
    if let Some(path) = &quality_path {
        diagnostic_files.push(("image_quality".to_string(), path.clone()));
    }
    let published = match publish_generated_artifact(
        &artifacts_dir,
        &run_id,
        &game_context,
        &asset_request.asset_type,
        artifact.entity_name.clone(),
        artifact.model.clone(),
        inputs_sha256,
        TokenUsage {
            input_tokens: artifact.usage_in,
            output_tokens: artifact.usage_out,
        },
        prompt_assembly.evidence,
        image_processing,
        files,
        diagnostic_files,
    )
    .await
    {
        Ok(published) => published,
        Err(error) => {
            let rollback = artifact.rollback_writes().await;
            let _ = rollback;
            finalize_with_failure(
                &repo,
                &run_id,
                &sink,
                with_optional_run_diagnostic(
                    FailureNormalizer::artifact("asset_generate.publish", &error),
                    &run_id,
                    diagnostics_written,
                ),
            )
            .await;
            return;
        }
    };
    if diagnostics_written && tokio::fs::remove_dir_all(&target_dir).await.is_err() {
        let artifact_rollback = published.rollback().await;
        let file_rollback = artifact.rollback_writes().await;
        let _ = (artifact_rollback, file_rollback);
        finalize_with_failure(
            &repo,
            &run_id,
            &sink,
            with_run_diagnostic(
                ActionableFailure::unclassified("asset_generate.diagnostics_cleanup"),
                &run_id,
            ),
        )
        .await;
        return;
    }
    if let Some(reason) = cancellation.reason() {
        let artifact_rollback = published.rollback().await;
        let file_rollback = artifact.rollback_writes().await;
        let _ = (artifact_rollback, file_rollback);
        finalize_with_cancellation(&repo, &run_id, &sink, reason).await;
        return;
    }
    if !matches!(
        finalize_with_success(&repo, &run_id, published.result.clone()).await,
        FinalizeOutcome::Succeeded
    ) {
        let artifact_rollback = published.rollback().await;
        let file_rollback = artifact.rollback_writes().await;
        let _ = (artifact_rollback, file_rollback);
        return;
    }
    let _ = published.commit().await;
    artifact.commit_writes();

    sink.emit(ProgressEvent {
        run_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!(
            "wrote {}{}",
            artifact.cs_path.display(),
            if let Some(p) = &png_path {
                format!(" + {}", p.display())
            } else {
                String::new()
            }
        )),
        delta: None,
    })
    .await;
}

async fn finalize_asset_cancellation(
    repo: &Arc<dyn RunRepository>,
    run_id: &RunId,
    sink: &Arc<dyn ProgressSink>,
    diagnostic_dir: &std::path::Path,
    diagnostics_written: bool,
    reason: crate::platform::domain::CancellationReason,
) {
    if diagnostics_written
        && let Err(error) = tokio::fs::remove_dir_all(diagnostic_dir).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(
            run_id = %run_id.0,
            io_kind = ?error.kind(),
            "failed to remove cancelled image diagnostics"
        );
    }
    finalize_with_cancellation(repo, run_id, sink, reason).await;
}

fn with_optional_run_diagnostic(
    failure: ActionableFailure,
    run_id: &RunId,
    available: bool,
) -> ActionableFailure {
    if available {
        with_run_diagnostic(failure, run_id)
    } else {
        failure
    }
}

fn with_run_diagnostic(failure: ActionableFailure, run_id: &RunId) -> ActionableFailure {
    failure.with_diagnostic(FailureDiagnostic::for_run(
        run_id,
        "Run diagnostics are available for this failed execution.",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::AssetCodegenRequest;
    use crate::image_gen::{GeneratedImage, ImageGenError, ImageGenResponse};
    use crate::image_proc::SimpleBgRemover;
    use crate::knowledge::test_support::fixture_game_context;
    use crate::llm::{
        CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmError,
        StreamEvent, Usage,
    };
    use crate::platform::application::RunApplicationService;
    use crate::platform::application::handlers::asset_compile::{
        AssetCompileValidator, CompileValidation,
    };
    use crate::platform::domain::RunRepository;
    use crate::platform::domain::RunStatus;
    use crate::platform::infra::FileRunRepository;
    use async_trait::async_trait;
    use futures_util::stream;
    use image::{ImageFormat, Rgba, RgbaImage};
    use std::collections::VecDeque;
    use std::io::Cursor;
    use std::path::Path;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::sync::{Barrier, Notify};

    struct ScriptedLlm {
        events: Mutex<Vec<Result<StreamEvent, LlmError>>>,
    }

    #[async_trait]
    impl LlmClient for ScriptedLlm {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            unimplemented!()
        }
        async fn stream(&self, _: CompletionRequest) -> Result<CompletionStream, LlmError> {
            let evs: Vec<_> = self.events.lock().unwrap().drain(..).collect();
            Ok(Box::pin(stream::iter(evs)))
        }
    }

    struct SequencedLlm {
        responses: Mutex<VecDeque<Vec<Result<StreamEvent, LlmError>>>>,
    }

    #[async_trait]
    impl LlmClient for SequencedLlm {
        async fn complete(&self, _: CompletionRequest) -> Result<CompletionResponse, LlmError> {
            unimplemented!()
        }

        async fn stream(&self, _: CompletionRequest) -> Result<CompletionStream, LlmError> {
            let events = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_default();
            Ok(Box::pin(stream::iter(events)))
        }
    }

    /// Mock image gen：返回预设的 bytes。
    struct MockImageGen {
        bytes: Vec<u8>,
        called: Mutex<u32>,
    }

    impl MockImageGen {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                bytes,
                called: Mutex::new(0),
            }
        }
        fn call_count(&self) -> u32 {
            *self.called.lock().unwrap()
        }
    }

    #[async_trait]
    impl ImageGenClient for MockImageGen {
        async fn generate(
            &self,
            _request: ImageGenRequest,
        ) -> Result<ImageGenResponse, ImageGenError> {
            *self.called.lock().unwrap() += 1;
            Ok(ImageGenResponse {
                model: "mock-image-model".into(),
                images: vec![GeneratedImage {
                    bytes: self.bytes.clone(),
                    format_hint: "png".into(),
                }],
                revised_prompt: Some("a revised prompt".into()),
            })
        }
    }

    struct BlockingImageProc {
        entered: Arc<Barrier>,
        release: Arc<Notify>,
    }

    #[async_trait]
    impl ImageProcClient for BlockingImageProc {
        fn processor(&self) -> crate::image_proc::ImageProcessor {
            crate::image_proc::ImageProcessor::Simple
        }

        async fn remove_background(
            &self,
            input_png: &[u8],
            cancellation: &CancellationToken,
        ) -> Result<crate::image_proc::ImageProcOutcome, ImageProcError> {
            self.entered.wait().await;
            self.release.notified().await;
            if cancellation.is_cancelled() {
                Err(ImageProcError::Cancelled)
            } else {
                Ok(crate::image_proc::ImageProcOutcome {
                    png: input_png.to_vec(),
                    provenance: crate::image_proc::ImageProcessingProvenance::simple(),
                })
            }
        }
    }

    /// 永远报错的 image gen。
    struct FailingImageGen;

    #[async_trait]
    impl ImageGenClient for FailingImageGen {
        async fn generate(
            &self,
            _request: ImageGenRequest,
        ) -> Result<ImageGenResponse, ImageGenError> {
            Err(ImageGenError::Http {
                status: 500,
                message: "simulated".into(),
            })
        }
    }

    struct PassingCompileValidator;

    #[async_trait]
    impl AssetCompileValidator for PassingCompileValidator {
        async fn validate(
            &self,
            _project_root: &Path,
            _run_id: &RunId,
            _cancellation: &CancellationToken,
        ) -> Result<CompileValidation, String> {
            Ok(CompileValidation {
                exit_code: 0,
                stdout_tail: "Build succeeded. 0 Error(s)".into(),
                stderr_tail: String::new(),
            })
        }
    }

    struct FailingCompileValidator;

    #[async_trait]
    impl AssetCompileValidator for FailingCompileValidator {
        async fn validate(
            &self,
            _project_root: &Path,
            _run_id: &RunId,
            _cancellation: &CancellationToken,
        ) -> Result<CompileValidation, String> {
            Err("simulated compile failure".into())
        }
    }

    #[derive(Default)]
    struct CountingCompileValidator {
        calls: AtomicU32,
    }

    #[async_trait]
    impl AssetCompileValidator for CountingCompileValidator {
        async fn validate(
            &self,
            _project_root: &Path,
            _run_id: &RunId,
            _cancellation: &CancellationToken,
        ) -> Result<CompileValidation, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CompileValidation {
                exit_code: 0,
                stdout_tail: String::new(),
                stderr_tail: String::new(),
            })
        }
    }

    fn ok_code_events(asset_name: &str) -> Vec<Result<StreamEvent, LlmError>> {
        let key = format!(
            "DEMOMOD-{}",
            crate::codegen::asset_localization_key_segment(asset_name)
        );
        let output = serde_json::json!({
            "csharp": format!("public sealed class {} {{}}", sanitize_entity_name(asset_name)),
            "localization": {
                "eng": {
                    format!("{key}.title"): asset_name,
                    format!("{key}.description"): "English description",
                    format!("{key}.flavor"): "English flavor"
                },
                "zhs": {
                    format!("{key}.title"): "中文名称",
                    format!("{key}.description"): "中文描述",
                    format!("{key}.flavor"): "中文风味"
                }
            }
        })
        .to_string();
        vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::Delta { text: output }),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::EndTurn,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            }),
        ]
    }

    fn code_events_with_csharp(
        asset_name: &str,
        csharp: &str,
    ) -> Vec<Result<StreamEvent, LlmError>> {
        let key = format!(
            "DEMOMOD-{}",
            crate::codegen::asset_localization_key_segment(asset_name)
        );
        let output = serde_json::json!({
            "csharp": csharp,
            "localization": {
                "eng": {
                    format!("{key}.title"): asset_name,
                    format!("{key}.description"): "English description",
                    format!("{key}.flavor"): "English flavor"
                },
                "zhs": {
                    format!("{key}.title"): "中文名称",
                    format!("{key}.description"): "中文描述",
                    format!("{key}.flavor"): "中文风味"
                }
            }
        })
        .to_string();
        vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::Delta { text: output }),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::EndTurn,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            }),
        ]
    }

    fn code_events_with_description(
        asset_name: &str,
        description: &str,
    ) -> Vec<Result<StreamEvent, LlmError>> {
        let key = format!(
            "DEMOMOD-{}",
            crate::codegen::asset_localization_key_segment(asset_name)
        );
        let output = serde_json::json!({
            "csharp": format!("public sealed class {} {{}}", sanitize_entity_name(asset_name)),
            "localization": {
                "eng": {
                    format!("{key}.title"): asset_name,
                    format!("{key}.description"): description,
                    format!("{key}.flavor"): "English flavor"
                },
                "zhs": {
                    format!("{key}.title"): "中文名称",
                    format!("{key}.description"): description,
                    format!("{key}.flavor"): "中文风味"
                }
            }
        })
        .to_string();
        vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::Delta { text: output }),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::EndTurn,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            }),
        ]
    }

    fn make_request(
        project_root: &Path,
        asset_name: &str,
        image_prompt: Option<&str>,
    ) -> SubmitAssetGenerateRequest {
        make_request_for_type(project_root, asset_name, "card", image_prompt)
    }

    fn make_request_for_type(
        project_root: &Path,
        asset_name: &str,
        asset_type: &str,
        image_prompt: Option<&str>,
    ) -> SubmitAssetGenerateRequest {
        SubmitAssetGenerateRequest {
            asset_request: AssetCodegenRequest {
                design_description: "造成 10 点伤害".into(),
                asset_type: asset_type.into(),
                asset_name: asset_name.into(),
                image_paths: vec![],
                project_root: project_root.to_path_buf(),
                name_zhs: "".into(),
                skip_build: true,
            },
            image_prompt: image_prompt.map(|s| s.to_string()),
            image_size: None,
        }
    }

    fn encode_png(image: &RgbaImage) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, ImageFormat::Png).unwrap();
        output.into_inner()
    }

    fn valid_subject_png() -> Vec<u8> {
        let mut image = RgbaImage::from_pixel(64, 64, Rgba([255, 255, 255, 255]));
        for y in 16..48 {
            for x in 16..48 {
                image.put_pixel(x, y, Rgba([200, 40, 30, 255]));
            }
        }
        encode_png(&image)
    }

    fn checkerboard_subject_png() -> Vec<u8> {
        let mut image = RgbaImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let value = if (x / 8 + y / 8) % 2 == 0 { 255 } else { 190 };
                image.put_pixel(x, y, Rgba([value, value, value, 255]));
            }
        }
        for y in 20..44 {
            for x in 20..44 {
                image.put_pixel(x, y, Rgba([180, 20, 20, 255]));
            }
        }
        encode_png(&image)
    }

    fn prepare_project(root: &Path) {
        std::fs::write(
            root.join("project.json"),
            r#"{"name":"demo","csharp_name":"DemoMod","game_id":"sts2","scaffolded":true,"generated_files":[],"build_output_dir":null}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("MainFile.cs"),
            "namespace DemoMod; public class MainFile {}",
        )
        .unwrap();
    }

    fn service_with_compile_validator(
        repo: Arc<dyn RunRepository>,
        llm: Arc<dyn LlmClient>,
    ) -> RunApplicationService {
        RunApplicationService::new(repo, llm)
            .with_asset_compile_validator(Arc::new(PassingCompileValidator))
    }

    fn service_with_validator(
        repo: Arc<dyn RunRepository>,
        llm: Arc<dyn LlmClient>,
        validator: Arc<dyn AssetCompileValidator>,
    ) -> RunApplicationService {
        RunApplicationService::new(repo, llm).with_asset_compile_validator(validator)
    }

    async fn wait_terminal(service: &RunApplicationService, id: &RunId) {
        for _ in 0..200 {
            let run = service.get(id).await.unwrap();
            if run.status.is_terminal() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn happy_path_writes_png_and_cs() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::create_dir_all(artifacts.join("AlphaCard")).unwrap();
        std::fs::write(artifacts.join("AlphaCard/raw.md"), b"legacy").unwrap();
        let kp = fixture_game_context(
            td.path(),
            &[(
                "AlphaCard.cs",
                "public class AlphaCard { public void Play() {} }",
            )],
            &[],
        );

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events("AlphaCard")),
        });
        let image_gen: Arc<dyn ImageGenClient> = Arc::new(MockImageGen::new(valid_subject_png()));
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = service_with_compile_validator(repo, llm);

        let req = make_request(td.path(), "AlphaCard", Some("draw an alpha card art"));
        let id = service
            .submit_asset_generate(
                req,
                kp,
                artifacts.clone(),
                image_gen.clone(),
                Arc::new(SimpleBgRemover::default()),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);

        let diagnostics = td.path().join(".ats/diagnostics").join(&id.0);
        let cs = td.path().join("Generated/AlphaCard.cs");
        assert!(cs.exists(), "cs missing");
        assert!(!artifacts.join("AlphaCard/evidence.md").exists());
        assert!(!artifacts.join("AlphaCard/raw.md").exists());
        assert!(
            !diagnostics.exists(),
            "successful run diagnostics should be cleaned"
        );
        assert!(
            td.path()
                .join("DemoMod/images/card_portraits/alpha_card.png")
                .exists(),
            "runtime card image missing"
        );
        assert!(
            td.path()
                .join("DemoMod/images/card_portraits/big/alpha_card.png")
                .exists(),
            "runtime big card image missing"
        );
        assert!(
            td.path()
                .join("DemoMod/localization/eng/cards.json")
                .exists()
        );
        assert!(
            td.path()
                .join("DemoMod/localization/zhs/cards.json")
                .exists()
        );

        let res = serde_json::to_value(run.result.unwrap()).unwrap();
        assert_eq!(res["entityName"], "AlphaCard");
        let manifest_path = td.path().join(res["artifactManifestRef"].as_str().unwrap());
        assert!(manifest_path.is_file());
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["producingRunId"], id.0);
        assert_eq!(manifest["schemaVersion"], 2);
        assert_eq!(manifest["imageProcessing"]["processor"], "simple");
        assert!(manifest["imageProcessing"].get("fallback").is_none());
        assert!(manifest["imageProcessing"].get("modelSha256").is_none());
        assert!(manifest["imageProcessing"]["build"]["buildId"].is_string());
        assert_eq!(manifest["files"].as_array().unwrap().len(), 8);
        assert!(
            manifest["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|file| file["role"] == "image_quality"
                    && file.get("publishedRelativePath").is_none())
        );
        assert!(manifest["evidence"].as_array().unwrap().iter().any(|item| {
            item["source"]
                .as_str()
                .is_some_and(|source| source.contains("AlphaCard.cs"))
        }));
    }

    #[tokio::test]
    async fn cancellation_waits_for_image_processing_then_removes_partial_diagnostics() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let context = fixture_game_context(td.path(), &[], &[]);
        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(Vec::new()),
        });
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Notify::new());
        let image_proc: Arc<dyn ImageProcClient> = Arc::new(BlockingImageProc {
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        });
        let service = service_with_compile_validator(Arc::clone(&repo), llm);
        let spawned = service
            .submit_asset_generate(
                make_request(td.path(), "CancelledCard", Some("draw a card")),
                context,
                artifacts,
                Arc::new(MockImageGen::new(valid_subject_png())),
                image_proc,
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        let run_id = spawned.run_id.clone();

        entered.wait().await;
        assert!(
            td.path()
                .join(format!(".ats/diagnostics/{}", run_id.0))
                .is_dir()
        );
        assert!(
            spawned
                .cancellation
                .cancel(crate::platform::domain::CancellationReason::ProjectClose)
        );
        assert_eq!(repo.get(&run_id).await.unwrap().status, RunStatus::Running);
        release.notify_waiters();
        spawned.task.await.unwrap();

        assert_eq!(
            repo.get(&run_id).await.unwrap().status,
            RunStatus::Cancelled
        );
        assert!(
            !td.path()
                .join(format!(".ats/diagnostics/{}", run_id.0))
                .exists()
        );
    }

    #[tokio::test]
    async fn checkerboard_residue_is_rejected_before_code_and_compile() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        let validator = Arc::new(CountingCompileValidator::default());
        let service = service_with_validator(
            Arc::new(FileRunRepository::new(history)),
            Arc::new(ScriptedLlm {
                events: Mutex::new(ok_code_events("NoisyRelic")),
            }),
            validator.clone(),
        );

        let id = service
            .submit_asset_generate(
                make_request_for_type(td.path(), "NoisyRelic", "relic", Some("checkerboard relic")),
                fixture_game_context(td.path(), &[], &[]),
                artifacts.clone(),
                Arc::new(MockImageGen::new(checkerboard_subject_png())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "image_proc.runtime_failed");
        assert_eq!(failure.stage, "asset_generate.quality_gate");
        assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
        let diagnostics = td.path().join(".ats/diagnostics").join(&id.0);
        assert!(diagnostics.join("NoisyRelic.png").is_file());
        assert!(diagnostics.join("NoisyRelic.rembg.png").is_file());
        assert!(diagnostics.join("image-quality.json").is_file());
        assert_eq!(
            run.failure
                .as_ref()
                .and_then(|failure| failure.diagnostic.as_ref())
                .map(|diagnostic| diagnostic.id.as_str()),
            Some(id.0.as_str())
        );
        assert!(!artifacts.join("NoisyRelic/NoisyRelic.cs").exists());
        assert!(!td.path().join("Generated/NoisyRelic.cs").exists());
    }

    #[tokio::test]
    async fn background_removal_failure_never_delivers_raw_image() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        let validator = Arc::new(CountingCompileValidator::default());
        let service = service_with_validator(
            Arc::new(FileRunRepository::new(history)),
            Arc::new(ScriptedLlm {
                events: Mutex::new(ok_code_events("InvalidImageRelic")),
            }),
            validator.clone(),
        );

        let id = service
            .submit_asset_generate(
                make_request_for_type(
                    td.path(),
                    "InvalidImageRelic",
                    "relic",
                    Some("invalid image bytes"),
                ),
                fixture_game_context(td.path(), &[], &[]),
                artifacts.clone(),
                Arc::new(MockImageGen::new(b"not-a-png".to_vec())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "image_proc.runtime_failed");
        assert_eq!(failure.stage, "asset_generate.remove_background");
        assert_eq!(failure.diagnostic.as_ref().unwrap().id, id.0);
        assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
        let diagnostics = td.path().join(".ats/diagnostics").join(&id.0);
        assert!(diagnostics.join("InvalidImageRelic.png").is_file());
        assert!(!diagnostics.join("InvalidImageRelic.rembg.png").exists());
        assert!(!td.path().join("Generated/InvalidImageRelic.cs").exists());
        assert!(
            !td.path()
                .join("DemoMod/images/relics/invalid_image_relic.png")
                .exists()
        );
    }

    #[tokio::test]
    async fn relic_runtime_images_are_role_specific() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        let service = service_with_compile_validator(
            Arc::new(FileRunRepository::new(history)),
            Arc::new(ScriptedLlm {
                events: Mutex::new(ok_code_events("RoleRelic")),
            }),
        );

        let id = service
            .submit_asset_generate(
                make_request_for_type(td.path(), "RoleRelic", "relic", Some("transparent relic")),
                fixture_game_context(td.path(), &[], &[]),
                artifacts,
                Arc::new(MockImageGen::new(valid_subject_png())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(
            run.status,
            RunStatus::Succeeded,
            "error={:?}",
            run.error_message()
        );
        let root = td.path().join("DemoMod/images/relics");
        let normal = std::fs::read(root.join("role_relic.png")).unwrap();
        let outline = std::fs::read(root.join("role_relic_outline.png")).unwrap();
        let big = std::fs::read(root.join("big/role_relic.png")).unwrap();
        assert_ne!(normal, outline);
        assert_ne!(normal, big);
        assert_ne!(outline, big);
        let normal_image = image::load_from_memory(&normal).unwrap();
        let outline_image = image::load_from_memory(&outline).unwrap();
        let big_image = image::load_from_memory(&big).unwrap();
        assert_eq!((normal_image.width(), normal_image.height()), (128, 128));
        assert_eq!((outline_image.width(), outline_image.height()), (128, 128));
        assert_eq!((big_image.width(), big_image.height()), (1024, 1024));
    }

    #[tokio::test]
    async fn semantic_regression_is_rejected_before_write_and_compile() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        let bad_csharp = r#"public sealed class BadRelic
{
    public override async Task BeforeCombatStart()
    {
        await PlayerCmd.GainEnergy(1m, Owner);
    }
}"#;
        let responses = VecDeque::from([
            code_events_with_csharp("BadRelic", bad_csharp),
            code_events_with_csharp("BadRelic", bad_csharp),
        ]);
        let llm: Arc<dyn LlmClient> = Arc::new(SequencedLlm {
            responses: Mutex::new(responses),
        });
        let validator = Arc::new(CountingCompileValidator::default());
        let service = service_with_validator(
            Arc::new(FileRunRepository::new(history)),
            llm,
            validator.clone(),
        );
        let request = make_request(td.path(), "BadRelic", None);
        let id = service
            .submit_asset_generate(
                request,
                fixture_game_context(td.path(), &[], &[]),
                artifacts.clone(),
                Arc::new(MockImageGen::new(Vec::new())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "run.input_invalid");
        assert_eq!(failure.stage, "asset_bundle.output");
        assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
        assert!(!td.path().join("Generated/BadRelic.cs").exists());
        assert!(!artifacts.join("BadRelic/BadRelic.cs").exists());
    }

    #[tokio::test]
    async fn unknown_localization_tag_is_rejected_before_write_and_compile() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        let invalid = "Gain [yellow]1[/yellow] Energy.";
        let responses = VecDeque::from([
            code_events_with_description("BadRichTextRelic", invalid),
            code_events_with_description("BadRichTextRelic", invalid),
        ]);
        let llm: Arc<dyn LlmClient> = Arc::new(SequencedLlm {
            responses: Mutex::new(responses),
        });
        let validator = Arc::new(CountingCompileValidator::default());
        let service = service_with_validator(
            Arc::new(FileRunRepository::new(history)),
            llm,
            validator.clone(),
        );
        let request = make_request_for_type(td.path(), "BadRichTextRelic", "relic", None);
        let id = service
            .submit_asset_generate(
                request,
                fixture_game_context(td.path(), &[], &[]),
                artifacts.clone(),
                Arc::new(MockImageGen::new(Vec::new())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "run.input_invalid");
        assert_eq!(failure.stage, "asset_bundle.output");
        assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
        assert!(!td.path().join("Generated/BadRichTextRelic.cs").exists());
        assert!(
            !td.path()
                .join("DemoMod/localization/eng/relics.json")
                .exists()
        );
        assert!(
            !td.path()
                .join("DemoMod/localization/zhs/relics.json")
                .exists()
        );
        assert!(!artifacts.join("BadRichTextRelic").exists());
    }

    #[tokio::test]
    async fn artifact_publish_failure_rolls_back_formal_asset_writes() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        let generated = td.path().join("Generated");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(
            generated.join("PublishRollbackCard.cs"),
            "public class PreviousVersion {}",
        )
        .unwrap();
        std::fs::create_dir_all(artifacts.join("PublishRollbackCard")).unwrap();
        std::fs::write(
            artifacts.join("PublishRollbackCard/runs"),
            b"force deterministic publish failure",
        )
        .unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history.clone()));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events("PublishRollbackCard")),
        });
        let validator = Arc::new(CountingCompileValidator::default());
        let service = service_with_validator(repo, llm, validator.clone());
        let id = service
            .submit_asset_generate(
                make_request(td.path(), "PublishRollbackCard", None),
                fixture_game_context(td.path(), &[], &[]),
                artifacts.clone(),
                Arc::new(MockImageGen::new(Vec::new())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.result.is_none());
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "artifact.path_invalid");
        assert_eq!(failure.stage, "asset_generate.publish");
        assert_eq!(validator.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            std::fs::read_to_string(generated.join("PublishRollbackCard.cs")).unwrap(),
            "public class PreviousVersion {}"
        );
        assert!(artifacts.join("PublishRollbackCard/runs").is_file());
        assert!(
            !td.path()
                .join("DemoMod/localization/eng/cards.json")
                .exists()
        );
        assert!(
            !td.path()
                .join("DemoMod/localization/zhs/cards.json")
                .exists()
        );
    }

    #[tokio::test]
    async fn image_gen_failure_marks_run_failed_without_writing_cs() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let kp = fixture_game_context(td.path(), &[], &[]);

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(vec![]),
        });
        let image_gen: Arc<dyn ImageGenClient> = Arc::new(FailingImageGen);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = service_with_compile_validator(repo, llm);

        let req = make_request(td.path(), "BetaCard", Some("a beta card art"));
        let id = service
            .submit_asset_generate(
                req,
                kp,
                artifacts.clone(),
                image_gen,
                Arc::new(SimpleBgRemover::default()),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "image.upstream_failed");
        assert_eq!(failure.stage, "asset_generate.image");
        assert!(
            !artifacts.join("BetaCard/BetaCard.cs").exists(),
            "cs should not be written when image_gen fails"
        );
    }

    #[tokio::test]
    async fn project_scope_mismatch_fails_before_image_request() {
        let td = tempfile::TempDir::new().unwrap();
        let active = td.path().join("active");
        let other = td.path().join("other");
        std::fs::create_dir_all(active.join("history")).unwrap();
        std::fs::create_dir_all(active.join("artifacts")).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        prepare_project(&active);
        prepare_project(&other);

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(active.join("history")));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events("ScopedCard")),
        });
        let mock_img = Arc::new(MockImageGen::new(b"unused".to_vec()));
        let service = service_with_compile_validator(repo, llm);
        let id = service
            .submit_asset_generate(
                make_request(&other, "ScopedCard", Some("must not run")),
                fixture_game_context(td.path(), &[], &[]),
                active.join("artifacts"),
                mock_img.clone(),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "run.input_invalid");
        assert_eq!(failure.stage, "asset_generate.project_scope");
        assert_eq!(mock_img.call_count(), 0);
    }

    #[tokio::test]
    async fn no_image_prompt_skips_image_gen() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let kp = fixture_game_context(td.path(), &[], &[]);

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events("GammaCard")),
        });
        let mock_img = Arc::new(MockImageGen::new(b"unused".to_vec()));
        let image_gen: Arc<dyn ImageGenClient> = mock_img.clone();
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = service_with_compile_validator(repo, llm);

        let req = make_request(td.path(), "GammaCard", None);
        let id = service
            .submit_asset_generate(
                req,
                kp,
                artifacts.clone(),
                image_gen,
                Arc::new(SimpleBgRemover::default()),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(mock_img.call_count(), 0, "image gen should not be called");

        let cs = td.path().join("Generated/GammaCard.cs");
        assert!(cs.exists());
        assert!(!artifacts.join("GammaCard/GammaCard.cs").exists());
        let png = artifacts.join("GammaCard/GammaCard.png");
        assert!(!png.exists(), "png should not be written");

        let res = serde_json::to_value(run.result.unwrap()).unwrap();
        assert!(
            td.path()
                .join(res["artifactManifestRef"].as_str().unwrap())
                .is_file()
        );
    }

    #[tokio::test]
    async fn empty_image_prompt_skips_image_gen() {
        // image_prompt = Some("   ") 应被视为"无图像需求"
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let kp = fixture_game_context(td.path(), &[], &[]);

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events("DeltaCard")),
        });
        let mock_img = Arc::new(MockImageGen::new(b"unused".to_vec()));
        let image_gen: Arc<dyn ImageGenClient> = mock_img.clone();
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = service_with_compile_validator(repo, llm);

        let req = make_request(td.path(), "DeltaCard", Some("   \n  "));
        let id = service
            .submit_asset_generate(
                req,
                kp,
                artifacts,
                image_gen,
                Arc::new(SimpleBgRemover::default()),
                sink,
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(mock_img.call_count(), 0);
    }

    #[tokio::test]
    async fn empty_model_output_retries_once_then_succeeds() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let empty = vec![
            Ok(StreamEvent::Start {
                model: "test-model".into(),
            }),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::EndTurn,
                usage: Usage::default(),
            }),
        ];
        let llm: Arc<dyn LlmClient> = Arc::new(SequencedLlm {
            responses: Mutex::new(VecDeque::from([empty, ok_code_events("RetryCard")])),
        });
        let service = service_with_compile_validator(repo, llm);
        let id = service
            .submit_asset_generate(
                make_request(td.path(), "RetryCard", None),
                fixture_game_context(td.path(), &[], &[]),
                artifacts,
                Arc::new(MockImageGen::new(Vec::new())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        assert!(td.path().join("Generated/RetryCard.cs").is_file());
    }

    #[tokio::test]
    async fn compile_failure_rolls_back_generated_files_and_localization() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        let artifacts = td.path().join("artifacts");
        let generated = td.path().join("Generated");
        let eng_dir = td.path().join("DemoMod/localization/eng");
        std::fs::create_dir_all(&history).unwrap();
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::create_dir_all(&eng_dir).unwrap();
        std::fs::write(
            generated.join("RollbackCard.cs"),
            "public class OldVersion {}",
        )
        .unwrap();
        std::fs::write(
            eng_dir.join("cards.json"),
            r#"{"EXISTING.title":"Existing"}"#,
        )
        .unwrap();

        let repo: Arc<dyn RunRepository> = Arc::new(FileRunRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events("RollbackCard")),
        });
        let service = service_with_validator(repo, llm, Arc::new(FailingCompileValidator));
        let id = service
            .submit_asset_generate(
                make_request(td.path(), "RollbackCard", Some("rollback card image")),
                fixture_game_context(td.path(), &[], &[]),
                artifacts.clone(),
                Arc::new(MockImageGen::new(valid_subject_png())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let run = service.get(&id).await.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        let failure = run.failure.as_ref().unwrap();
        assert_eq!(failure.code, "core.unclassified");
        assert_eq!(failure.stage, "asset_bundle.compile");
        assert_eq!(
            std::fs::read_to_string(generated.join("RollbackCard.cs")).unwrap(),
            "public class OldVersion {}"
        );
        assert_eq!(
            std::fs::read_to_string(eng_dir.join("cards.json")).unwrap(),
            r#"{"EXISTING.title":"Existing"}"#
        );
        assert!(!artifacts.join("RollbackCard/runs").exists());
        assert!(
            !td.path()
                .join("DemoMod/localization/zhs/cards.json")
                .exists()
        );
        assert!(
            !td.path()
                .join("DemoMod/images/card_portraits/rollback_card.png")
                .exists(),
            "new runtime image should be rolled back"
        );
        assert!(
            td.path()
                .join(".ats/diagnostics")
                .join(&id.0)
                .join("RollbackCard.png")
                .is_file(),
            "raw image artifact should remain for diagnosis"
        );
    }
}
