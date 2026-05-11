//! JobApplicationService —— Job 的 submit / get / list / cancel 入口，
//! 兼 3 类内嵌 handler：text_generate / code_generate / build_project。
//!
//! 后续 stage（4 个剩余 handler + 引入 trait 注册表）按需扩展。

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::codegen::PromptAssembler;
use crate::knowledge::{KnowledgePaths, SourceMode};
use crate::llm::{CompletionRequest, LlmClient, Message, MessageRole, StreamEvent};
use crate::platform::contracts::{
    SubmitBuildProjectRequest, SubmitCodeGenerateRequest, SubmitTextGenerateRequest,
};
use crate::platform::domain::{
    Job, JobError, JobId, JobKind, JobProgress, JobRepository, JobResult, JobStatus, JobSummary,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub job_id: JobId,
    pub stage: String,
    pub percent: Option<f32>,
    pub message: Option<String>,
    /// 部分流式 handler 会随事件附带增量文本（如 LLM token）。
    pub delta: Option<String>,
}

#[async_trait]
pub trait ProgressSink: Send + Sync {
    async fn emit(&self, event: ProgressEvent);
}

/// 无操作 sink，方便测试 / 不需要进度回传的场景。
pub struct NoopProgressSink;

#[async_trait]
impl ProgressSink for NoopProgressSink {
    async fn emit(&self, _: ProgressEvent) {}
}

pub struct JobApplicationService {
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
}

impl JobApplicationService {
    #[must_use]
    pub fn new(repo: Arc<dyn JobRepository>, llm: Arc<dyn LlmClient>) -> Self {
        Self { repo, llm }
    }

    pub async fn get(&self, id: &JobId) -> JobResult<Job> {
        self.repo.get(id).await
    }

    pub async fn list(&self) -> JobResult<Vec<JobSummary>> {
        self.repo.list().await
    }

    /// 标记 Cancelled 并保存。任务实际执行中如已经发起 LLM 请求，本 stage 不
    /// 中断网络层；handler 结束时会发现 status=Cancelled 而跳过最终结果覆写。
    pub async fn cancel(&self, id: &JobId) -> JobResult<()> {
        let mut job = self.repo.get(id).await?;
        if job.status.is_terminal() {
            return Err(JobError::Terminal {
                id: job.id.0.clone(),
                status: format!("{:?}", job.status),
            });
        }
        job.status = JobStatus::Cancelled;
        job.completed_at = Some(chrono::Utc::now());
        self.repo.update(&job).await
    }

    /// 提交 text_generate 任务：保存 Pending → spawn 后台 tokio 任务跑 LLM。
    /// 立刻返回 JobId；调用方通过 `get` / `list` / progress 事件观察进度。
    pub async fn submit_text_generate(
        &self,
        request: SubmitTextGenerateRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::TextGenerate, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_text_generate(repo, llm, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }

    /// 提交 code_generate 任务：装 prompt → LLM stream → 解 fence → 写文件。
    ///
    /// `knowledge_paths` 来自 ConfigStatus::runtime_dir()（app data 共享），
    /// 与 active project 解耦。生成的 `.cs` 落到工程的
    /// `artifacts/<name>/<name>.cs`，原始 markdown 同步存到 `raw.md` 便于排错。
    pub async fn submit_code_generate(
        &self,
        request: SubmitCodeGenerateRequest,
        knowledge_paths: KnowledgePaths,
        artifacts_dir: PathBuf,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::CodeGenerate, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let llm = Arc::clone(&self.llm);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_code_generate(
                repo,
                llm,
                sink,
                id_for_task,
                request,
                knowledge_paths,
                artifacts_dir,
            )
            .await;
        });

        Ok(job_id)
    }

    /// 提交 build_project 任务：在 `request.project_root` 下跑 `dotnet publish`，
    /// 捕获 stdout/stderr，按 exit_code + "X Error(s)" 启发判断成功。
    pub async fn submit_build_project(
        &self,
        request: SubmitBuildProjectRequest,
        sink: Arc<dyn ProgressSink>,
    ) -> JobResult<JobId> {
        let payload = serde_json::to_value(&request)
            .map_err(|e| JobError::Storage(format!("serialize request: {e}")))?;
        let job = Job::new(JobKind::BuildProject, payload);
        let job_id = job.id.clone();
        self.repo.create(&job).await?;

        let repo = Arc::clone(&self.repo);
        let id_for_task = job_id.clone();
        tokio::spawn(async move {
            run_build_project(repo, sink, id_for_task, request).await;
        });

        Ok(job_id)
    }
}

