//! asset_generate handler：先 image_gen 出图，再 code_generate 出 .cs。
//!
//! 决策流：
//! 1. transition_to_running
//! 2. 如 image_prompt 非空 → 调 ImageGenClient → 背景去除 → 质量门禁
//!    → 写诊断原图、处理图和质量报告 → 把正式资源路径塞回 image_paths
//! 3. 用 PromptAssembler.assemble_asset_prompt 装 prompt
//! 4. 生成结构化 C# + 双语本地化 bundle，并通过隔离编译门禁
//! 5. 门禁通过后落 job.result；失败时回滚正式生成目录
//!
//! image_gen 失败 → 整任务 Failed（不继续 code）。
//! image_prompt 留空 → 跳过 image_gen，仅跑 code_generate（与 code_generate(asset)
//! 等价但保留原 image_paths）。

use std::path::PathBuf;
use std::sync::Arc;

use tokio::fs;

use super::asset_bundle::{AssetBundleGeneration, runtime_image_paths_for, validate_project_scope};
use super::asset_compile::AssetCompileValidator;
use super::code_generate::{finalize_asset_bundle_error, sanitize_entity_name};
use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, finalize_with_error, finalize_with_success,
    transition_to_running,
};
use crate::codegen::{AssetKind, PromptAssembler};
use crate::image_gen::{ImageGenClient, ImageGenRequest};
use crate::image_proc::{
    ImageProcClient, ImageProcError, ImageQualityReport, ImageQualitySpec, analyze_png_quality,
};
use crate::knowledge::{KnowledgePaths, runtime::detect_source_mode};
use crate::llm::LlmClient;
use crate::platform::contracts::SubmitAssetGenerateRequest;
use crate::platform::domain::{JobId, JobRepository};

