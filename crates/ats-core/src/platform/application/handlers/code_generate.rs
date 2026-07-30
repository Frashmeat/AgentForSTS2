//! code_generate handler：asset 走结构化 bundle + compile gate，custom_code 走单 C# fence。
//!
//! `generate_and_write_code_artifact` 保留给 custom_code 和 batch_custom_code 复用；
//! asset 的多文件事务由 `asset_bundle` 模块统一处理。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::fs;

use super::asset_bundle::{AssetBundleError, AssetBundleGeneration, validate_project_scope};
use super::asset_compile::AssetCompileValidator;
use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, emit_cancelled_mid_stream, finalize_with_error,
    finalize_with_success, is_cancelled, transition_to_running,
};
use crate::codegen::{AssetKind, PromptAssembler};
use crate::game_pack::VerifiedGameContext;
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::contracts::SubmitCodeGenerateRequest;
use crate::platform::domain::{JobId, JobRepository};

#[allow(clippy::too_many_arguments)] // handler 直接接收 job 依赖与 compile adapter，保持 service 注入方式一致
pub(crate) async fn run_code_generate(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitCodeGenerateRequest,
    game_context: VerifiedGameContext,
    artifacts_dir: PathBuf,
    compile_validator: Arc<dyn AssetCompileValidator>,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    if let SubmitCodeGenerateRequest::Asset { request: asset } = &request {
        if let Err(err) = validate_project_scope(&asset.project_root, &artifacts_dir) {
            finalize_with_error(
                &repo,
                &job_id,
                &sink,
                &format!("invalid asset project scope: {err}"),
            )
            .await;
            return;
        }
        if AssetKind::parse(&asset.asset_type).is_none() {
            finalize_with_error(
                &repo,
                &job_id,
                &sink,
                &format!("unsupported asset_type: {}", asset.asset_type),
            )
            .await;
            return;
        }
    }

    let assembler = PromptAssembler::built_in();
    let prompt_result = match &request {
        SubmitCodeGenerateRequest::Asset { request: req } => assembler
            .assemble_asset_prompt_with_evidence(req, &game_context)
            .map(|assembly| (assembly.prompt, assembly.evidence_record)),
        SubmitCodeGenerateRequest::CustomCode { request: req } => assembler
            .assemble_custom_code_prompt(req, &game_context)
            .map(|prompt| (prompt, String::new())),
    };
    let (prompt, evidence_record) = match prompt_result {
        Ok(value) => value,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &sink, &format!("prompt assembly: {err}")).await;
            return;
        }
    };
    let (result, output_path) = match &request {
        SubmitCodeGenerateRequest::Asset {
            request: asset_request,
        } => {
            let generator = AssetBundleGeneration::new(
                Arc::clone(&repo),
                Arc::clone(&llm),
                compile_validator,
                Arc::clone(&sink),
            );
            let artifact = match generator
                .generate(
                    &job_id,
                    prompt,
                    &evidence_record,
                    asset_request,
                    &artifacts_dir,
                    None,
                )
                .await
            {
                Ok(artifact) => artifact,
                Err(err) => {
                    finalize_asset_bundle_error(&repo, &job_id, &sink, err).await;
                    return;
                }
            };
            let output_path = artifact.cs_path.clone();
            let localization_paths: Vec<String> = artifact
                .localization_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            let result = serde_json::json!({
                "model": artifact.model,
                "entityName": artifact.entity_name,
                "csPath": artifact.cs_path.display().to_string(),
                "artifactCsPath": artifact.artifact_cs_path.display().to_string(),
                "rawPath": artifact.raw_path.display().to_string(),
                "evidencePath": artifact.evidence_path.display().to_string(),
                "localizationPaths": localization_paths,
                "runtimeImagePaths": artifact.runtime_image_paths.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
                "extractedChars": artifact.extracted_chars,
                "rawChars": artifact.raw_chars,
                "usage": { "inputTokens": artifact.usage_in, "outputTokens": artifact.usage_out },
                "compileGate": {
                    "exitCode": artifact.compile.exit_code,
                    "stdoutTail": artifact.compile.stdout_tail,
                    "stderrTail": artifact.compile.stderr_tail,
                },
            });
            (result, output_path)
        }
        SubmitCodeGenerateRequest::CustomCode { .. } => {
            let entity_name = code_generate_entity_name(&request);
            let artifact = match generate_and_write_code_artifact(
                Arc::clone(&repo),
                Arc::clone(&llm),
                Arc::clone(&sink),
                &job_id,
                prompt,
                &entity_name,
                &artifacts_dir,
            )
            .await
            {
                Ok(artifact) => artifact,
                Err(GenerateError::Stream(err)) => {
                    finalize_with_error(&repo, &job_id, &sink, &err).await;
                    return;
                }
                Err(GenerateError::ModelOutput(err)) => {
                    finalize_with_error(
                        &repo,
                        &job_id,
                        &sink,
                        &format!("invalid code model output: {err}"),
                    )
                    .await;
                    return;
                }
                Err(GenerateError::Write(err)) => {
                    finalize_with_error(&repo, &job_id, &sink, &format!("write artifact: {err}"))
                        .await;
                    return;
                }
                Err(GenerateError::Cancelled) => return,
            };
            let output_path = artifact.cs_path.clone();
            let result = serde_json::json!({
                "model": artifact.model,
                "entityName": artifact.entity_name,
                "csPath": artifact.cs_path.display().to_string(),
                "artifactCsPath": artifact.artifact_cs_path.display().to_string(),
                "rawPath": artifact.raw_path.display().to_string(),
                "extractedChars": artifact.extracted_chars,
                "rawChars": artifact.raw_chars,
                "usage": { "inputTokens": artifact.usage_in, "outputTokens": artifact.usage_out },
            });
            (result, output_path)
        }
    };

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
        message: Some(format!("wrote {}", output_path.display())),
        delta: None,
    })
    .await;
}

