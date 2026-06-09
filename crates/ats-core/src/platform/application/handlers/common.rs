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
    // CAS：仅当未被 cancel 时才置 Running，避免与并发 cancel 互相覆盖。
    let job = repo
        .modify(
            id,
            Box::new(|job| {
                if matches!(job.status, JobStatus::Cancelled) {
                    false
                } else {
                    job.status = JobStatus::Running;
                    job.started_at = Some(chrono::Utc::now());
                    job.attempts += 1;
                    job.progress = Some(JobProgress {
                        stage: "running".into(),
                        percent: Some(0.0),
                        message: None,
                    });
                    true
                }
            }),
        )
        .await?;
    if matches!(job.status, JobStatus::Cancelled) {
        return Err(JobError::Terminal {
            id: job.id.0.clone(),
            status: format!("{:?}", job.status),
        });
    }
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

/// 任务失败收口。CAS：仅当未处于终态时才落 Failed，保留已有的 Cancelled 等终态。
pub async fn finalize_with_error(repo: &Arc<dyn JobRepository>, id: &JobId, message: &str) {
    let message = message.to_string();
    let _ = repo
        .modify(
            id,
            Box::new(move |job| {
                if job.status.is_terminal() {
                    false
                } else {
                    job.status = JobStatus::Failed;
                    job.completed_at = Some(chrono::Utc::now());
                    job.error = Some(message);
                    true
                }
            }),
        )
        .await;
}

/// `finalize_with_success` 的结果，供 handler 决定 emit 哪个进度事件。
pub enum FinalizeOutcome {
    /// 成功落 Completed + result。
    Completed,
    /// 已被并发 cancel：保留 Cancelled，未写 result。
    Cancelled,
    /// job 不见了（罕见：被删 / 文件消失）。
    Vanished,
}

/// 任务成功收口。CAS：仅当 job 仍非终态时才落 Completed + result。
/// 若已被并发 cancel，则保留 Cancelled、不写 result，返回 [`FinalizeOutcome::Cancelled`]，
/// 关闭「stream 收尾把用户的取消悄悄复活成 Completed」这个竞态。
pub async fn finalize_with_success(
    repo: &Arc<dyn JobRepository>,
    id: &JobId,
    result: serde_json::Value,
) -> FinalizeOutcome {
    match repo
        .modify(
            id,
            Box::new(move |job| {
                if job.status.is_terminal() {
                    false
                } else {
                    job.status = JobStatus::Completed;
                    job.completed_at = Some(chrono::Utc::now());
                    job.result = Some(result);
                    true
                }
            }),
        )
        .await
    {
        Ok(job) if matches!(job.status, JobStatus::Cancelled) => FinalizeOutcome::Cancelled,
        Ok(_) => FinalizeOutcome::Completed,
        Err(_) => FinalizeOutcome::Vanished,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::domain::{Job, JobKind};
    use crate::platform::infra::FileJobRepository;

    fn repo(td: &tempfile::TempDir) -> Arc<dyn JobRepository> {
        Arc::new(FileJobRepository::new(td.path().join("history")))
    }

    #[tokio::test]
    async fn finalize_with_success_does_not_overwrite_cancelled() {
        let td = tempfile::TempDir::new().unwrap();
        let repo = repo(&td);
        let mut job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();
        // 模拟并发 cancel：先把 job 置 Cancelled。
        job.status = JobStatus::Cancelled;
        job.completed_at = Some(chrono::Utc::now());
        repo.update(&job).await.unwrap();

        // handler 收尾尝试 Completed —— 必须被 CAS 挡住。
        let outcome =
            finalize_with_success(&repo, &job.id, serde_json::json!({"content": "x"})).await;
        assert!(matches!(outcome, FinalizeOutcome::Cancelled));
        let reloaded = repo.get(&job.id).await.unwrap();
        assert_eq!(reloaded.status, JobStatus::Cancelled);
        assert!(
            reloaded.result.is_none(),
            "cancelled job must not receive a result"
        );
    }

    #[tokio::test]
    async fn finalize_with_success_completes_running_job() {
        let td = tempfile::TempDir::new().unwrap();
        let repo = repo(&td);
        let mut job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();
        job.status = JobStatus::Running;
        repo.update(&job).await.unwrap();

        let outcome =
            finalize_with_success(&repo, &job.id, serde_json::json!({"content": "ok"})).await;
        assert!(matches!(outcome, FinalizeOutcome::Completed));
        let reloaded = repo.get(&job.id).await.unwrap();
        assert_eq!(reloaded.status, JobStatus::Completed);
        assert_eq!(reloaded.result.unwrap()["content"], "ok");
    }

    #[tokio::test]
    async fn transition_to_running_refuses_cancelled_job() {
        let td = tempfile::TempDir::new().unwrap();
        let repo = repo(&td);
        let sink: Arc<dyn ProgressSink> = Arc::new(NoopProgressSink);
        let mut job = Job::new(JobKind::TextGenerate, serde_json::json!({}));
        repo.create(&job).await.unwrap();
        job.status = JobStatus::Cancelled;
        repo.update(&job).await.unwrap();

        let err = transition_to_running(&repo, &job.id, &sink).await.unwrap_err();
        assert!(matches!(err, JobError::Terminal { .. }));
        // 仍是 Cancelled，未被复活成 Running。
        assert_eq!(repo.get(&job.id).await.unwrap().status, JobStatus::Cancelled);
    }
}