#[allow(clippy::too_many_arguments)] // handler 注入 job 与外部 adapter，避免为单一调用引入浅 Context 结构体
pub(crate) async fn run_asset_generate(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    image_gen: Arc<dyn ImageGenClient>,
    image_proc: Arc<dyn ImageProcClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitAssetGenerateRequest,
    knowledge_paths: KnowledgePaths,
    artifacts_dir: PathBuf,
    compile_validator: Arc<dyn AssetCompileValidator>,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    let mut asset_request = request.asset_request.clone();
    if let Err(err) = validate_project_scope(&asset_request.project_root, &artifacts_dir) {
        finalize_with_error(
            &repo,
            &job_id,
            &sink,
            &format!("invalid asset project scope: {err}"),
        )
        .await;
        return;
    }
    if AssetKind::parse(&asset_request.asset_type).is_none() {
        finalize_with_error(
            &repo,
            &job_id,
            &sink,
            &format!("unsupported asset_type: {}", asset_request.asset_type),
        )
        .await;
        return;
    }
    let entity_name = sanitize_entity_name(&asset_request.asset_name);
    let target_dir = artifacts_dir.join(&entity_name);

    // 1. 出图（若提供 image_prompt）
    let mut png_path: Option<PathBuf> = None;
    let mut image_model: Option<String> = None;
    let mut revised_prompt: Option<String> = None;
    let mut runtime_image_source: Option<PathBuf> = None;
    let mut image_quality_path: Option<PathBuf> = None;
    let mut image_quality_report: Option<ImageQualityReport> = None;

    if let Some(prompt) = &request.image_prompt {
        let prompt = prompt.trim();
        if !prompt.is_empty() {
            sink.emit(ProgressEvent {
                job_id: job_id.clone(),
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
            let img_resp = match image_gen.generate(img_req).await {
                Ok(r) => r,
                Err(err) => {
                    finalize_with_error(&repo, &job_id, &sink, &format!("image_gen: {err}")).await;
                    return;
                }
            };
            let first = match img_resp.images.first() {
                Some(i) => i,
                None => {
                    finalize_with_error(&repo, &job_id, &sink, "image_gen returned no image").await;
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
                finalize_with_error(&repo, &job_id, &sink, &format!("create target dir: {err}"))
                    .await;
                return;
            }
            if let Err(err) = fs::write(&path, &first.bytes).await {
                finalize_with_error(&repo, &job_id, &sink, &format!("write image: {err}")).await;
                return;
            }
            image_model = Some(img_resp.model.clone());
            revised_prompt = img_resp.revised_prompt.clone();

            // 背景去除：调注入的 ImageProcClient（生产路径是 BgRemoverChain
            // ML→Simple 回退）。处理失败或质量门禁失败时保留诊断文件，但不交付原图。
            let raw_bytes = first.bytes.clone();
            let rembg_path = target_dir.join(format!("{entity_name}.rembg.png"));
            match image_proc.remove_background(&raw_bytes).await {
                Ok(processed) => {
                    if let Err(err) = crate::fs_atomic::write_atomic(&rembg_path, &processed).await
                    {
                        finalize_with_error(
                            &repo,
                            &job_id,
                            &sink,
                            &format!(
                                "write processed image: {err}; raw diagnostic kept at {}",
                                path.display()
                            ),
                        )
                        .await;
                        return;
                    }

                    let report = match analyze_png_quality(&processed, ImageQualitySpec::default())
                    {
                        Ok(report) => report,
                        Err(err) => {
                            finalize_with_error(
                                &repo,
                                &job_id,
                                &sink,
                                &format!(
                                    "analyze processed image quality: {err}; diagnostics kept at {} and {}",
                                    path.display(),
                                    rembg_path.display()
                                ),
                            )
                            .await;
                            return;
                        }
                    };
                    let report_path = target_dir.join("image-quality.json");
                    let report_bytes = match serde_json::to_vec_pretty(&report) {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            finalize_with_error(
                                &repo,
                                &job_id,
                                &sink,
                                &format!("serialize image quality report: {err}"),
                            )
                            .await;
                            return;
                        }
                    };
                    if let Err(err) =
                        crate::fs_atomic::write_atomic(&report_path, &report_bytes).await
                    {
                        finalize_with_error(
                            &repo,
                            &job_id,
                            &sink,
                            &format!("write image quality report: {err}"),
                        )
                        .await;
                        return;
                    }
                    if !report.accepted {
                        let quality_error =
                            ImageProcError::Quality(report.rejection_summary()).to_string();
                        finalize_with_error(
                            &repo,
                            &job_id,
                            &sink,
                            &format!(
                                "{quality_error}; diagnostics kept at {}, {}, and {}",
                                path.display(),
                                rembg_path.display(),
                                report_path.display()
                            ),
                        )
                        .await;
                        return;
                    }
                    runtime_image_source = Some(rembg_path.clone());
                    image_quality_path = Some(report_path);
                    image_quality_report = Some(report);
                }
                Err(err) => {
                    finalize_with_error(
                        &repo,
                        &job_id,
                        &sink,
                        &format!(
                            "background removal failed: {err}; raw diagnostic kept at {}; no runtime image delivered",
                            path.display()
                        ),
                    )
                    .await;
                    return;
                }
            }
            png_path = Some(path);

            asset_request.image_paths = match runtime_image_paths_for(&asset_request) {
                Ok(paths) => paths,
                Err(err) => {
                    finalize_with_error(
                        &repo,
                        &job_id,
                        &sink,
                        &format!("plan runtime image delivery: {err}"),
                    )
                    .await;
                    return;
                }
            };

            sink.emit(ProgressEvent {
                job_id: job_id.clone(),
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
    let prompt_assembly = match assembler.assemble_asset_prompt_with_evidence(
        &asset_request,
        &knowledge_paths,
        detect_source_mode(&knowledge_paths),
    ) {
        Ok(assembly) => assembly,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &sink, &format!("prompt assembly: {err}")).await;
            return;
        }
    };

    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "code-gen-start".into(),
        percent: Some(0.45),
        message: Some(format!("LLM prompt {} chars", prompt_assembly.prompt.len())),
        delta: None,
    })
    .await;

    // 3. 生成结构化 bundle，写入后执行隔离编译门禁
    let generator = AssetBundleGeneration::new(
        Arc::clone(&repo),
        Arc::clone(&llm),
        compile_validator,
        Arc::clone(&sink),
    );
    let artifact = match generator
        .generate(
            &job_id,
            prompt_assembly.prompt,
            &prompt_assembly.evidence_record,
            &asset_request,
            &artifacts_dir,
            runtime_image_source.as_deref(),
        )
        .await
    {
        Ok(artifact) => artifact,
        Err(err) => {
            finalize_asset_bundle_error(&repo, &job_id, &sink, err).await;
            return;
        }
    };

    // 4. 收口
    let result = serde_json::json!({
        "entityName": artifact.entity_name,
        "csPath": artifact.cs_path.display().to_string(),
        "artifactCsPath": artifact.artifact_cs_path.display().to_string(),
        "rawPath": artifact.raw_path.display().to_string(),
        "evidencePath": artifact.evidence_path.display().to_string(),
        "localizationPaths": artifact.localization_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "runtimeImagePaths": artifact.runtime_image_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "pngPath": png_path.as_ref().map(|p| p.display().to_string()),
        "imageQualityPath": image_quality_path.as_ref().map(|p| p.display().to_string()),
        "imageQuality": image_quality_report,
        "imageModel": image_model,
        "revisedPrompt": revised_prompt,
        "codeModel": artifact.model,
        "extractedChars": artifact.extracted_chars,
        "rawChars": artifact.raw_chars,
        "usage": { "inputTokens": artifact.usage_in, "outputTokens": artifact.usage_out },
        "compileGate": {
            "exitCode": artifact.compile.exit_code,
            "stdoutTail": artifact.compile.stdout_tail,
            "stderrTail": artifact.compile.stderr_tail,
        },
    });
    if !matches!(
        finalize_with_success(&repo, &job_id, result).await,
        FinalizeOutcome::Completed
    ) {
        return;
    }

    sink.emit(ProgressEvent {
        job_id,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::AssetCodegenRequest;
    use crate::image_gen::{GeneratedImage, ImageGenError, ImageGenResponse};
    use crate::image_proc::SimpleBgRemover;
    use crate::llm::{
        CompletionRequest, CompletionResponse, CompletionStream, FinishReason, LlmError,
        StreamEvent, Usage,
    };
    use crate::platform::application::JobApplicationService;
    use crate::platform::application::handlers::asset_compile::{
        AssetCompileValidator, CompileValidation,
    };
    use crate::platform::domain::JobRepository;
    use crate::platform::domain::JobStatus;
    use crate::platform::infra::FileJobRepository;
    use async_trait::async_trait;
    use futures_util::stream;
    use image::{ImageFormat, Rgba, RgbaImage};
    use std::collections::VecDeque;
    use std::io::Cursor;
    use std::path::Path;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

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
            _job_id: &JobId,
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
            _job_id: &JobId,
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
            _job_id: &JobId,
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
            r#"{"name":"demo","csharp_name":"DemoMod","scaffolded":true}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("MainFile.cs"),
            "namespace DemoMod; public class MainFile {}",
        )
        .unwrap();
    }

    fn service_with_compile_validator(
        repo: Arc<dyn JobRepository>,
        llm: Arc<dyn LlmClient>,
    ) -> JobApplicationService {
        JobApplicationService::new(repo, llm)
            .with_asset_compile_validator(Arc::new(PassingCompileValidator))
    }

    fn service_with_validator(
        repo: Arc<dyn JobRepository>,
        llm: Arc<dyn LlmClient>,
        validator: Arc<dyn AssetCompileValidator>,
    ) -> JobApplicationService {
        JobApplicationService::new(repo, llm).with_asset_compile_validator(validator)
    }

    async fn wait_terminal(service: &JobApplicationService, id: &JobId) {
        for _ in 0..200 {
            let job = service.get(id).await.unwrap();
            if job.status.is_terminal() {
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
        let kp = KnowledgePaths::from_runtime_dir(td.path());

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
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

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);

        let png = artifacts.join("AlphaCard/AlphaCard.png");
        let artifact_cs = artifacts.join("AlphaCard/AlphaCard.cs");
        let evidence = artifacts.join("AlphaCard/evidence.md");
        let quality = artifacts.join("AlphaCard/image-quality.json");
        let cs = td.path().join("Generated/AlphaCard.cs");
        assert!(png.exists(), "png missing");
        assert!(cs.exists(), "cs missing");
        assert!(artifact_cs.exists(), "artifact cs missing");
        assert!(evidence.exists(), "evidence record missing");
        assert!(quality.exists(), "image quality report missing");
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

        let res = job.result.unwrap();
        assert_eq!(res["entityName"], "AlphaCard");
        assert!(
            res["pngPath"].as_str().unwrap_or("").ends_with(".png"),
            "pngPath should be set: {:?}",
            res["pngPath"]
        );
        assert_eq!(res["imageModel"], "mock-image-model");
        assert_eq!(res["revisedPrompt"], "a revised prompt");
        assert_eq!(res["runtimeImagePaths"].as_array().unwrap().len(), 2);
        assert_eq!(res["imageQuality"]["accepted"], true);
        assert_eq!(
            PathBuf::from(res["imageQualityPath"].as_str().unwrap())
                .canonicalize()
                .unwrap(),
            quality.canonicalize().unwrap()
        );
        let evidence_result = PathBuf::from(res["evidencePath"].as_str().unwrap());
        assert_eq!(
            evidence_result.canonicalize().unwrap(),
            evidence.canonicalize().unwrap()
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
            Arc::new(FileJobRepository::new(history)),
            Arc::new(ScriptedLlm {
                events: Mutex::new(ok_code_events("NoisyRelic")),
            }),
            validator.clone(),
        );

        let id = service
            .submit_asset_generate(
                make_request_for_type(td.path(), "NoisyRelic", "relic", Some("checkerboard relic")),
                KnowledgePaths::from_runtime_dir(td.path()),
                artifacts.clone(),
                Arc::new(MockImageGen::new(checkerboard_subject_png())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error
                .unwrap_or_default()
                .contains("likely_background_residue")
        );
        assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
        assert!(artifacts.join("NoisyRelic/NoisyRelic.png").is_file());
        assert!(artifacts.join("NoisyRelic/NoisyRelic.rembg.png").is_file());
        assert!(artifacts.join("NoisyRelic/image-quality.json").is_file());
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
            Arc::new(FileJobRepository::new(history)),
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
                KnowledgePaths::from_runtime_dir(td.path()),
                artifacts.clone(),
                Arc::new(MockImageGen::new(b"not-a-png".to_vec())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error
                .unwrap_or_default()
                .contains("no runtime image delivered")
        );
        assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
        assert!(
            artifacts
                .join("InvalidImageRelic/InvalidImageRelic.png")
                .is_file()
        );
        assert!(
            !artifacts
                .join("InvalidImageRelic/InvalidImageRelic.rembg.png")
                .exists()
        );
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
            Arc::new(FileJobRepository::new(history)),
            Arc::new(ScriptedLlm {
                events: Mutex::new(ok_code_events("RoleRelic")),
            }),
        );

        let id = service
            .submit_asset_generate(
                make_request_for_type(td.path(), "RoleRelic", "relic", Some("transparent relic")),
                KnowledgePaths::from_runtime_dir(td.path()),
                artifacts,
                Arc::new(MockImageGen::new(valid_subject_png())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed, "error={:?}", job.error);
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
            Arc::new(FileJobRepository::new(history)),
            llm,
            validator.clone(),
        );
        let request = make_request(td.path(), "BadRelic", None);
        let id = service
            .submit_asset_generate(
                request,
                KnowledgePaths::from_runtime_dir(td.path()),
                artifacts.clone(),
                Arc::new(MockImageGen::new(Vec::new())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.unwrap_or_default().contains("ResetEnergy"));
        assert_eq!(validator.calls.load(Ordering::SeqCst), 0);
        assert!(!td.path().join("Generated/BadRelic.cs").exists());
        assert!(!artifacts.join("BadRelic/BadRelic.cs").exists());
    }

    #[tokio::test]
    async fn image_gen_failure_marks_job_failed_without_writing_cs() {
        let td = tempfile::TempDir::new().unwrap();
        prepare_project(td.path());
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        let artifacts = td.path().join("artifacts");
        std::fs::create_dir_all(&artifacts).unwrap();
        let kp = KnowledgePaths::from_runtime_dir(td.path());

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
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

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error.unwrap_or_default().contains("image_gen"),
            "error should mention image_gen"
        );
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

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(active.join("history")));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events("ScopedCard")),
        });
        let mock_img = Arc::new(MockImageGen::new(b"unused".to_vec()));
        let service = service_with_compile_validator(repo, llm);
        let id = service
            .submit_asset_generate(
                make_request(&other, "ScopedCard", Some("must not run")),
                KnowledgePaths::from_runtime_dir(td.path()),
                active.join("artifacts"),
                mock_img.clone(),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.unwrap_or_default().contains("project scope"));
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
        let kp = KnowledgePaths::from_runtime_dir(td.path());

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
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

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        assert_eq!(mock_img.call_count(), 0, "image gen should not be called");

        let cs = td.path().join("Generated/GammaCard.cs");
        assert!(cs.exists());
        assert!(artifacts.join("GammaCard/GammaCard.cs").exists());
        let png = artifacts.join("GammaCard/GammaCard.png");
        assert!(!png.exists(), "png should not be written");

        let res = job.result.unwrap();
        assert!(res["pngPath"].is_null());
        assert!(res["imageModel"].is_null());
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
        let kp = KnowledgePaths::from_runtime_dir(td.path());

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
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

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
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
        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
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
                KnowledgePaths::from_runtime_dir(td.path()),
                artifacts,
                Arc::new(MockImageGen::new(Vec::new())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
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

        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(history));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(ok_code_events("RollbackCard")),
        });
        let service = service_with_validator(repo, llm, Arc::new(FailingCompileValidator));
        let id = service
            .submit_asset_generate(
                make_request(td.path(), "RollbackCard", Some("rollback card image")),
                KnowledgePaths::from_runtime_dir(td.path()),
                artifacts.clone(),
                Arc::new(MockImageGen::new(valid_subject_png())),
                Arc::new(SimpleBgRemover::default()),
                Arc::new(super::super::common::NoopProgressSink),
            )
            .await
            .unwrap();
        wait_terminal(&service, &id).await;
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error
                .unwrap_or_default()
                .contains("simulated compile failure")
        );
        assert_eq!(
            std::fs::read_to_string(generated.join("RollbackCard.cs")).unwrap(),
            "public class OldVersion {}"
        );
        assert_eq!(
            std::fs::read_to_string(eng_dir.join("cards.json")).unwrap(),
            r#"{"EXISTING.title":"Existing"}"#
        );
        assert!(
            artifacts.join("RollbackCard/RollbackCard.cs").is_file(),
            "raw artifact should remain for diagnosis"
        );
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
            artifacts.join("RollbackCard/RollbackCard.png").is_file(),
            "raw image artifact should remain for diagnosis"
        );
    }
}