async fn run_text_generate(
    repo: Arc<dyn JobRepository>,
    llm: Arc<dyn LlmClient>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitTextGenerateRequest,
) {
    // 转 Running 状态
    if let Err(err) = transition_to_running(&repo, &job_id, &sink).await {
        tracing::warn!(error = %err, "failed to mark job running");
        return;
    }

    let completion_request = CompletionRequest {
        messages: vec![Message {
            role: MessageRole::User,
            content: request.prompt.clone(),
        }],
        system_prompt: request.system_prompt.clone(),
        max_tokens: request.max_tokens.unwrap_or(2048),
        temperature: request.temperature,
        model: request.model.clone(),
    };

    let stream_result = llm.stream(completion_request).await;
    let mut stream = match stream_result {
        Ok(s) => s,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &err.to_string()).await;
            sink.emit(ProgressEvent {
                job_id: job_id.clone(),
                stage: "stream-start-error".into(),
                percent: None,
                message: Some(err.to_string()),
                delta: None,
            })
            .await;
            return;
        }
    };

    let mut accumulated = String::new();
    let mut model = String::new();
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut finish: Option<String> = None;

    while let Some(item) = stream.next().await {
        match item {
            Ok(StreamEvent::Start { model: m }) => {
                model = m.clone();
                sink.emit(ProgressEvent {
                    job_id: job_id.clone(),
                    stage: "stream-start".into(),
                    percent: None,
                    message: Some(format!("model={m}")),
                    delta: None,
                })
                .await;
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
            Ok(StreamEvent::End {
                finish_reason,
                usage,
            }) => {
                input_tokens = usage.input_tokens;
                output_tokens = usage.output_tokens;
                finish = Some(format!("{finish_reason:?}").to_lowercase());
            }
            Err(err) => {
                finalize_with_error(&repo, &job_id, &err.to_string()).await;
                sink.emit(ProgressEvent {
                    job_id: job_id.clone(),
                    stage: "stream-error".into(),
                    percent: None,
                    message: Some(err.to_string()),
                    delta: None,
                })
                .await;
                return;
            }
        }
    }

    // 流结束后检查 job 是否已被 cancel
    let mut job = match repo.get(&job_id).await {
        Ok(j) => j,
        Err(err) => {
            tracing::warn!(error = %err, "job vanished mid-run");
            return;
        }
    };
    if matches!(job.status, JobStatus::Cancelled) {
        // 用户已取消——保留结果到 result 但不改回 Completed
        sink.emit(ProgressEvent {
            job_id: job_id.clone(),
            stage: "cancelled-after-stream".into(),
            percent: None,
            message: Some("job was cancelled while running".into()),
            delta: None,
        })
        .await;
        return;
    }

    job.status = JobStatus::Completed;
    job.completed_at = Some(chrono::Utc::now());
    job.result = Some(serde_json::json!({
        "model": model,
        "content": accumulated,
        "finishReason": finish,
        "usage": { "inputTokens": input_tokens, "outputTokens": output_tokens },
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "completed".into(),
        percent: Some(1.0),
        message: None,
        delta: None,
    })
    .await;
}

async fn transition_to_running(
    repo: &Arc<dyn JobRepository>,
    id: &JobId,
    sink: &Arc<dyn ProgressSink>,
) -> JobResult<()> {
    let mut job = repo.get(id).await?;
    // 用户可能在 spawn 任务被调度前就 cancel 了——尊重该状态，不要回写 Running。
    if matches!(job.status, JobStatus::Cancelled) {
        return Err(JobError::Terminal {
            id: job.id.0.clone(),
            status: format!("{:?}", job.status),
        });
    }
    job.status = JobStatus::Running;
    job.started_at = Some(chrono::Utc::now());
    job.attempts += 1;
    job.progress = Some(JobProgress {
        stage: "running".into(),
        percent: Some(0.0),
        message: None,
    });
    repo.update(&job).await?;
    sink.emit(ProgressEvent {
        job_id: id.clone(),
        stage: "running".into(),
        percent: Some(0.0),
        message: None,
        delta: None,
    })
    .await;
    Ok(())
}

async fn finalize_with_error(repo: &Arc<dyn JobRepository>, id: &JobId, message: &str) {
    if let Ok(mut job) = repo.get(id).await {
        if matches!(job.status, JobStatus::Cancelled) {
            // 已取消优先于失败标记，保留 Cancelled
            return;
        }
        job.status = JobStatus::Failed;
        job.completed_at = Some(chrono::Utc::now());
        job.error = Some(message.to_string());
        let _ = repo.update(&job).await;
    }
}

// -------- code_generate handler --------

#[allow(clippy::too_many_arguments)]
async fn run_code_generate(
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

    let stream_result = llm.stream(completion_request).await;
    let mut stream = match stream_result {
        Ok(s) => s,
        Err(err) => {
            finalize_with_error(&repo, &job_id, &err.to_string()).await;
            return;
        }
    };
    let mut accumulated = String::new();
    let mut model = String::new();
    let mut usage_in: u32 = 0;
    let mut usage_out: u32 = 0;
    while let Some(item) = stream.next().await {
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
                finalize_with_error(&repo, &job_id, &err.to_string()).await;
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

    // 解 markdown fence；若没有 fence 把整个响应当成代码兜底
    let extracted = extract_first_code_block(&accumulated)
        .unwrap_or_else(|| accumulated.clone());

    // 落盘到 <artifacts_dir>/<name>/<name>.cs + raw.md
    let target_dir = artifacts_dir.join(&entity_name);
    let cs_path = target_dir.join(format!("{entity_name}.cs"));
    let raw_path = target_dir.join("raw.md");
    let write_result: std::io::Result<()> = async {
        fs::create_dir_all(&target_dir).await?;
        fs::write(&cs_path, &extracted).await?;
        fs::write(&raw_path, &accumulated).await?;
        Ok(())
    }
    .await;

    let mut job = job;
    match write_result {
        Ok(()) => {
            job.status = JobStatus::Completed;
            job.completed_at = Some(chrono::Utc::now());
            job.result = Some(serde_json::json!({
                "model": model,
                "entityName": entity_name,
                "csPath": cs_path.display().to_string(),
                "rawPath": raw_path.display().to_string(),
                "extractedChars": extracted.len(),
                "rawChars": accumulated.len(),
                "usage": { "inputTokens": usage_in, "outputTokens": usage_out },
            }));
            let _ = repo.update(&job).await;
            sink.emit(ProgressEvent {
                job_id,
                stage: "completed".into(),
                percent: Some(1.0),
                message: Some(format!("wrote {}", cs_path.display())),
                delta: None,
            })
            .await;
        }
        Err(err) => {
            finalize_with_error(&repo, &job.id, &format!("write artifact: {err}")).await;
        }
    }
}

fn code_generate_entity_name(req: &SubmitCodeGenerateRequest) -> String {
    let raw = match req {
        SubmitCodeGenerateRequest::Asset { request } => request.asset_name.clone(),
        SubmitCodeGenerateRequest::CustomCode { request } => request.name.clone(),
    };
    let safe: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
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

// -------- build_project handler --------

async fn run_build_project(
    repo: Arc<dyn JobRepository>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitBuildProjectRequest,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "spawning".into(),
        percent: None,
        message: Some(format!("dotnet publish in {}", request.project_root.display())),
        delta: None,
    })
    .await;

    let cwd = request.project_root.clone();
    let output_result = tokio::task::spawn_blocking(move || {
        // tokio::process::Command::output 在 Windows 下偶尔挂起；用 std + spawn_blocking 更稳。
        std::process::Command::new("dotnet")
            .arg("publish")
            .current_dir(&cwd)
            .output()
    })
    .await;

    let output = match output_result {
        Ok(Ok(out)) => out,
        Ok(Err(err)) => {
            finalize_with_error(&repo, &job_id, &format!("spawn dotnet: {err}")).await;
            return;
        }
        Err(err) => {
            finalize_with_error(&repo, &job_id, &format!("join blocking: {err}")).await;
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);
    // 启发判定：exit 0 优先；否则若 stdout 含 "0 Error(s)" 视为成功（Godot 导出常退 -1 但 MSBuild 成功）
    let success = exit_code == 0 || stdout.contains("0 Error(s)");

    let job = match repo.get(&job_id).await {
        Ok(j) => j,
        Err(_) => return,
    };
    if matches!(job.status, JobStatus::Cancelled) {
        return;
    }

    let mut job = job;
    job.status = if success { JobStatus::Completed } else { JobStatus::Failed };
    job.completed_at = Some(chrono::Utc::now());
    if !success {
        job.error = Some(format!("dotnet publish exited with code {exit_code}"));
    }
    job.result = Some(serde_json::json!({
        "success": success,
        "exitCode": exit_code,
        "stdoutTail": tail(&stdout, 5000),
        "stderrTail": tail(&stderr, 5000),
        "projectRoot": request.project_root.display().to_string(),
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id,
        stage: if success { "completed".into() } else { "failed".into() },
        percent: Some(1.0),
        message: Some(format!("exit_code={exit_code}, success={success}")),
        delta: None,
    })
    .await;
}

fn tail(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let skip = text.chars().count() - max_chars;
    let tail: String = text.chars().skip(skip).collect();
    format!("...[truncated {skip} chars]\n{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{
        CompletionResponse, CompletionStream, FinishReason, LlmError, Usage,
    };
    use crate::platform::infra::FileJobRepository;
    use futures_util::stream;
    use std::sync::Mutex;

    /// 单测用 LLM 客户端：按预设的事件序列返回流；非流式接口未用。
    struct ScriptedLlm {
        events: Mutex<Vec<Result<StreamEvent, LlmError>>>,
    }

    #[async_trait]
    impl LlmClient for ScriptedLlm {
        async fn complete(
            &self,
            _: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            unimplemented!()
        }
        async fn stream(
            &self,
            _: CompletionRequest,
        ) -> Result<CompletionStream, LlmError> {
            let evs: Vec<_> = self.events.lock().unwrap().drain(..).collect();
            Ok(Box::pin(stream::iter(evs)))
        }
    }

    struct CapturingSink {
        events: tokio::sync::Mutex<Vec<ProgressEvent>>,
    }

    #[async_trait]
    impl ProgressSink for CapturingSink {
        async fn emit(&self, event: ProgressEvent) {
            self.events.lock().await.push(event);
        }
    }

    async fn make_service() -> (
        tempfile::TempDir,
        JobApplicationService,
        Arc<CapturingSink>,
    ) {
        let td = tempfile::TempDir::new().unwrap();
        let repo = Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm = Arc::new(ScriptedLlm {
            events: Mutex::new(vec![
                Ok(StreamEvent::Start { model: "test-model".into() }),
                Ok(StreamEvent::Delta { text: "hello ".into() }),
                Ok(StreamEvent::Delta { text: "world".into() }),
                Ok(StreamEvent::End {
                    finish_reason: FinishReason::EndTurn,
                    usage: Usage { input_tokens: 4, output_tokens: 2 },
                }),
            ]),
        });
        let sink = Arc::new(CapturingSink {
            events: tokio::sync::Mutex::new(Vec::new()),
        });
        let service = JobApplicationService::new(repo, llm);
        (td, service, sink)
    }

    #[tokio::test]
    async fn text_generate_flow_completes_and_persists_result() {
        let (_td, service, sink) = make_service().await;
        let req = SubmitTextGenerateRequest {
            prompt: "say hi".into(),
            ..Default::default()
        };
        let id = service
            .submit_text_generate(req, sink.clone())
            .await
            .unwrap();
        // spawn 后等任务跑完。最长 500ms 应足够，因 ScriptedLlm 是同步内存事件。
        for _ in 0..50 {
            let job = service.get(&id).await.unwrap();
            if job.status.is_terminal() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let job = service.get(&id).await.unwrap();
        assert_eq!(job.status, JobStatus::Completed);
        let result = job.result.expect("result should be set");
        assert_eq!(result["content"], "hello world");
        assert_eq!(result["model"], "test-model");
        assert_eq!(result["usage"]["outputTokens"], 2);

        let events = sink.events.lock().await;
        assert!(events.iter().any(|e| e.stage == "running"));
        assert!(events.iter().any(|e| e.stage == "stream-delta"));
        assert!(events.iter().any(|e| e.stage == "completed"));
    }

    #[tokio::test]
    async fn cancel_marks_pending_job_cancelled() {
        // 直接操纵 repo 绕开 spawn 任务，专注测 cancel 状态机本身。
        // submit→cancel 的竞态依赖文件 rename 顺序，非确定性，留给真机冒烟覆盖。
        let td = tempfile::TempDir::new().unwrap();
        let repo: Arc<dyn JobRepository> = Arc::new(FileJobRepository::new(td.path().to_path_buf()));
        let llm: Arc<dyn LlmClient> = Arc::new(ScriptedLlm {
            events: Mutex::new(vec![]),
        });
        let service = JobApplicationService::new(repo.clone(), llm);

        let job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();

        service.cancel(&job.id).await.unwrap();
        let reloaded = service.get(&job.id).await.unwrap();
        assert_eq!(reloaded.status, JobStatus::Cancelled);

        // 再次 cancel 应当报 Terminal
        let err = service.cancel(&job.id).await.unwrap_err();
        assert!(matches!(err, JobError::Terminal { .. }));
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
    fn tail_keeps_short_text_intact() {
        assert_eq!(tail("hello", 100), "hello");
    }

    #[test]
    fn tail_truncates_long_text() {
        let s: String = "x".repeat(120);
        let t = tail(&s, 50);
        assert!(t.starts_with("...[truncated"));
        // 末尾应保留 50 字符
        let tail_chars: String = t.chars().rev().take(50).collect();
        assert_eq!(tail_chars.chars().count(), 50);
    }

    #[test]
    fn code_generate_entity_name_sanitizes() {
        use crate::codegen::CustomCodegenRequest;
        let req = SubmitCodeGenerateRequest::CustomCode {
            request: CustomCodegenRequest {
                name: "my hook/v2".into(),
                ..Default::default()
            },
        };
        assert_eq!(code_generate_entity_name(&req), "my_hook_v2");
    }
}
