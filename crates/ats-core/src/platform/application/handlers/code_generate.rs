//! code_generate handler：PromptAssembler 装 prompt → LLM 流式 → 解 fence → 写 .cs。
//!
//! Asset 和 CustomCode 两种入参共用同一条主链，区别仅在 prompt 装配步骤。
//! `generate_and_write_code_artifact` 提取出来给 batch_custom_code 复用。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::fs;

use super::common::{
    ProgressEvent, ProgressSink, emit_cancelled_mid_stream, finalize_with_error, is_cancelled,
    transition_to_running,
};
use crate::codegen::PromptAssembler;
use crate::knowledge::{KnowledgePaths, SourceMode};
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::contracts::SubmitCodeGenerateRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};

pub async fn run_code_generate(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitCodeGenerateRequest,
    knowledge_paths: KnowledgePaths,
    artifacts_dir: PathBuf,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    let assembler = PromptAssembler::built_in();
    let prompt_result = match &request {
        SubmitCodeGenerateRequest::Asset { request: req } => {
            assembler.assemble_asset_prompt(req, &knowledge_paths, SourceMode::Missing)
        }
        SubmitCodeGenerateRequest::CustomCode { request: req } => {
            assembler.assemble_custom_code_prompt(req, &knowledge_paths, SourceMode::Missing)
        }
    };
    let prompt = match prompt_result {
        Ok(p) => p,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &format!("prompt assembly: {err}")).await;
            return;
        }
    };
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
        Ok(a) => a,
        Err(GenerateError::Stream(err)) => {
            finalize_with_error(&repo, &job_id, &err).await;
            return;
        }
        Err(GenerateError::Write(err)) => {
            finalize_with_error(&repo, &job_id, &format!("write artifact: {err}")).await;
            return;
        }
        Err(GenerateError::Cancelled) => {
            // 已经在 handler 内 emit cancelled-mid-stream；状态机已是 Cancelled，
            // 不写 result，不动 status，直接退出。
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
        "model": artifact.model,
        "entityName": artifact.entity_name,
        "csPath": artifact.cs_path.display().to_string(),
        "artifactCsPath": artifact.artifact_cs_path.display().to_string(),
        "rawPath": artifact.raw_path.display().to_string(),
        "extractedChars": artifact.extracted_chars,
        "rawChars": artifact.raw_chars,
        "usage": { "inputTokens": artifact.usage_in, "outputTokens": artifact.usage_out },
    }));
    let _ = repo.update(&job).await;
    sink.emit(ProgressEvent {
        job_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!("wrote {}", artifact.cs_path.display())),
        delta: None,
    })
    .await;
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
    Write(String),
    /// 流被取消（job.status=Cancelled）；调用方应当不写 result。
    Cancelled,
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

    let extracted = extract_first_code_block(&accumulated).unwrap_or_else(|| accumulated.clone());

    let target_dir = artifacts_dir.join(entity_name);
    let artifact_cs_path = target_dir.join(format!("{entity_name}.cs"));
    let raw_path = target_dir.join("raw.md");
    let project_root = artifacts_dir.parent().ok_or_else(|| {
        GenerateError::Write(format!(
            "artifacts dir has no parent: {}",
            artifacts_dir.display()
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
    fs::write(&artifact_cs_path, &extracted)
        .await
        .map_err(|e| GenerateError::Write(e.to_string()))?;
    fs::write(&cs_path, &extracted)
        .await
        .map_err(|e| GenerateError::Write(e.to_string()))?;
    fs::write(&raw_path, &accumulated)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::CustomCodegenRequest;

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
