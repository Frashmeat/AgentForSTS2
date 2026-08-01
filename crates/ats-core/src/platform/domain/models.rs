//! Run 领域模型和持久化不变量。

use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::failure::ActionableFailure;
use crate::planning::PlanItem;

pub const RUN_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct RunId(pub String);

impl RunId {
    #[must_use]
    pub fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        Self(format!("run-{nanos:032x}-{counter:08x}"))
    }

    #[must_use]
    pub fn is_safe_segment(&self) -> bool {
        self.0.starts_with("run-")
            && !self.0.is_empty()
            && self
                .0
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    TextGenerate,
    CodeGenerate,
    AssetGenerate,
    BatchCustomCode,
    BuildProject,
    PackageProject,
    SingleAssetPlan,
    LogAnalysis,
    TruthSnapshotRefresh,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl RunStatus {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunProgress {
    pub stage: String,
    pub percent: Option<f32>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CancellationReason {
    User,
    ProjectClose,
    ProjectSwitch,
    AppShutdown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RunTimelineEventKind {
    Created,
    Started,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl RunTimelineEventKind {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunTimelineEvent {
    pub kind: RunTimelineEventKind,
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancellation_reason: Option<CancellationReason>,
}

impl RunTimelineEvent {
    fn new(kind: RunTimelineEventKind, at: DateTime<Utc>) -> Self {
        Self {
            kind,
            at,
            stage: None,
            failure_code: None,
            cancellation_reason: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BatchArtifactItemResult {
    pub item_id: String,
    pub artifact_manifest_ref: Option<String>,
    pub manifest_sha256: Option<String>,
    pub diagnostic_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BuildStepResult {
    pub id: String,
    pub runner: String,
    pub success: bool,
    pub exit_code: i32,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum RunResult {
    TextGeneration {
        model: String,
        content: String,
        finish_reason: String,
        usage: TokenUsage,
    },
    ArtifactProduction {
        artifact_manifest_ref: String,
        manifest_sha256: String,
        artifact_id: String,
        entity_name: String,
        model: Option<String>,
        usage: Option<TokenUsage>,
    },
    BatchArtifactProduction {
        total: usize,
        succeeded: usize,
        failed: usize,
        items: Vec<BatchArtifactItemResult>,
    },
    Build {
        project_relative_root: String,
        steps: Vec<BuildStepResult>,
        artifact_manifest_ref: Option<String>,
        manifest_sha256: Option<String>,
    },
    Package {
        artifact_manifest_ref: String,
        manifest_sha256: String,
        artifact_id: String,
        files_added: usize,
        uncompressed_bytes: u64,
        package_bytes: u64,
    },
    Plan {
        item: PlanItem,
        item_file_ref: Option<String>,
        model: String,
        usage: TokenUsage,
    },
    LogAnalysis {
        model: String,
        report: String,
        log_chars: usize,
        truncated_chars: usize,
        usage: TokenUsage,
    },
    TruthSnapshotRefresh {
        game_pack_id: String,
        snapshot_id: String,
        cache_hit: bool,
        source_count: usize,
        index_count: usize,
        tool_versions: BTreeMap<String, String>,
        warnings: Vec<String>,
    },
}

impl RunResult {
    #[must_use]
    pub fn supports(&self, run_kind: RunKind) -> bool {
        matches!(
            (run_kind, self),
            (RunKind::TextGenerate, Self::TextGeneration { .. })
                | (
                    RunKind::CodeGenerate | RunKind::AssetGenerate,
                    Self::ArtifactProduction { .. }
                )
                | (
                    RunKind::BatchCustomCode,
                    Self::BatchArtifactProduction { .. }
                )
                | (RunKind::BuildProject, Self::Build { .. })
                | (RunKind::PackageProject, Self::Package { .. })
                | (RunKind::SingleAssetPlan, Self::Plan { .. })
                | (RunKind::LogAnalysis, Self::LogAnalysis { .. })
                | (
                    RunKind::TruthSnapshotRefresh,
                    Self::TruthSnapshotRefresh { .. }
                )
        )
    }
}

#[derive(Debug, Clone)]
pub enum RunTransition {
    Start,
    Cancel { reason: CancellationReason },
    Succeed { result: RunResult },
    Fail { failure: ActionableFailure },
    Interrupt { failure: ActionableFailure },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub schema_version: u32,
    pub id: RunId,
    pub kind: RunKind,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub payload: serde_json::Value,
    pub progress: Option<RunProgress>,
    pub failure: Option<ActionableFailure>,
    pub result: Option<RunResult>,
    pub attempts: u32,
    pub timeline: Vec<RunTimelineEvent>,
}

impl RunRecord {
    #[must_use]
    pub fn new(kind: RunKind, payload: serde_json::Value) -> Self {
        let created_at = Utc::now();
        Self {
            schema_version: RUN_SCHEMA_VERSION,
            id: RunId::new(),
            kind,
            status: RunStatus::Pending,
            created_at,
            started_at: None,
            completed_at: None,
            payload,
            progress: None,
            failure: None,
            result: None,
            attempts: 0,
            timeline: vec![RunTimelineEvent::new(
                RunTimelineEventKind::Created,
                created_at,
            )],
        }
    }

    pub fn apply_transition(
        &mut self,
        transition: RunTransition,
        at: DateTime<Utc>,
    ) -> Result<(), String> {
        if self.status.is_terminal() {
            return Err(format!("run is already terminal ({:?})", self.status));
        }

        match transition {
            RunTransition::Start if self.status == RunStatus::Pending => {
                self.status = RunStatus::Running;
                self.started_at = Some(at);
                self.attempts = self.attempts.saturating_add(1);
                self.timeline
                    .push(RunTimelineEvent::new(RunTimelineEventKind::Started, at));
            }
            RunTransition::Cancel { reason }
                if matches!(self.status, RunStatus::Pending | RunStatus::Running) =>
            {
                let mut requested =
                    RunTimelineEvent::new(RunTimelineEventKind::CancelRequested, at);
                requested.cancellation_reason = Some(reason);
                self.timeline.push(requested);
                let mut cancelled = RunTimelineEvent::new(RunTimelineEventKind::Cancelled, at);
                cancelled.cancellation_reason = Some(reason);
                self.timeline.push(cancelled);
                self.status = RunStatus::Cancelled;
                self.completed_at = Some(at);
                self.failure = None;
                self.result = None;
            }
            RunTransition::Succeed { result } if self.status == RunStatus::Running => {
                if !result.supports(self.kind) {
                    return Err(format!(
                        "result variant does not match run kind {:?}",
                        self.kind
                    ));
                }
                self.status = RunStatus::Succeeded;
                self.completed_at = Some(at);
                self.failure = None;
                self.result = Some(result);
                self.timeline
                    .push(RunTimelineEvent::new(RunTimelineEventKind::Succeeded, at));
            }
            RunTransition::Fail { failure } if self.status == RunStatus::Running => {
                let mut event = RunTimelineEvent::new(RunTimelineEventKind::Failed, at);
                event.stage = Some(failure.stage.clone());
                event.failure_code = Some(failure.code.clone());
                self.status = RunStatus::Failed;
                self.completed_at = Some(at);
                self.failure = Some(failure);
                self.result = None;
                self.timeline.push(event);
            }
            RunTransition::Interrupt { failure }
                if matches!(self.status, RunStatus::Pending | RunStatus::Running) =>
            {
                if failure.code != "run.interrupted" {
                    return Err("interrupted transition requires run.interrupted failure".into());
                }
                let mut event = RunTimelineEvent::new(RunTimelineEventKind::Interrupted, at);
                event.stage = Some(failure.stage.clone());
                event.failure_code = Some(failure.code.clone());
                self.status = RunStatus::Failed;
                self.completed_at = Some(at);
                self.failure = Some(failure);
                self.result = None;
                self.timeline.push(event);
            }
            _ => {
                return Err(format!(
                    "invalid transition from run status {:?}",
                    self.status
                ));
            }
        }
        self.validate()
    }

    pub fn set_progress(&mut self, progress: RunProgress) -> Result<(), String> {
        if self.status != RunStatus::Running {
            return Err(format!(
                "progress requires running status, got {:?}",
                self.status
            ));
        }
        self.progress = Some(progress);
        self.validate()
    }

    #[must_use]
    pub fn error_message(&self) -> Option<&str> {
        self.failure
            .as_ref()
            .map(|failure| failure.message.as_str())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != RUN_SCHEMA_VERSION {
            return Err(format!(
                "unsupported run schema version {}",
                self.schema_version
            ));
        }
        if !self.id.is_safe_segment() {
            return Err(format!("unsafe run id: {}", self.id));
        }
        if !self.payload.is_object() {
            return Err("run payload must be a JSON object".into());
        }
        if self.timeline.first().map(|event| event.kind) != Some(RunTimelineEventKind::Created)
            || self
                .timeline
                .iter()
                .filter(|event| event.kind == RunTimelineEventKind::Created)
                .count()
                != 1
        {
            return Err("run timeline must contain exactly one leading created event".into());
        }
        if self
            .timeline
            .iter()
            .filter(|event| event.kind.is_terminal())
            .count()
            > 1
        {
            return Err("run timeline contains more than one terminal event".into());
        }
        if matches!(self.status, RunStatus::Pending) && self.started_at.is_some() {
            return Err("pending run must not have startedAt".into());
        }
        let pending_interrupted = self.status == RunStatus::Failed
            && self.started_at.is_none()
            && self
                .failure
                .as_ref()
                .is_some_and(|failure| failure.code == "run.interrupted");
        if matches!(
            self.status,
            RunStatus::Running | RunStatus::Succeeded | RunStatus::Failed
        ) && self.started_at.is_none()
            && !pending_interrupted
        {
            return Err("started run must have startedAt".into());
        }
        if self.status.is_terminal() != self.completed_at.is_some() {
            return Err("completedAt must exist exactly for terminal runs".into());
        }
        match self.status {
            RunStatus::Succeeded => match (&self.failure, &self.result) {
                (None, Some(result)) if result.supports(self.kind) => {}
                _ => return Err("succeeded run requires matching result and no failure".into()),
            },
            RunStatus::Failed => {
                if self.failure.is_none() || self.result.is_some() {
                    return Err("failed run requires failure and no result".into());
                }
            }
            RunStatus::Cancelled => {
                if self.failure.is_some() || self.result.is_some() {
                    return Err("cancelled run must not have failure or result".into());
                }
            }
            RunStatus::Pending | RunStatus::Running => {
                if self.failure.is_some() || self.result.is_some() || self.completed_at.is_some() {
                    return Err("non-terminal run must not have terminal fields".into());
                }
            }
        }
        if let Some(failure) = &self.failure {
            failure.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: RunId,
    pub kind: RunKind,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub progress: Option<RunProgress>,
    pub failure: Option<ActionableFailure>,
}

impl From<&RunRecord> for RunSummary {
    fn from(run: &RunRecord) -> Self {
        Self {
            id: run.id.clone(),
            kind: run.kind,
            status: run.status,
            created_at: run.created_at,
            completed_at: run.completed_at,
            progress: run.progress.clone(),
            failure: run.failure.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{FailureDiagnostic, MAX_FAILURE_MESSAGE_CHARS};

    fn text_result() -> RunResult {
        RunResult::TextGeneration {
            model: "fixture".into(),
            content: "ok".into(),
            finish_reason: "end_turn".into(),
            usage: TokenUsage::default(),
        }
    }

    #[test]
    fn lifecycle_appends_exactly_one_terminal_event() {
        let mut run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({"prompt":"x"}));
        let started = Utc::now();
        run.apply_transition(RunTransition::Start, started).unwrap();
        run.apply_transition(
            RunTransition::Succeed {
                result: text_result(),
            },
            Utc::now(),
        )
        .unwrap();

        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(
            run.timeline
                .iter()
                .filter(|event| event.kind.is_terminal())
                .count(),
            1
        );
        assert!(run.validate().is_ok());
    }

    #[test]
    fn cancelled_run_has_no_failure_or_result() {
        let mut run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({"prompt":"x"}));
        run.apply_transition(
            RunTransition::Cancel {
                reason: CancellationReason::User,
            },
            Utc::now(),
        )
        .unwrap();

        assert_eq!(run.status, RunStatus::Cancelled);
        assert!(run.failure.is_none());
        assert!(run.result.is_none());
        assert_eq!(run.timeline.len(), 3);
    }

    #[test]
    fn mismatched_result_is_rejected_before_terminal_state() {
        let mut run = RunRecord::new(RunKind::PackageProject, serde_json::json!({}));
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        let error = run
            .apply_transition(
                RunTransition::Succeed {
                    result: text_result(),
                },
                Utc::now(),
            )
            .unwrap_err();

        assert!(error.contains("does not match"));
        assert_eq!(run.status, RunStatus::Running);
    }

    #[test]
    fn failure_diagnostic_id_must_be_a_safe_segment() {
        let mut run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        let mut failure = ActionableFailure::unclassified("stream");
        failure.diagnostic = Some(FailureDiagnostic::for_run(&run.id, "trace available"));
        run.apply_transition(RunTransition::Fail { failure }, Utc::now())
            .unwrap();
        assert!(run.validate().is_ok());

        run.failure
            .as_mut()
            .unwrap()
            .diagnostic
            .as_mut()
            .unwrap()
            .id = "../outside".into();
        assert!(run.validate().unwrap_err().contains("diagnostic id"));
    }

    #[test]
    fn failure_message_is_bounded() {
        let failure = ActionableFailure::new(
            "core.test",
            crate::failure::FailureCategory::Internal,
            "stream",
            "x".repeat(3_000),
            crate::failure::RecoveryAction::None,
            false,
        );
        assert_eq!(failure.message.chars().count(), MAX_FAILURE_MESSAGE_CHARS);
    }

    #[test]
    fn pending_run_can_be_reconciled_as_interrupted_without_faking_a_start() {
        let mut run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        run.apply_transition(
            RunTransition::Interrupt {
                failure: ActionableFailure::interrupted("run.reconcile"),
            },
            Utc::now(),
        )
        .unwrap();

        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.started_at.is_none());
        assert_eq!(run.failure.as_ref().unwrap().code, "run.interrupted");
        assert_eq!(
            run.timeline
                .iter()
                .filter(|event| event.kind.is_terminal())
                .count(),
            1
        );
        assert!(run.validate().is_ok());
    }
}
