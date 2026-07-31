//! code_generate handler：asset 走结构化 bundle + compile gate，custom_code 走单 C# fence。
//!
//! `generate_and_write_code_artifact` 保留给 custom_code 和 batch_custom_code 复用；
//! asset 的多文件事务由 `asset_bundle` 模块统一处理。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::asset_bundle::{
    AssetBundleError, AssetBundleGeneration, ProjectFileTransaction, validate_project_scope,
};
use super::asset_compile::AssetCompileValidator;
use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, emit_cancelled_mid_stream, finalize_with_failure,
    finalize_with_success, is_cancelled, transition_to_running,
};
use crate::codegen::{GenerationEvidence, PromptAssembler};
use crate::failure::{ActionableFailure, FailureDiagnostic, FailureNormalizer};
use crate::game_pack::{ValidationRule, VerifiedGameContext};
use crate::llm::{CompletionRequest, LlmClient, LlmError, Message, MessageRole, StreamEvent};
use crate::platform::artifact::{
    ArtifactFileInput, ArtifactGameContext, ArtifactGeneration, ArtifactPublishRequest,
    ArtifactStore, LegacyArtifactCleanup, sha256_bytes, snapshot_evidence,
};
use crate::platform::contracts::SubmitCodeGenerateRequest;
use crate::platform::domain::{RunId, RunRepository, RunResult, TokenUsage};
use futures_util::StreamExt;

