//! log_analysis handler：读构建日志 → 调 LLM 出诊断 markdown。
//!
//! 不写文件、不耦合工程目录，诊断结果直接落到 `job.result.report`。前端 / Tauri
//! 命令拿到后自行渲染或拷贝。
//!
//! 输入两种来源：内联文本（log_text）或文件路径（log_path）。两者都缺即失败。
//! 日志超过 max_log_chars（默认 30000）时只取末尾，避免 LLM token 爆。

use std::sync::Arc;

use futures_util::StreamExt;

use super::common::{
    ProgressEvent, ProgressSink, emit_cancelled_mid_stream, finalize_with_error, is_cancelled,
    transition_to_running,
};
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::contracts::SubmitLogAnalysisRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};

const DEFAULT_MAX_LOG_CHARS: usize = 30_000;
const SYSTEM_PROMPT: &str = "你是一位经验丰富的 .NET / MSBuild 构建专家，专门诊断 dotnet publish 失败。\n\n\
任务：用户会提供一段构建日志（可能包含 stdout 和 stderr）。请：\n\
1. 优先找出**根本原因**（不要罗列所有 warning）。\n\
2. 给出**最可能的修复方向**，具体到文件 / NuGet 包 / 配置项 / 命令。\n\
3. 如属 NuGet 包缺失、版本冲突、命名空间错误、SDK 版本不匹配等典型问题，明确指出。\n\
4. 如日志看起来已成功（含 \"0 Error(s)\"），如实说明并提示用户检查是否拿错文件。\n\n\
输出格式：中文 markdown，固定三段结构：\n\
## 根本原因\n## 建议修复\n## 额外观察\n";

pub async fn run_log_analysis(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitLogAnalysisRequest,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    // 1. 取日志内容：优先 inline text，其次读文件
    let raw_log = match resolve_log_text(&request).await {
        Ok(t) => t,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &sink, &err).await;
            return;
        }
    };
    if raw_log.trim().is_empty() {
        finalize_with_error(&repo, &job_id, &sink, "log content is empty").await;
        return;
    }

    let max_chars = request.max_log_chars.unwrap_or(DEFAULT_MAX_LOG_CHARS);
    let (log_for_prompt, truncated_chars) = truncate_tail(&raw_log, max_chars);

    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "prepared".into(),
        percent: Some(0.1),
        message: Some(format!(
            "log {} chars, {} truncated, sending to LLM",
            raw_log.chars().count(),
            truncated_chars
        )),
        delta: None,
    })
    .await;

    // 2. 装 prompt
    let user_prompt = build_user_prompt(&log_for_prompt, request.context_hint.as_deref());
    let completion_request = CompletionRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: user_prompt,
        }],
        system_prompt: Some(SYSTEM_PROMPT.to_string()),
        max_tokens: 2048,
        temperature: None,
        model: None,
    };

    let mut stream = match llm.stream(completion_request).await {
        Ok(s) => s,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &sink, &err.to_string()).await;
            return;
        }
    };

    let mut accumulated = String::new();
    let mut model = String::new();
    let mut usage_in: u32 = 0;
    let mut usage_out: u32 = 0;
    let mut tick: u32 = 0;
    while let Some(item) = stream.next().await {
        tick = tick.wrapping_add(1);
        if tick.is_multiple_of(5) && is_cancelled(&repo, &job_id).await {
            emit_cancelled_mid_stream(&sink, &job_id).await;
            return;
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
            Err(err) => {
                finalize_with_error(&repo, &job_id, &sink, &err.to_string()).await;
                return;
            }
        }
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
        "model": model,
        "report": accumulated,
        "logChars": raw_log.chars().count(),
        "truncatedChars": truncated_chars,
        "usage": { "inputTokens": usage_in, "outputTokens": usage_out },
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!("report {} chars", accumulated.chars().count())),
        delta: None,
    })
    .await;
}

async fn resolve_log_text(request: &SubmitLogAnalysisRequest) -> Result<String, String> {
    if let Some(text) = &request.log_text {
        return Ok(text.clone());
    }
    if let Some(path) = &request.log_path {
        return tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("read log file {}: {e}", path.display()));
    }
    Err("neither log_text nor log_path provided".into())
}

fn build_user_prompt(log: &str, context_hint: Option<&str>) -> String {
    let mut buf = String::new();
    if let Some(hint) = context_hint
        && !hint.trim().is_empty()
    {
        buf.push_str("用户上下文提示：\n");
        buf.push_str(hint.trim());
        buf.push_str("\n\n");
    }
    buf.push_str("以下是构建日志：\n\n```\n");
    buf.push_str(log);
    buf.push_str("\n```\n");
    buf
}