pub(crate) async fn finalize_asset_bundle_error(
    repo: &Arc<dyn JobRepository>,
    job_id: &JobId,
    sink: &Arc<dyn ProgressSink>,
    error: AssetBundleError,
) {
    let message = match error {
        AssetBundleError::Stream(err) => format!("asset model stream: {err}"),
        AssetBundleError::ModelOutput(err) => format!("invalid asset model output: {err}"),
        AssetBundleError::Write(err) => format!("write asset bundle: {err}"),
        AssetBundleError::Compile(err) => format!("asset compile gate: {err}"),
        AssetBundleError::Cancelled => return,
    };
    finalize_with_error(repo, job_id, sink, &message).await;
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
    pub artifact_cs_path: PathBuf,
    pub raw_path: PathBuf,
    pub extracted_chars: usize,
    pub raw_chars: usize,
    pub usage_in: u32,
    pub usage_out: u32,
}

pub(crate) enum GenerateError {
    Stream(String),
    ModelOutput(String),
    Write(String),
    /// 流被取消（job.status=Cancelled）；调用方应当不写 result。
    Cancelled,
}

/// 兜底校验：LLM 产出的代码必须有至少一个 C# 声明或 using 语句。
/// 杜绝"// 假设此处 namespace 为 MyMod4" 类型的占位注释通过检查。
pub(crate) fn validate_generated_code_skein(text: &str) -> Result<(), String> {
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
    crate::codegen::validate_sts2_generated_csharp(text)
}