#[allow(clippy::too_many_arguments)] // handler 直接接收 run 依赖与 compile adapter，保持 service 注入方式一致
pub(crate) async fn run_code_generate(
    repo: Arc<dyn RunRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    run_id: RunId,
    request: SubmitCodeGenerateRequest,
    game_context: VerifiedGameContext,
    artifacts_dir: PathBuf,
    compile_validator: Arc<dyn AssetCompileValidator>,
) {
    if transition_to_running(&repo, &run_id, &sink).await.is_err() {
        return;
    }

    if let SubmitCodeGenerateRequest::Asset { request: asset } = &request {
        if validate_project_scope(&asset.project_root, &artifacts_dir).is_err() {
            finalize_with_failure(
                &repo,
                &run_id,
                &sink,
                ActionableFailure::invalid_input(
                    "code_generate.project_scope",
                    "The asset project path is outside the active project.",
                ),
            )
            .await;
            return;
        }
        if game_context
            .pack()
            .resource_spec(&asset.asset_type)
            .is_none()
        {
            finalize_with_failure(
                &repo,
                &run_id,
                &sink,
                ActionableFailure::invalid_input(
                    "code_generate.asset_type",
                    "The selected Game Pack does not support this asset type.",
                ),
            )
            .await;
            return;
        }
    }

    let assembler = PromptAssembler::built_in();
    let prompt_result = match &request {
        SubmitCodeGenerateRequest::Asset { request: req } => assembler
            .assemble_asset_prompt_with_evidence(req, &game_context)
            .map(|assembly| (assembly.prompt, assembly.evidence)),
        SubmitCodeGenerateRequest::CustomCode { request: req } => assembler
            .assemble_custom_code_prompt_with_evidence(req, &game_context)
            .map(|assembly| (assembly.prompt, assembly.evidence)),
    };
    let (prompt, evidence) = match prompt_result {
        Ok(value) => value,
        Err(_) => {
            finalize_with_failure(
                &repo,
                &run_id,
                &sink,
                ActionableFailure::unclassified("code_generate.prompt"),
            )
            .await;
            return;
        }
    };
    let inputs_sha256 = sha256_bytes(prompt.as_bytes());
    let output_path = match &request {
        SubmitCodeGenerateRequest::Asset {
            request: asset_request,
        } => {
            let generator = AssetBundleGeneration::new(
                Arc::clone(&repo),
                Arc::clone(&llm),
                compile_validator,
                Arc::clone(&sink),
            );
            let mut artifact = match generator
                .generate(&run_id, prompt, game_context.pack(), asset_request, None)
                .await
            {
                Ok(artifact) => artifact,
                Err(err) => {
                    finalize_asset_bundle_error(&repo, &run_id, &sink, err, None).await;
                    return;
                }
            };
            let output_path = artifact.cs_path.clone();
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
            let published = match publish_generated_artifact(
                &artifacts_dir,
                &run_id,
                &game_context,
                &asset_request.asset_type,
                artifact.entity_name.clone(),
                artifact.model.clone(),
                inputs_sha256.clone(),
                TokenUsage {
                    input_tokens: artifact.usage_in,
                    output_tokens: artifact.usage_out,
                },
                evidence,
                files,
                Vec::new(),
            )
            .await
            {
                Ok(published) => published,
                Err(_) => {
                    let rollback = artifact.rollback_writes().await;
                    let _ = rollback;
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        ActionableFailure::unclassified("code_generate.publish"),
                    )
                    .await;
                    return;
                }
            };
            let result = published.result.clone();
            if !matches!(
                finalize_with_success(&repo, &run_id, result.clone()).await,
                FinalizeOutcome::Succeeded
            ) {
                let artifact_rollback = published.rollback().await;
                let file_rollback = artifact.rollback_writes().await;
                let _ = (artifact_rollback, file_rollback);
                return;
            }
            let _ = published.commit().await;
            artifact.commit_writes();
            output_path
        }
        SubmitCodeGenerateRequest::CustomCode { .. } => {
            let entity_name = code_generate_entity_name(&request);
            let mut artifact = match generate_and_write_code_artifact(
                Arc::clone(&repo),
                Arc::clone(&llm),
                Arc::clone(&sink),
                &run_id,
                prompt,
                &entity_name,
                &artifacts_dir,
                &game_context.pack().validation_rules,
            )
            .await
            {
                Ok(artifact) => artifact,
                Err(GenerateError::Stream(err)) => {
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        FailureNormalizer::llm("code_generate.stream", &err),
                    )
                    .await;
                    return;
                }
                Err(GenerateError::ModelOutput) => {
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        ActionableFailure::invalid_input(
                            "code_generate.output",
                            "The model returned invalid code. Retry generation.",
                        ),
                    )
                    .await;
                    return;
                }
                Err(GenerateError::Write) => {
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        ActionableFailure::unclassified("code_generate.write"),
                    )
                    .await;
                    return;
                }
                Err(GenerateError::Cancelled) => return,
            };
            let output_path = artifact.cs_path.clone();
            let published = match publish_generated_artifact(
                &artifacts_dir,
                &run_id,
                &game_context,
                "custom_code",
                artifact.entity_name.clone(),
                artifact.model.clone(),
                inputs_sha256,
                TokenUsage {
                    input_tokens: artifact.usage_in,
                    output_tokens: artifact.usage_out,
                },
                evidence,
                vec![("csharp".into(), artifact.cs_path.clone())],
                Vec::new(),
            )
            .await
            {
                Ok(published) => published,
                Err(_) => {
                    let rollback = artifact.rollback_writes().await;
                    let _ = rollback;
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        ActionableFailure::unclassified("code_generate.publish"),
                    )
                    .await;
                    return;
                }
            };
            let result = published.result.clone();
            if !matches!(
                finalize_with_success(&repo, &run_id, result.clone()).await,
                FinalizeOutcome::Succeeded
            ) {
                let artifact_rollback = published.rollback().await;
                let file_rollback = artifact.rollback_writes().await;
                let _ = (artifact_rollback, file_rollback);
                return;
            }
            let _ = published.commit().await;
            artifact.commit_writes();
            output_path
        }
    };
    sink.emit(ProgressEvent {
        run_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!("wrote {}", output_path.display())),
        delta: None,
    })
    .await;
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_generated_artifact(
    artifacts_dir: &Path,
    run_id: &RunId,
    game_context: &VerifiedGameContext,
    artifact_kind: &str,
    entity_name: String,
    model: String,
    inputs_sha256: String,
    usage: TokenUsage,
    evidence: Vec<GenerationEvidence>,
    files: Vec<(String, PathBuf)>,
    snapshot_only_files: Vec<(String, PathBuf)>,
) -> Result<PublishedRunArtifact, String> {
    let project_root = artifacts_dir
        .parent()
        .ok_or_else(|| {
            format!(
                "artifacts dir has no project parent: {}",
                artifacts_dir.display()
            )
        })?
        .to_path_buf();
    let store = ArtifactStore::new(project_root);
    let mut file_inputs = Vec::with_capacity(files.len());
    for (role, source_path) in files {
        let published_relative_path = store
            .project_relative_ref(&source_path)
            .map_err(|error| error.to_string())?;
        file_inputs.push(ArtifactFileInput {
            role,
            source_path,
            published_relative_path: Some(published_relative_path),
        });
    }
    file_inputs.extend(snapshot_only_files.into_iter().map(|(role, source_path)| {
        ArtifactFileInput {
            role,
            source_path,
            published_relative_path: None,
        }
    }));
    let mut artifact_evidence = snapshot_evidence(game_context);
    artifact_evidence.extend(evidence.into_iter().map(Into::into));
    let published = store
        .publish_async(ArtifactPublishRequest {
            artifact_id: entity_name.clone(),
            artifact_kind: artifact_kind.to_string(),
            run_id: run_id.clone(),
            game_context: ArtifactGameContext::from(game_context),
            evidence: artifact_evidence,
            generation: ArtifactGeneration {
                provider: "configured_llm".into(),
                model: model.clone(),
                inputs_sha256,
            },
            image_processing: None,
            files: file_inputs,
        })
        .await
        .map_err(|error| format!("publish artifact manifest: {error}"))?;
    let legacy_cleanup = match store.begin_legacy_cleanup(&entity_name, run_id) {
        Ok(cleanup) => cleanup,
        Err(error) => {
            let _ = store.remove_published_run(&entity_name, run_id);
            return Err(format!("prepare legacy artifact cleanup: {error}"));
        }
    };
    Ok(PublishedRunArtifact {
        result: RunResult::ArtifactProduction {
            artifact_manifest_ref: published.artifact_manifest_ref,
            manifest_sha256: published.manifest_sha256,
            artifact_id: entity_name.clone(),
            entity_name: entity_name.clone(),
            model: Some(model),
            usage: Some(usage),
        },
        store,
        artifact_id: entity_name,
        run_id: run_id.clone(),
        legacy_cleanup: Some(legacy_cleanup),
    })
}

