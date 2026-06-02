//! Observer pattern wrapper：把任意 JobRepository 包一层，在 create/update
//! 状态迁移时自动 emit AuditEntry。handler 完全无感知。
//!
//! 这是 Stage 5 audit auto-write 的核心抽象：替代每个 handler 手动 emit，
//! 让 audit 成为 repository 层的横切关注点。
//!
//! 触发规则：
//! - `create()`：emit `job.submitted`，data 含 kind / created_at
//! - `update()`：先 get 旧 job 取 prev_status，与新 status 比较：
//!   - Pending → Running：emit `job.started`
//!   - * → Completed：emit `job.completed`
//!   - * → Failed：emit `job.failed`，data 含 error 摘要
//!   - * → Cancelled：emit `job.cancelled`
//! - 同状态 → 同状态 update（如 Running stage 推进）：不 emit，避免噪音
//!
//! 写失败只 eprintln（在 sink 内部），不传播到 repo 调用方，保证业务路径不被审计影响。

use std::sync::Arc;

use async_trait::async_trait;

use crate::audit::{AuditEntry, AuditSinkArc};
use crate::platform::domain::{Job, JobId, JobRepository, JobResult, JobStatus, JobSummary};

pub struct AuditedJobRepository {
    inner: Arc<dyn JobRepository>,
    audit: AuditSinkArc,
}

impl AuditedJobRepository {
    #[must_use]
    pub fn new(inner: Arc<dyn JobRepository>, audit: AuditSinkArc) -> Self {
        Self { inner, audit }
    }

    /// 暴露内层 repo，便于不需要审计的纯读操作绕过 emit（虽然现在 emit 只在写路径）。
    #[must_use]
    pub fn inner(&self) -> &Arc<dyn JobRepository> {
        &self.inner
    }
}

#[async_trait]
impl JobRepository for AuditedJobRepository {
    async fn create(&self, job: &Job) -> JobResult<()> {
        self.inner.create(job).await?;
        let entry = AuditEntry::new("job.submitted", format!("{:?} submitted", job.kind))
            .with_ref(job.id.0.clone())
            .with_data(serde_json::json!({
                "kind": job.kind,
                "createdAt": job.created_at,
            }));
        self.audit.emit(entry).await;
        Ok(())
    }

    async fn update(&self, job: &Job) -> JobResult<()> {
        let prev = self.inner.get(&job.id).await.ok();
        self.inner.update(job).await?;
        if let Some(p) = prev
            && p.status != job.status
            && let Some(entry) = transition_to_entry(&p, job)
        {
            self.audit.emit(entry).await;
        }
        Ok(())
    }

    async fn get(&self, id: &JobId) -> JobResult<Job> {
        self.inner.get(id).await
    }

    async fn list(&self) -> JobResult<Vec<JobSummary>> {
        self.inner.list().await
    }

    async fn delete(&self, id: &JobId) -> JobResult<()> {
        self.inner.delete(id).await
    }
}

