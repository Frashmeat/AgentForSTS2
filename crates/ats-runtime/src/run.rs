use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use ats_kernel::{FailureCode, FeatureId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::VersionedPayload;

pub const RUN_RECORD_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Eq, PartialEq, Hash)]
#[serde(transparent)]
pub struct RunId(String);

impl RunId {
    #[must_use]
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        Self(format!("run-{nanos:032x}-{counter:08x}"))
    }

    pub fn parse(value: impl Into<String>) -> Result<Self, RunLifecycleError> {
        let value = value.into();
        if value.len() > "run-".len()
            && value.starts_with("run-")
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            Ok(Self(value))
        } else {
            Err(RunLifecycleError::InvalidRunId)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
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

impl<'de> Deserialize<'de> for RunId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunTimelineEvent {
    pub kind: RunTimelineEventKind,
    pub at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<FailureCode>,
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

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunFailure {
    pub code: FailureCode,
    pub stage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<VersionedPayload>,
}

impl<'de> Deserialize<'de> for RunFailure {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            code: FailureCode,
            stage: String,
            #[serde(default)]
            details: Option<VersionedPayload>,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.code, wire.stage, wire.details).map_err(serde::de::Error::custom)
    }
}

impl RunFailure {
    pub fn new(
        code: FailureCode,
        stage: impl Into<String>,
        details: Option<VersionedPayload>,
    ) -> Result<Self, RunLifecycleError> {
        let failure = Self {
            code,
            stage: stage.into(),
            details,
        };
        failure.validate()?;
        Ok(failure)
    }