pub(crate) struct PublishedRunArtifact {
    pub(crate) result: RunResult,
    store: ArtifactStore,
    artifact_id: String,
    run_id: RunId,
    legacy_cleanup: Option<LegacyArtifactCleanup>,
}

impl PublishedRunArtifact {
    pub(crate) async fn rollback(mut self) -> Result<(), String> {
        let store = self.store.clone();
        let artifact_id = self.artifact_id.clone();
        let run_id = self.run_id.clone();
        let cleanup = self.legacy_cleanup.take();
        tokio::task::spawn_blocking(move || {
            if let Some(cleanup) = cleanup {
                cleanup.rollback()?;
            }
            store.remove_published_run(&artifact_id, &run_id)
        })
        .await
        .map_err(|error| format!("artifact rollback task failed: {error}"))?
        .map_err(|error| format!("remove published artifact: {error}"))
    }

    pub(crate) async fn commit(mut self) -> Result<(), String> {
        let cleanup = self.legacy_cleanup.take();
        tokio::task::spawn_blocking(move || match cleanup {
            Some(cleanup) => cleanup.commit(),
            None => Ok(()),
        })
        .await
        .map_err(|error| format!("artifact cleanup task failed: {error}"))?
        .map_err(|error| format!("commit legacy artifact cleanup: {error}"))
    }
}

pub(crate) async fn finalize_asset_bundle_error(
    repo: &Arc<dyn RunRepository>,
    run_id: &RunId,
    sink: &Arc<dyn ProgressSink>,
    error: AssetBundleError,
    diagnostic_ref: Option<String>,
) {
    let mut failure = match error {
        AssetBundleError::Stream(err) => FailureNormalizer::llm("asset_bundle.stream", &err),
        AssetBundleError::ModelOutput => ActionableFailure::invalid_input(
            "asset_bundle.output",
            "The model returned an invalid asset bundle. Retry generation.",
        ),
        AssetBundleError::Write => ActionableFailure::unclassified("asset_bundle.write"),
        AssetBundleError::Compile => ActionableFailure::unclassified("asset_bundle.compile"),
        AssetBundleError::Cancelled => return,
    };
    if diagnostic_ref.is_some() {
        failure = failure.with_diagnostic(FailureDiagnostic::for_run(
            run_id,
            "Run diagnostics are available for this failed execution.",
        ));
    }
    finalize_with_failure(repo, run_id, sink, failure).await;
}

/// 从 SubmitCodeGenerateRequest 提取一个文件名安全的实体名。非 ASCII 字母数字/下划线
/// 一律替换为下划线；全空兜底为 "Unnamed"。
pub(crate) fn code_generate_entity_name(req: &SubmitCodeGenerateRequest) -> String {
    let raw = match req {
        SubmitCodeGenerateRequest::Asset { request } => request.asset_name.clone(),
        SubmitCodeGenerateRequest::CustomCode { request } => request.name.clone(),
    };
    sanitize_entity_name(&raw)
}