fn transition_to_entry(prev: &Job, current: &Job) -> Option<AuditEntry> {
    let (kind, message) = match current.status {
        JobStatus::Running if matches!(prev.status, JobStatus::Pending) => (
            "job.started",
            format!("{:?} started (attempt {})", current.kind, current.attempts),
        ),
        JobStatus::Completed => ("job.completed", format!("{:?} completed", current.kind)),
        JobStatus::Failed => ("job.failed", format!("{:?} failed", current.kind)),
        JobStatus::Cancelled => ("job.cancelled", format!("{:?} cancelled", current.kind)),
        _ => return None,
    };
    let mut data = serde_json::json!({
        "kind": current.kind,
        "prevStatus": format!("{:?}", prev.status),
        "newStatus": format!("{:?}", current.status),
    });
    if let Some(err) = &current.error {
        data["error"] = serde_json::Value::String(truncate(err, 500));
    }
    Some(
        AuditEntry::new(kind, message)
            .with_ref(current.id.0.clone())
            .with_data(data),
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}...[truncated]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::{AuditSink, FileAuditSink, read_recent};
    use crate::platform::domain::{Job, JobKind};
    use crate::platform::infra::FileJobRepository;
    use std::path::PathBuf;
    use tokio::sync::Mutex;

    /// 收集 in-memory，方便直接 assert，不依赖磁盘。
    struct MemorySink {
        entries: Mutex<Vec<AuditEntry>>,
    }
    impl MemorySink {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                entries: Mutex::new(Vec::new()),
            })
        }
        async fn snapshot(&self) -> Vec<AuditEntry> {
            self.entries.lock().await.clone()
        }
    }
    #[async_trait]
    impl AuditSink for MemorySink {
        async fn emit(&self, entry: AuditEntry) {
            self.entries.lock().await.push(entry);
        }
    }

    fn make_repo(td: &tempfile::TempDir) -> (Arc<dyn JobRepository>, PathBuf) {
        let history = td.path().join("history");
        std::fs::create_dir_all(&history).unwrap();
        (Arc::new(FileJobRepository::new(history.clone())), history)
    }

    #[tokio::test]
    async fn create_emits_submitted() {
        let td = tempfile::TempDir::new().unwrap();
        let (inner, _) = make_repo(&td);
        let sink = MemorySink::new();
        let repo = AuditedJobRepository::new(inner, sink.clone());
        let job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();
        let entries = sink.snapshot().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "job.submitted");
        assert_eq!(entries[0].ref_id.as_deref(), Some(job.id.0.as_str()));
    }

    #[tokio::test]
    async fn status_transitions_emit_in_order() {
        let td = tempfile::TempDir::new().unwrap();
        let (inner, _) = make_repo(&td);
        let sink = MemorySink::new();
        let repo = AuditedJobRepository::new(inner, sink.clone());

        let mut job = Job::new(JobKind::CodeGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();

        // Pending → Running
        job.status = JobStatus::Running;
        job.attempts = 1;
        repo.update(&job).await.unwrap();

        // Running → Completed
        job.status = JobStatus::Completed;
        repo.update(&job).await.unwrap();

        let entries = sink.snapshot().await;
        let kinds: Vec<_> = entries.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, vec!["job.submitted", "job.started", "job.completed"]);
    }

    #[tokio::test]
    async fn same_status_updates_do_not_emit() {
        // running 阶段 stage 推进时多次 update —— 状态不变 → 无 audit
        let td = tempfile::TempDir::new().unwrap();
        let (inner, _) = make_repo(&td);
        let sink = MemorySink::new();
        let repo = AuditedJobRepository::new(inner, sink.clone());

        let mut job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();
        job.status = JobStatus::Running;
        repo.update(&job).await.unwrap(); // emit job.started
        repo.update(&job).await.unwrap(); // no emit
        repo.update(&job).await.unwrap(); // no emit
        let entries = sink.snapshot().await;
        assert_eq!(entries.len(), 2); // submitted + started
    }

    #[tokio::test]
    async fn failed_transition_includes_error_payload() {
        let td = tempfile::TempDir::new().unwrap();
        let (inner, _) = make_repo(&td);
        let sink = MemorySink::new();
        let repo = AuditedJobRepository::new(inner, sink.clone());

        let mut job = Job::new(JobKind::LogAnalysis, serde_json::json!({}));
        repo.create(&job).await.unwrap();
        job.status = JobStatus::Failed;
        job.error = Some("LLM 429 rate limited".into());
        repo.update(&job).await.unwrap();

        let entries = sink.snapshot().await;
        let failed = entries
            .iter()
            .find(|e| e.kind == "job.failed")
            .expect("failed entry");
        assert_eq!(failed.data["error"], "LLM 429 rate limited");
        assert_eq!(failed.data["prevStatus"], "Pending");
        assert_eq!(failed.data["newStatus"], "Failed");
    }

    #[tokio::test]
    async fn file_audit_sink_writes_jsonl() {
        let td = tempfile::TempDir::new().unwrap();
        let project = td.path().to_path_buf();
        std::fs::create_dir_all(project.join(".ats")).unwrap();

        let (inner, _) = make_repo(&td);
        let sink: AuditSinkArc = Arc::new(FileAuditSink::new(project.clone()));
        let repo = AuditedJobRepository::new(inner, sink);

        let mut job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();
        job.status = JobStatus::Running;
        repo.update(&job).await.unwrap();
        job.status = JobStatus::Completed;
        repo.update(&job).await.unwrap();

        let recent = read_recent(&project, 10).unwrap();
        let kinds: Vec<_> = recent.iter().map(|e| e.kind.as_str()).collect();
        // read_recent 倒序：最新在前
        assert_eq!(kinds, vec!["job.completed", "job.started", "job.submitted"]);
    }

    #[tokio::test]
    async fn truncates_long_error_message() {
        let td = tempfile::TempDir::new().unwrap();
        let (inner, _) = make_repo(&td);
        let sink = MemorySink::new();
        let repo = AuditedJobRepository::new(inner, sink.clone());

        let mut job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();
        job.status = JobStatus::Failed;
        job.error = Some("x".repeat(2000));
        repo.update(&job).await.unwrap();

        let entries = sink.snapshot().await;
        let failed = entries.iter().find(|e| e.kind == "job.failed").unwrap();
        let truncated = failed.data["error"].as_str().unwrap();
        assert!(truncated.ends_with("[truncated]"));
        assert!(truncated.chars().count() < 600);
    }
}