/// 取文本末尾 max_chars 个字符。若未截断，第二个返回值为 0。
fn truncate_tail(text: &str, max_chars: usize) -> (String, usize) {
    let total = text.chars().count();
    if total <= max_chars {
        return (text.to_string(), 0);
    }
    let skip = total - max_chars;
    let tail: String = text.chars().skip(skip).collect();
    (
        format!("...[截断 {skip} 字符 / 共 {total} 字符]\n{tail}"),
        skip,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{CompletionResponse, CompletionStream, FinishReason, LlmError, Usage};
    use crate::platform::application::JobApplicationService;
    use crate::platform::domain::{JobRepository, JobStatus};
    use crate::platform::infra::FileJobRepository;
    use async_trait::async_trait;
    use futures_util::stream;
    use std::sync::Mutex;

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

    fn make_llm(events: Vec<Result<StreamEvent, LlmError>>) -> Arc<dyn LlmClient> {
        Arc::new(ScriptedLlm {
            events: Mutex::new(events),
        })
    }

    fn diagnostic_events() -> Vec<Result<StreamEvent, LlmError>> {
        vec![
            Ok(StreamEvent::Start {
                model: "diag-model".into(),
            }),
            Ok(StreamEvent::Delta {
                text: "## 根本原因\n\n".into(),
            }),
            Ok(StreamEvent::Delta {
                text: "缺失 Newtonsoft.Json。\n".into(),
            }),
            Ok(StreamEvent::End {
                finish_reason: FinishReason::EndTurn,
                usage: Usage {
                    input_tokens: 50,
                    output_tokens: 12,
                },
            }),
        ]
    }

    async fn wait_terminal(service: &JobApplicationService, id: &JobId) {
        for _ in 0..50 {
            let job = service.get(id).await.unwrap();
            if job.status.is_terminal() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn log_analysis_inline_text_succeeds() {
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn JobRepository> =
            Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm = make_llm(diagnostic_events());
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitLogAnalysisRequest {
            log_text: Some("MSBUILD : error MSB1009: Project file does not exist.".into()),
            ..Default::default()
        };
        let id = service.submit_log_analysis(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let result = job.result.expect("result");
        assert!(result["report"].as_str().unwrap().contains("根本原因"));
        assert_eq!(result["model"], "diag-model");
        assert_eq!(result["truncatedChars"], 0);
    }

    #[tokio::test]
    async fn log_analysis_reads_log_file() {
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn JobRepository> =
            Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm = make_llm(diagnostic_events());
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let log_file = td.path().join("build.log");
        tokio::fs::write(&log_file, b"CS0246: namespace not found\n")
            .await
            .unwrap();

        let req = SubmitLogAnalysisRequest {
            log_path: Some(log_file),
            ..Default::default()
        };
        let id = service.submit_log_analysis(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
    }

    #[tokio::test]
    async fn log_analysis_fails_when_neither_input_provided() {
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn JobRepository> =
            Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm = make_llm(vec![]);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitLogAnalysisRequest::default();
        let id = service.submit_log_analysis(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(
            job.error
                .as_deref()
                .unwrap_or("")
                .contains("neither log_text nor log_path")
        );
    }

    #[tokio::test]
    async fn log_analysis_fails_on_empty_log() {
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn JobRepository> =
            Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm = make_llm(vec![]);
        let sink = Arc::new(super::super::common::NoopProgressSink);
        let service = JobApplicationService::new(repo, llm);

        let req = SubmitLogAnalysisRequest {
            log_text: Some("   \n  \n".into()),
            ..Default::default()
        };
        let id = service.submit_log_analysis(req, sink).await.unwrap();
        wait_terminal(&service, &id).await;

        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Failed);
        assert!(job.error.as_deref().unwrap_or("").contains("empty"));
    }

    #[test]
    fn truncate_tail_keeps_short() {
        let (out, skipped) = truncate_tail("hello", 100);
        assert_eq!(out, "hello");
        assert_eq!(skipped, 0);
    }

    #[test]
    fn truncate_tail_truncates_long() {
        let s = "x".repeat(100);
        let (out, skipped) = truncate_tail(&s, 30);
        assert_eq!(skipped, 70);
        assert!(out.starts_with("...[截断"));
        // 截断标记后留 30 个字符
        let body = out.split_once('\n').unwrap().1;
        assert_eq!(body.chars().count(), 30);
    }

    #[test]
    fn build_user_prompt_includes_context_hint() {
        let p = build_user_prompt("LOG", Some("我刚改了 TargetFramework"));
        assert!(p.contains("用户上下文提示"));
        assert!(p.contains("TargetFramework"));
        assert!(p.contains("LOG"));
    }

    #[test]
    fn build_user_prompt_omits_blank_hint() {
        let p = build_user_prompt("LOG", Some("   "));
        assert!(!p.contains("用户上下文提示"));
        assert!(p.contains("LOG"));
    }
}