pub(crate) fn sanitize_entity_name(raw: &str) -> String {
    let safe: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() {
        "Unnamed".into()
    } else {
        safe
    }
}

/// 从 markdown 文本中提取第一对 ``` fence 之间的内容。
/// 跳过 fence 起始行（含可选语言标签如 ```csharp），保留中间所有行。
/// 找不到 fence 时返回 None；调用方决定回退策略。
pub(crate) fn extract_first_code_block(text: &str) -> Option<String> {
    let mut in_block = false;
    let mut block_lines: Vec<&str> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            if in_block {
                return Some(block_lines.join("\n"));
            }
            in_block = true;
            continue;
        }
        if in_block {
            block_lines.push(line);
        }
    }
    None
}

pub(crate) struct WrittenArtifact {
    pub model: String,
    pub entity_name: String,
    pub cs_path: PathBuf,
    pub usage_in: u32,
    pub usage_out: u32,
    transaction: Option<ProjectFileTransaction>,
}

impl WrittenArtifact {
    pub(crate) fn commit_writes(&mut self) {
        if let Some(transaction) = self.transaction.take() {
            transaction.commit();
        }
    }

    pub(crate) async fn rollback_writes(&mut self) -> Result<(), String> {
        match self.transaction.take() {
            Some(transaction) => transaction.rollback().await,
            None => Ok(()),
        }
    }
}

pub(crate) enum GenerateError {
    Stream(LlmError),
    ModelOutput,
    Write,
    /// 流被取消（run.status=Cancelled）；调用方应当不写 result。
    Cancelled,
}

/// 兜底校验：LLM 产出的代码必须有至少一个 C# 声明或 using 语句。
/// 杜绝"// 假设此处 namespace 为 MyMod4" 类型的占位注释通过检查。
pub(crate) fn validate_generated_code_skein(
    text: &str,
    validation_rules: &[ValidationRule],
) -> Result<(), String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(
            r"(?m)^\s*(public|internal|private|protected|sealed|abstract|static|partial|class|struct|enum|interface|namespace|using|record)\s",
        )
        .unwrap()
    });
    if !re.is_match(text) {
        let non_comment: String = text
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.is_empty() && !t.starts_with("//") && !t.starts_with("/*") && !t.starts_with('*')
            })
            .collect::<Vec<&str>>()
            .join("\n");
        if non_comment.trim().is_empty() {
            return Err(
                "LLM 生成内容只含注释或占位符，无有效 C# 代码。请检查 prompt 或 knowledge 就绪状态后重试"
                    .into(),
            );
        }
    }
    crate::codegen::validate_generated_csharp(text, validation_rules)
}

/// 把 prompt 转给 LLM 流式生成，累积响应后解 fence，再事务性写入
/// `<project_root>/Generated/<entity_name>.cs`。
///
/// 流式 delta 通过 sink 实时推出。供 code_generate 和 batch_custom_code 共用。
///
/// 中途轮询 repo 状态：若 run 被 cancel 则立即返回 `GenerateError::Cancelled`，
/// 让 reqwest stream 被 drop（实际断开网络）。
pub(crate) async fn generate_and_write_code_artifact(
    repo: Arc<dyn RunRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    run_id: &RunId,
    prompt: String,
    entity_name: &str,
    artifacts_dir: &Path,
    validation_rules: &[ValidationRule],
) -> Result<WrittenArtifact, GenerateError> {
    let completion_request = CompletionRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: prompt,
        }],
        system_prompt: None,
        max_tokens: 4096,
        temperature: None,
        model: None,
    };

    let mut stream = llm
        .stream(completion_request)
        .await
        .map_err(GenerateError::Stream)?;

    let mut accumulated = String::new();
    let mut model = String::new();
    let mut usage_in: u32 = 0;
    let mut usage_out: u32 = 0;
    let mut tick: u32 = 0;
    while let Some(item) = stream.next().await {
        tick = tick.wrapping_add(1);
        if tick.is_multiple_of(5) && is_cancelled(&repo, run_id).await {
            emit_cancelled_mid_stream(&sink, run_id).await;
            return Err(GenerateError::Cancelled);
        }
        match item {
            Ok(StreamEvent::Start { model: m }) => {
                model = m;
            }
            Ok(StreamEvent::Delta { text }) => {
                accumulated.push_str(&text);
                sink.emit(ProgressEvent {
                    run_id: run_id.clone(),
                    stage: "stream-delta".into(),
                    percent: None,
                    message: None,
                    delta: Some(text),
                })
                .await;
            }
            Ok(StreamEvent::End { usage, .. }) => {
                usage_in = usage.input_tokens;
                usage_out = usage.output_tokens;
            }
            Err(err) => return Err(GenerateError::Stream(err)),
        }
    }

    let extracted = extract_code_or_reject(&accumulated)?;
    // 兜底校验：生成的"代码"必须有实际声明结构，不能是纯注释占位符
    validate_generated_code_skein(&extracted, validation_rules)
        .map_err(|_| GenerateError::ModelOutput)?;

    let project_root = artifacts_dir.parent().ok_or(GenerateError::Write)?;
    let generated_dir = project_root.join("Generated");
    let cs_path = generated_dir.join(format!("{entity_name}.cs"));
    let transaction =
        ProjectFileTransaction::write_one(cs_path.clone(), extracted.as_bytes().to_vec())
            .await
            .map_err(|_| GenerateError::Write)?;

    Ok(WrittenArtifact {
        model,
        entity_name: entity_name.to_string(),
        cs_path,
        usage_in,
        usage_out,
        transaction: Some(transaction),
    })
}