    fn validate(&self) -> Result<(), RunLifecycleError> {
        if self.stage.is_empty()
            || self.stage.len() > 128
            || !self.stage.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return Err(RunLifecycleError::InvalidFailureStage);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum RunTransition {
    Start,
    Cancel { reason: CancellationReason },
    Succeed { result: VersionedPayload },
    Fail { failure: RunFailure },
    Interrupt { failure: RunFailure },
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum RunLifecycleError {
    #[error("run ID is not a safe segment")]
    InvalidRunId,
    #[error("run failure stage is invalid")]
    InvalidFailureStage,
    #[error("run transition is invalid for the current status")]
    InvalidTransition,
    #[error("run record violates lifecycle invariants")]
    InvalidRecord,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    schema_version: u32,
    id: RunId,
    feature_id: FeatureId,
    status: RunStatus,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    request: VersionedPayload,
    progress: Option<RunProgress>,
    failure: Option<RunFailure>,
    result: Option<VersionedPayload>,
    attempts: u32,
    timeline: Vec<RunTimelineEvent>,
}

impl RunRecord {
    #[must_use]
    pub fn new(feature_id: FeatureId, request: VersionedPayload) -> Self {
        let created_at = Utc::now();
        Self {
            schema_version: RUN_RECORD_SCHEMA_VERSION,
            id: RunId::new(),
            feature_id,
            status: RunStatus::Pending,
            created_at,
            started_at: None,
            completed_at: None,
            request,
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
    ) -> Result<(), RunLifecycleError> {
        let mut next = self.clone();
        next.apply_transition_in_place(transition, at)?;
        next.validate()?;
        *self = next;
        Ok(())
    }

    fn apply_transition_in_place(
        &mut self,
        transition: RunTransition,
        at: DateTime<Utc>,
    ) -> Result<(), RunLifecycleError> {
        if self.status.is_terminal() || at < self.created_at {
            return Err(RunLifecycleError::InvalidTransition);
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
                self.status = RunStatus::Succeeded;
                self.completed_at = Some(at);
                self.failure = None;
                self.result = Some(result);
                self.timeline
                    .push(RunTimelineEvent::new(RunTimelineEventKind::Succeeded, at));
            }
            RunTransition::Fail { failure } if self.status == RunStatus::Running => {
                failure.validate()?;
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
                if matches!(self.status, RunStatus::Pending | RunStatus::Running)
                    && failure.code.as_str() == "run.interrupted" =>
            {
                failure.validate()?;
                let mut event = RunTimelineEvent::new(RunTimelineEventKind::Interrupted, at);
                event.stage = Some(failure.stage.clone());
                event.failure_code = Some(failure.code.clone());
                self.status = RunStatus::Failed;
                self.completed_at = Some(at);
                self.failure = Some(failure);
                self.result = None;
                self.timeline.push(event);
            }
            _ => return Err(RunLifecycleError::InvalidTransition),
        }
        Ok(())
    }

    pub fn set_progress(&mut self, progress: RunProgress) -> Result<(), RunLifecycleError> {
        if self.status != RunStatus::Running || !valid_progress(&progress) {
            return Err(RunLifecycleError::InvalidTransition);
        }
        let mut next = self.clone();
        next.progress = Some(progress);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), RunLifecycleError> {
        if self.schema_version != RUN_RECORD_SCHEMA_VERSION
            || !valid_progress_option(self.progress.as_ref())
            || self.timeline.first().map(|event| event.kind) != Some(RunTimelineEventKind::Created)
            || self.timeline.first().map(|event| event.at) != Some(self.created_at)
            || self
                .timeline
                .iter()
                .filter(|event| event.kind == RunTimelineEventKind::Created)
                .count()
                != 1
            || self
                .timeline
                .iter()
                .filter(|event| event.kind.is_terminal())
                .count()
                > 1
        {
            return Err(RunLifecycleError::InvalidRecord);
        }
        if !self
            .timeline
            .windows(2)
            .all(|events| events[0].at <= events[1].at)
            || !self.timeline.iter().all(valid_timeline_event)
        {
            return Err(RunLifecycleError::InvalidRecord);
        }
        if self.status == RunStatus::Pending && self.started_at.is_some() {
            return Err(RunLifecycleError::InvalidRecord);
        }
        let pending_interrupted = self.status == RunStatus::Failed
            && self.started_at.is_none()
            && self
                .failure
                .as_ref()
                .is_some_and(|failure| failure.code.as_str() == "run.interrupted");
        if matches!(
            self.status,
            RunStatus::Running | RunStatus::Succeeded | RunStatus::Failed
        ) && self.started_at.is_none()
            && !pending_interrupted
        {
            return Err(RunLifecycleError::InvalidRecord);
        }
        if self.status.is_terminal() != self.completed_at.is_some() {
            return Err(RunLifecycleError::InvalidRecord);
        }
        let started_events = self
            .timeline
            .iter()
            .filter(|event| event.kind == RunTimelineEventKind::Started)
            .count();
        if self.started_at.is_some() != (started_events == 1)
            || self.attempts != u32::from(self.started_at.is_some())
            || self.started_at.is_some_and(|started_at| {
                started_at < self.created_at
                    || !self.timeline.iter().any(|event| {
                        event.kind == RunTimelineEventKind::Started && event.at == started_at
                    })
            })
            || self.completed_at.is_some_and(|completed_at| {
                completed_at < self.created_at
                    || self
                        .started_at
                        .is_some_and(|started_at| completed_at < started_at)
            })
        {
            return Err(RunLifecycleError::InvalidRecord);
        }
        match self.status {
            RunStatus::Succeeded if self.failure.is_none() && self.result.is_some() => {}
            RunStatus::Failed if self.failure.is_some() && self.result.is_none() => {}
            RunStatus::Cancelled if self.failure.is_none() && self.result.is_none() => {}
            RunStatus::Pending | RunStatus::Running
                if self.failure.is_none()
                    && self.result.is_none()
                    && self.completed_at.is_none() => {}
            _ => return Err(RunLifecycleError::InvalidRecord),
        }
        if let Some(failure) = &self.failure {
            failure.validate()?;
        }
        if !terminal_event_matches(self) {
            return Err(RunLifecycleError::InvalidRecord);
        }
        Ok(())
    }

    #[must_use]
    pub fn id(&self) -> &RunId {
        &self.id
    }

    #[must_use]
    pub fn feature_id(&self) -> &FeatureId {
        &self.feature_id
    }

    #[must_use]
    pub fn request(&self) -> &VersionedPayload {
        &self.request
    }

    #[must_use]
    pub fn result(&self) -> Option<&VersionedPayload> {
        self.result.as_ref()
    }

    #[must_use]
    pub fn failure(&self) -> Option<&RunFailure> {
        self.failure.as_ref()
    }

    #[must_use]
    pub fn status(&self) -> RunStatus {
        self.status
    }
}

fn valid_progress_option(progress: Option<&RunProgress>) -> bool {
    progress.is_none_or(valid_progress)
}

fn valid_progress(progress: &RunProgress) -> bool {
    !progress.stage.is_empty()
        && progress.stage.len() <= 128
        && progress
            .message
            .as_ref()
            .is_none_or(|message| message.len() <= 512)
        && progress
            .percent
            .is_none_or(|percent| percent.is_finite() && (0.0..=100.0).contains(&percent))
}

fn valid_timeline_event(event: &RunTimelineEvent) -> bool {
    match event.kind {
        RunTimelineEventKind::Created | RunTimelineEventKind::Started => {
            event.stage.is_none()
                && event.failure_code.is_none()
                && event.cancellation_reason.is_none()
        }
        RunTimelineEventKind::CancelRequested | RunTimelineEventKind::Cancelled => {
            event.stage.is_none()
                && event.failure_code.is_none()
                && event.cancellation_reason.is_some()
        }
        RunTimelineEventKind::Succeeded => {
            event.stage.is_none()
                && event.failure_code.is_none()
                && event.cancellation_reason.is_none()
        }
        RunTimelineEventKind::Failed | RunTimelineEventKind::Interrupted => {
            event.stage.is_some()
                && event.failure_code.is_some()
                && event.cancellation_reason.is_none()
        }
    }
}

fn terminal_event_matches(run: &RunRecord) -> bool {
    let terminal = run.timeline.iter().find(|event| event.kind.is_terminal());
    match run.status {
        RunStatus::Pending | RunStatus::Running => terminal.is_none(),
        RunStatus::Succeeded => terminal.is_some_and(|event| {
            event.kind == RunTimelineEventKind::Succeeded && Some(event.at) == run.completed_at
        }),
        RunStatus::Cancelled => terminal.is_some_and(|event| {
            event.kind == RunTimelineEventKind::Cancelled && Some(event.at) == run.completed_at
        }),
        RunStatus::Failed => terminal.is_some_and(|event| {
            matches!(
                event.kind,
                RunTimelineEventKind::Failed | RunTimelineEventKind::Interrupted
            ) && Some(event.at) == run.completed_at
                && run.failure.as_ref().is_some_and(|failure| {
                    event.failure_code.as_ref() == Some(&failure.code)
                        && event.stage.as_deref() == Some(failure.stage.as_str())
                        && (event.kind != RunTimelineEventKind::Interrupted
                            || failure.code.as_str() == "run.interrupted")
                })
        }),
    }
}

impl<'de> Deserialize<'de> for RunRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            id: RunId,
            feature_id: FeatureId,
            status: RunStatus,
            created_at: DateTime<Utc>,
            started_at: Option<DateTime<Utc>>,
            completed_at: Option<DateTime<Utc>>,
            request: VersionedPayload,
            progress: Option<RunProgress>,
            failure: Option<RunFailure>,
            result: Option<VersionedPayload>,
            attempts: u32,
            timeline: Vec<RunTimelineEvent>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let record = Self {
            schema_version: wire.schema_version,
            id: wire.id,
            feature_id: wire.feature_id,
            status: wire.status,
            created_at: wire.created_at,
            started_at: wire.started_at,
            completed_at: wire.completed_at,
            request: wire.request,
            progress: wire.progress,
            failure: wire.failure,
            result: wire.result,
            attempts: wire.attempts,
            timeline: wire.timeline,
        };
        record.validate().map_err(serde::de::Error::custom)?;
        Ok(record)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunSummary {
    pub id: RunId,
    pub feature_id: FeatureId,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub progress: Option<RunProgress>,
    pub failure: Option<RunFailure>,
}

impl From<&RunRecord> for RunSummary {
    fn from(run: &RunRecord) -> Self {
        Self {
            id: run.id.clone(),
            feature_id: run.feature_id.clone(),
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
    use serde::{Deserialize, Serialize};

    use ats_kernel::{SchemaId, SchemaRef, SchemaVersion};

    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Fixture {
        value: String,
    }

    fn payload(id: &str) -> VersionedPayload {
        VersionedPayload::from_typed(
            SchemaRef {
                id: SchemaId::parse(id).unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
            &Fixture { value: "ok".into() },
        )
        .unwrap()
    }

    fn run() -> RunRecord {
        RunRecord::new(
            FeatureId::parse("fixture.generate").unwrap(),
            payload("fixture.request"),
        )
    }

    #[test]
    fn succeeds_without_a_runtime_product_result_enum() {
        let mut run = run();
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        run.apply_transition(
            RunTransition::Succeed {
                result: payload("fixture.result"),
            },
            Utc::now(),
        )
        .unwrap();

        assert_eq!(run.status(), RunStatus::Succeeded);
        assert_eq!(run.result().unwrap().schema().id.as_str(), "fixture.result");
        assert!(run.failure().is_none());
        let json = serde_json::to_string(&run).unwrap();
        assert_eq!(serde_json::from_str::<RunRecord>(&json).unwrap(), run);
    }

    #[test]
    fn invalid_transition_does_not_mutate_pending_record() {
        let mut run = run();
        let error = run
            .apply_transition(
                RunTransition::Succeed {
                    result: payload("fixture.result"),
                },
                Utc::now(),
            )
            .unwrap_err();
        assert_eq!(error, RunLifecycleError::InvalidTransition);
        assert_eq!(run.status(), RunStatus::Pending);
        assert!(run.result().is_none());
    }

    #[test]
    fn cancellation_and_interruption_preserve_terminal_invariants() {
        let mut cancelled = run();
        cancelled
            .apply_transition(
                RunTransition::Cancel {
                    reason: CancellationReason::User,
                },
                Utc::now(),
            )
            .unwrap();
        assert_eq!(cancelled.status(), RunStatus::Cancelled);

        let mut interrupted = run();
        interrupted
            .apply_transition(
                RunTransition::Interrupt {
                    failure: RunFailure::new(
                        FailureCode::parse("run.interrupted").unwrap(),
                        "run.reconcile",
                        None,
                    )
                    .unwrap(),
                },
                Utc::now(),
            )
            .unwrap();
        assert_eq!(interrupted.status(), RunStatus::Failed);
        assert!(interrupted.validate().is_ok());
    }

    #[test]
    fn deserialization_rejects_tampered_terminal_state() {
        let mut value = serde_json::to_value(run()).unwrap();
        value["status"] = serde_json::json!("succeeded");
        assert!(serde_json::from_value::<RunRecord>(value).is_err());
    }

    #[test]
    fn rejects_empty_run_id_and_invalid_failure_deserialization() {
        assert_eq!(RunId::parse("run-"), Err(RunLifecycleError::InvalidRunId));
        assert!(
            serde_json::from_str::<RunFailure>(r#"{"code":"run.invalid","stage":"Bad Stage"}"#)
                .is_err()
        );
    }

    #[test]
    fn invalid_transition_is_transactional() {
        let mut run = run();
        let before = serde_json::to_value(&run).unwrap();
        let before_created = run.created_at;
        assert_eq!(
            run.apply_transition(
                RunTransition::Start,
                before_created - chrono::Duration::seconds(1),
            ),
            Err(RunLifecycleError::InvalidTransition)
        );
        assert_eq!(serde_json::to_value(&run).unwrap(), before);
    }
}