/// 把 prompt 转给 LLM 流式生成，累积响应后解 fence，再写到
/// `<artifacts_dir>/<entity_name>/<entity_name>.cs` + `raw.md`。
/// 同时把可编译 `.cs` 镜像到 `<project_root>/Generated/<entity_name>.cs`，
/// 让 IDE 和 dotnet 工程能直接识别生成源码。
///
/// 流式 delta 通过 sink 实时推出。供 code_generate 和 batch_custom_code 共用。
///
/// 中途轮询 repo 状态：若 job 被 cancel 则立即返回 `GenerateError::Cancelled`，
/// 让 reqwest stream 被 drop（实际断开网络）。
pub(crate) async fn generate_and_write_code_artifact(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: &JobId,
    prompt: String,
    entity_name: &str,
    artifacts_dir: &Path,
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
        .map_err(|e| GenerateError::Stream(e.to_string()))?;

    let mut accumulated = String::new();
    let mut model = String::new();
    let mut usage_in: u32 = 0;
    let mut usage_out: u32 = 0;
    let mut tick: u32 = 0;
    while let Some(item) = stream.next().await {
        tick = tick.wrapping_add(1);
        if tick.is_multiple_of(5) && is_cancelled(&repo, job_id).await {
            emit_cancelled_mid_stream(&sink, job_id).await;
            return Err(GenerateError::Cancelled);
        }
        match item {
            Ok(StreamEvent::Start { model: m }) => {
                model = m;
            }
            Ok(StreamEvent::Delta { text }) => {
                accumulated.push_str(&text);
                sink.emit(ProgressEvent {
                    job_id: job_id.clone(),
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
            Err(err) => return Err(GenerateError::Stream(err.to_string())),
        }
    }

    let extracted = extract_code_or_reject(&accumulated)?;
    // 兜底校验：生成的"代码"必须有实际声明结构，不能是纯注释占位符
    validate_generated_code_skein(&extracted).map_err(GenerateError::ModelOutput)?;

    let target_dir = artifacts_dir.join(entity_name);
    let artifact_cs_path = target_dir.join(format!("{entity_name}.cs"));
    let raw_path = target_dir.join("raw.md");
    let project_root = artifacts_dir.parent().ok_or_else(|| {
        GenerateError::Write(format!(
            "artifacts dir has no parent: {}",
            artifacts_dir.display(),
        ))
    })?;
    let generated_dir = project_root.join("Generated");
    let cs_path = generated_dir.join(format!("{entity_name}.cs"));
    fs::create_dir_all(&target_dir)
        .await
        .map_err(|e| GenerateError::Write(e.to_string()))?;
    fs::create_dir_all(&generated_dir)
        .await
        .map_err(|e| GenerateError::Write(e.to_string()))?;
    crate::fs_atomic::write_atomic(&artifact_cs_path, extracted.as_bytes())
        .await
        .map_err(|e| GenerateError::Write(e.to_string()))?;
    crate::fs_atomic::write_atomic(&cs_path, extracted.as_bytes())
        .await
        .map_err(|e| GenerateError::Write(e.to_string()))?;
    crate::fs_atomic::write_atomic(&raw_path, accumulated.as_bytes())
        .await
        .map_err(|e| GenerateError::Write(e.to_string()))?;

    Ok(WrittenArtifact {
        model,
        entity_name: entity_name.to_string(),
        cs_path,
        artifact_cs_path,
        raw_path,
        extracted_chars: extracted.len(),
        raw_chars: accumulated.len(),
        usage_in,
        usage_out,
    })
}

/// 从累积的原始模型输出中提取代码块，并拒绝空输出。
///
/// 优先取首个围栏代码块；无围栏时回退到原文。若提取结果去空白后为空
/// （模型拒答 / 只回了空白 / 流中途无内容），返回 `Err` 让调用方把 job 标记 Failed，
/// 而不是把 0 字节 `.cs` 当成功产物写盘并计入 batch「succeeded」。
fn extract_code_or_reject(accumulated: &str) -> Result<String, GenerateError> {
    let extracted =
        extract_first_code_block(accumulated).unwrap_or_else(|| accumulated.to_string());
    if extracted.trim().is_empty() {
        return Err(GenerateError::Stream(
            "model produced no code (empty output)".into(),
        ));
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
            Err(GenerateError::Stream(_))
        ));
        assert!(matches!(
            extract_code_or_reject("   \n\t  "),
            Err(GenerateError::Stream(_))
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
