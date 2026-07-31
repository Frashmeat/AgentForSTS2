//! Shared progress events and Run lifecycle helpers.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::platform::domain::{
    ActionableFailure, RunId, RunProgress, RunRepository, RunRepositoryResult, RunResult,
    RunStatus, RunTransition,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressEvent {
    pub run_id: RunId,
    pub stage: String,
    pub percent: Option<f32>,
    pub message: Option<String>,
    pub delta: Option<String>,
}

#[async_trait]
pub trait ProgressSink: Send + Sync {
    async fn emit(&self, event: ProgressEvent);
}

pub struct NoopProgressSink;

#[async_trait]
impl ProgressSink for NoopProgressSink {
    async fn emit(&self, _: ProgressEvent) {}
}

pub async fn transition_to_running(
    repo: &Arc<dyn RunRepository>,
    id: &RunId,
    sink: &Arc<dyn ProgressSink>,
) -> RunRepositoryResult<()> {
    repo.transition(id, RunTransition::Start).await?;
    repo.update_progress(
        id,
        RunProgress {
            stage: "running".into(),
            percent: Some(0.0),
            message: None,
        },
    )
    .await?;
    sink.emit(ProgressEvent {
        run_id: id.clone(),
        stage: "running".into(),
        percent: Some(0.0),
        message: None,
        delta: None,
    })
    .await;
    Ok(())
}

pub async fn finalize_with_failure(
    repo: &Arc<dyn RunRepository>,
    id: &RunId,
    sink: &Arc<dyn ProgressSink>,
    failure: ActionableFailure,
) {
    sink.emit(ProgressEvent {
        run_id: id.clone(),
        stage: "failed".into(),
        percent: None,
        message: Some(failure.message.clone()),
        delta: None,
    })
    .await;
    let _ = repo.transition(id, RunTransition::Fail { failure }).await;
}

pub enum FinalizeOutcome {
    Succeeded,
    Cancelled,
    Vanished,
}

pub async fn finalize_with_success(
    repo: &Arc<dyn RunRepository>,
    id: &RunId,
    result: RunResult,
) -> FinalizeOutcome {
    match repo.transition(id, RunTransition::Succeed { result }).await {
        Ok(_) => FinalizeOutcome::Succeeded,
        Err(_) => match repo.get(id).await {
            Ok(run) if run.status == RunStatus::Cancelled => FinalizeOutcome::Cancelled,
            _ => FinalizeOutcome::Vanished,
        },
    }
}

pub async fn is_cancelled(repo: &Arc<dyn RunRepository>, id: &RunId) -> bool {
    matches!(
        repo.get(id).await.ok().map(|run| run.status),
        Some(RunStatus::Cancelled)
    )
}

pub async fn emit_cancelled_mid_stream(sink: &Arc<dyn ProgressSink>, run_id: &RunId) {
    sink.emit(ProgressEvent {
        run_id: run_id.clone(),
        stage: "cancelled-mid-stream".into(),
        percent: None,
        message: Some("run cancelled; dropping stream".into()),
        delta: None,
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::domain::{CancellationReason, RunKind, RunRecord, TokenUsage};
    use crate::platform::infra::FileRunRepository;

    fn repo(td: &tempfile::TempDir) -> Arc<dyn RunRepository> {
        Arc::new(FileRunRepository::new(td.path().join("history")))
    }

    fn text_result(content: &str) -> RunResult {
        RunResult::TextGeneration {
            model: "fixture".into(),
            content: content.into(),
            finish_reason: "end_turn".into(),
            usage: TokenUsage::default(),
        }
    }

    #[tokio::test]
    async fn success_does_not_overwrite_cancelled() {
        let td = tempfile::TempDir::new().unwrap();
        let repo = repo(&td);
        let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        repo.create(&run).await.unwrap();
        repo.transition(
            &run.id,
            RunTransition::Cancel {
                reason: CancellationReason::User,
            },
        )
        .await
        .unwrap();

        let outcome = finalize_with_success(&repo, &run.id, text_result("x")).await;
        assert!(matches!(outcome, FinalizeOutcome::Cancelled));
        let reloaded = repo.get(&run.id).await.unwrap();
        assert_eq!(reloaded.status, RunStatus::Cancelled);
        assert!(reloaded.result.is_none());
    }

    #[tokio::test]
    async fn success_completes_running_run() {
        let td = tempfile::TempDir::new().unwrap();
        let repo = repo(&td);
        let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        repo.create(&run).await.unwrap();
        repo.transition(&run.id, RunTransition::Start)
            .await
            .unwrap();

        let outcome = finalize_with_success(&repo, &run.id, text_result("ok")).await;
        assert!(matches!(outcome, FinalizeOutcome::Succeeded));
        assert_eq!(
            repo.get(&run.id).await.unwrap().status,
            RunStatus::Succeeded
        );
    }

    #[tokio::test]
    async fn start_refuses_cancelled_run() {
        let td = tempfile::TempDir::new().unwrap();
        let repo = repo(&td);
        let sink: Arc<dyn ProgressSink> = Arc::new(NoopProgressSink);
        let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        repo.create(&run).await.unwrap();
        repo.transition(
            &run.id,
            RunTransition::Cancel {
                reason: CancellationReason::User,
            },
        )
        .await
        .unwrap();

        assert!(transition_to_running(&repo, &run.id, &sink).await.is_err());
        assert_eq!(
            repo.get(&run.id).await.unwrap().status,
            RunStatus::Cancelled
        );
    }
}
