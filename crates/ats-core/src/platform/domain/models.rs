//! Job 领域模型。
//!
//! 设计取向：
//! - JobId 是字符串新类型（UUID v4），跨双轨共用
//! - payload / result 用 serde_json::Value 通用容器，handler 内部按 kind 解码
//! - 时间戳全部 UTC，序列化为 ISO-8601
//! - 终态：Completed / Failed / Cancelled，进入后不可再变

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct JobId(pub String);

impl JobId {
    #[must_use]
    pub fn new() -> Self {
        // 极简 UUID v4：用纳秒时间 + 进程内计数器拼接，避免引入 uuid crate
        // 大依赖。生产实测下碰撞概率可忽略；冲突时 FileJobRepository::create 会
        // 报 AlreadyExists 给上层重试。
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Self(format!("job-{ns:032x}-{n:08x}"))
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    TextGenerate,
    CodeGenerate,
    AssetGenerate,
    BatchCustomCode,
    BuildProject,
    PackageProject,
    SingleAssetPlan,
    LogAnalysis,
    TruthSnapshotRefresh,
    /// Historical only. New submissions must use `TruthSnapshotRefresh`.
    KnowledgeRefresh,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobStatus {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub stage: String,
    pub percent: Option<f32>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: JobId,
    pub kind: JobKind,
    pub status: JobStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub progress: Option<JobProgress>,
    pub attempts: u32,
}

impl Job {
    #[must_use]
    pub fn new(kind: JobKind, payload: serde_json::Value) -> Self {
        Self {
            id: JobId::new(),
            kind,
            status: JobStatus::Pending,
            created_at: chrono::Utc::now(),
            started_at: None,
            completed_at: None,
            payload,
            result: None,
            error: None,
            progress: None,
            attempts: 0,
        }
    }
}

/// 任务清单视图（不含大字段如 payload / result）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub id: JobId,
    pub kind: JobKind,
    pub status: JobStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub progress: Option<JobProgress>,
    pub error: Option<String>,
}

impl From<&Job> for JobSummary {
    fn from(job: &Job) -> Self {
        Self {
            id: job.id.clone(),
            kind: job.kind,
            status: job.status,
            created_at: job.created_at,
            completed_at: job.completed_at,
            progress: job.progress.clone(),
            error: job.error.clone(),
        }
    }
}