/// 从累积的原始模型输出中提取代码块，并拒绝空输出。
///
/// 优先取首个围栏代码块；无围栏时回退到原文。若提取结果去空白后为空
/// （模型拒答 / 只回了空白 / 流中途无内容），返回 `Err` 让调用方把 run 标记 Failed，
/// 而不是把 0 字节 `.cs` 当成功产物写盘并计入 batch「succeeded」。
fn extract_code_or_reject(accumulated: &str) -> Result<String, GenerateError> {
    let extracted =
        extract_first_code_block(accumulated).unwrap_or_else(|| accumulated.to_string());
    if extracted.trim().is_empty() {
        return Err(GenerateError::ModelOutput);
    }
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::CustomCodegenRequest;

    #[test]
    fn extract_code_or_reject_errors_on_empty_output() {
        assert!(matches!(
            extract_code_or_reject(""),
            Err(GenerateError::ModelOutput)
        ));
        assert!(matches!(
            extract_code_or_reject("   \n\t  "),
            Err(GenerateError::ModelOutput)
        ));
        // 无围栏但有内容 → 回退原文，不报错
        match extract_code_or_reject("public class Foo {}") {
            Ok(s) => assert_eq!(s, "public class Foo {}"),
            Err(_) => panic!("non-empty content should be accepted"),
        }
        // 有围栏 → 取围栏内代码
        let md = "```csharp\npublic class Bar {}\n```";
        match extract_code_or_reject(md) {
            Ok(s) => assert!(s.contains("public class Bar")),
            Err(_) => panic!("fenced code should be accepted"),
        }
    }

    #[test]
    fn extract_first_code_block_strips_fence() {
        let md = "Here's the file:\n\n```csharp\npublic class Foo {}\n```\n\nNotes after.";
        let extracted = extract_first_code_block(md).expect("fence present");
        assert!(extracted.contains("public class Foo"));
        assert!(!extracted.contains("```"));
    }

    #[test]
    fn extract_first_code_block_handles_no_fence() {
        assert!(extract_first_code_block("just text, no fence").is_none());
    }

    #[test]
    fn extract_first_code_block_takes_first_only() {
        let md = "```cs\nA\n```\nbetween\n```\nB\n```";
        assert_eq!(extract_first_code_block(md).unwrap(), "A");
    }

    #[test]
    fn code_generate_entity_name_sanitizes() {
        let req = SubmitCodeGenerateRequest::CustomCode {
            request: CustomCodegenRequest {
                name: "my hook/v2".into(),
                ..Default::default()
            },
        };
        assert_eq!(code_generate_entity_name(&req), "my_hook_v2");
    }

    #[test]
    fn sanitize_entity_name_handles_empty_and_unicode() {
        assert_eq!(sanitize_entity_name(""), "Unnamed");
        // 中、文 各 1 字符 + 空格 1 字符 = 3 个非 ASCII 字符 → 3 个下划线
        assert_eq!(sanitize_entity_name("中文 Name"), "___Name");
    }
}
