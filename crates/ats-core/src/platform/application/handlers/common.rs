//! Handler 之间共享的进度事件类型 + 状态迁移 helper。

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::platform::domain::{JobError, JobId, JobProgress, JobRepository, JobResult, JobStatus};

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

/// 将 Pending 任务置为 Running 并 emit 一个 "running" 事件。
///
/// 若任务已被 cancel，返回 Terminal 错误供调用方早退。
pub async fn transition_to_running(
    repo: &Arc<dyn JobRepository>,
    id: &JobId,
    sink: &Arc<dyn ProgressSink>,
) -> JobResult<()> {
    let mut job = repo.get(id).await?;
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

/// 任务失败收口。若 job 已 Cancelled 则保留该状态。
pub async fn finalize_with_error(repo: &Arc<dyn JobRepository>, id: &JobId, message: &str) {
    if let Ok(mut job) = repo.get(id).await {
        if matches!(job.status, JobStatus::Cancelled) {
            return;
        }
        job.status = JobStatus::Failed;
        job.completed_at = Some(chrono::Utc::now());
        job.error = Some(message.to_string());
        let _ = repo.update(&job).await;
    }
}

/// 轮询检查 job 是否被取消。Stream 循环每 N 个事件调一次，发现取消则 break
/// 以让 stream / reqwest 连接被 drop，真正断开 LLM 网络请求。
///
/// 任何 repo 读错误（罕见）视为未取消，避免误判中断正常任务。
pub async fn is_cancelled(repo: &Arc<dyn JobRepository>, id: &JobId) -> bool {
    matches!(
        repo.get(id).await.ok().map(|j| j.status),
        Some(JobStatus::Cancelled)
    )
}

/// stream 循环里被取消时统一发的进度事件。
pub async fn emit_cancelled_mid_stream(sink: &Arc<dyn ProgressSink>, job_id: &JobId) {
    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "cancelled-mid-stream".into(),
        percent: None,
        message: Some("job cancelled; dropping stream".into()),
        delta: None,
    })
    .await;
}
