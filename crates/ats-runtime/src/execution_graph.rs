use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{
    CompositionDraftId, ExecutionGraphId, ExecutionNodeId, FailureCode, FeatureId, Sha256Digest,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{RunId, VersionedPayload};

pub const EXECUTION_GRAPH_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionGraphStatus {
    Running,
    PauseRequested,
    Paused,
    CancelRequested,
    Cancelled,
    CommitPrepared,
    CommitBlocked,
    Succeeded,
}

impl ExecutionGraphStatus {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Succeeded)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionNodeStatus {
    Pending,
    Running,
    Succeeded,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LogicalAttemptOutcome {
    Running,
    Succeeded,
    Failed,
    Interrupted,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionFailure {
    pub code: FailureCode,
    pub stage: String,
}

impl ExecutionFailure {
    pub fn new(code: FailureCode, stage: impl Into<String>) -> Result<Self, ExecutionGraphError> {
        let value = Self {
            code,
            stage: stage.into(),
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), ExecutionGraphError> {
        if valid_role(&self.stage) {
            Ok(())
        } else {
            Err(ExecutionGraphError::InvalidFailure)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogicalNodeAttempt {
    pub run_id: RunId,
    pub ordinal: u32,
    pub started_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    pub outcome: LogicalAttemptOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<ExecutionFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HashedExecutionPayload {
    pub payload: VersionedPayload,
    pub sha256: Sha256Digest,
}

impl HashedExecutionPayload {
    pub fn new(payload: VersionedPayload) -> Result<Self, ExecutionGraphError> {
        let sha256 = hash_json(&payload)?;
        Ok(Self { payload, sha256 })
    }

    pub fn validate(&self) -> Result<(), ExecutionGraphError> {
        if hash_json(&self.payload)? == self.sha256 {
            Ok(())
        } else {
            Err(ExecutionGraphError::PayloadHashMismatch)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionNodeRecord {
    pub node_id: ExecutionNodeId,
    pub role_id: String,
    pub depends_on: Vec<ExecutionNodeId>,
    pub status: ExecutionNodeStatus,
    pub logical_attempts: Vec<LogicalNodeAttempt>,
    pub request_snapshot_hash: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<HashedExecutionPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_failure: Option<ExecutionFailure>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExecutionNodeSpec {
    pub node_id: ExecutionNodeId,
    pub role_id: String,
    pub depends_on: Vec<ExecutionNodeId>,
    pub request_snapshot_hash: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionCommitIntent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<CompositionDraftId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_draft: Option<VersionedPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_payload_sha256: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<ExecutionPublicationIntent>,
    pub validated_content_digest: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionPublicationIntent {
    pub target_id: String,
    pub canonical_payload: VersionedPayload,
    pub payload_sha256: Sha256Digest,
}

impl ExecutionCommitIntent {
    #[must_use]
    pub fn draft(
        draft_id: CompositionDraftId,
        canonical_draft: VersionedPayload,
        draft_payload_sha256: Sha256Digest,
        validated_content_digest: Sha256Digest,
    ) -> Self {
        Self {
            draft_id: Some(draft_id),
            canonical_draft: Some(canonical_draft),
            draft_payload_sha256: Some(draft_payload_sha256),
            publication: None,
            validated_content_digest,
        }
    }

    #[must_use]
    pub fn publication(
        target_id: impl Into<String>,
        canonical_payload: VersionedPayload,
        payload_sha256: Sha256Digest,
        validated_content_digest: Sha256Digest,
    ) -> Self {
        Self {
            draft_id: None,
            canonical_draft: None,
            draft_payload_sha256: None,
            publication: Some(ExecutionPublicationIntent {
                target_id: target_id.into(),
                canonical_payload,
                payload_sha256,
            }),
            validated_content_digest,
        }
    }

    pub fn validate(&self) -> Result<(), ExecutionGraphError> {
        match (
            &self.draft_id,
            &self.canonical_draft,
            &self.draft_payload_sha256,
            &self.publication,
        ) {
            (Some(_), Some(payload), Some(expected), None)
                if hash_json(payload.payload())? == *expected =>
            {
                Ok(())
            }
            (None, None, None, Some(publication))
                if valid_target_id(&publication.target_id)
                    && hash_json(publication.canonical_payload.payload())?
                        == publication.payload_sha256 =>
            {
                Ok(())
            }
            (Some(_), Some(_), Some(_), None) | (None, None, None, Some(_)) => {
                Err(ExecutionGraphError::PayloadHashMismatch)
            }
            _ => Err(ExecutionGraphError::InvalidMetadata),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionGraphRecord {
    schema_version: u32,
    execution_graph_id: ExecutionGraphId,
    owner_feature_id: FeatureId,
    request_snapshot_hash: Sha256Digest,
    revision: u64,
    status: ExecutionGraphStatus,
    active_run_id: Option<RunId>,
    previous_run_id: Option<RunId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    blueprint: HashedExecutionPayload,
    nodes: BTreeMap<ExecutionNodeId, ExecutionNodeRecord>,
    commit_intent: Option<ExecutionCommitIntent>,
    final_result_ref: Option<HashedExecutionPayload>,
}

impl ExecutionGraphRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new_claimed(
        execution_graph_id: ExecutionGraphId,
        owner_feature_id: FeatureId,
        request_snapshot_hash: Sha256Digest,
        blueprint: VersionedPayload,
        node_specs: Vec<ExecutionNodeSpec>,
        run_id: RunId,
        at: DateTime<Utc>,
    ) -> Result<Self, ExecutionGraphError> {
        let mut nodes = BTreeMap::new();
        for spec in node_specs {
            let node_id = spec.node_id.clone();
            if nodes
                .insert(
                    node_id,
                    ExecutionNodeRecord {
                        node_id: spec.node_id,
                        role_id: spec.role_id,
                        depends_on: spec.depends_on,
                        status: ExecutionNodeStatus::Pending,
                        logical_attempts: Vec::new(),
                        request_snapshot_hash: spec.request_snapshot_hash,
                        checkpoint: None,
                        safe_failure: None,
                    },
                )
                .is_some()
            {
                return Err(ExecutionGraphError::DuplicateNode);
            }
        }
        let graph = Self {
            schema_version: EXECUTION_GRAPH_SCHEMA_VERSION,
            execution_graph_id,
            owner_feature_id,
            request_snapshot_hash,
            revision: 1,
            status: ExecutionGraphStatus::Running,
            active_run_id: Some(run_id),
            previous_run_id: None,
            created_at: at,
            updated_at: at,
            blueprint: HashedExecutionPayload::new(blueprint)?,
            nodes,
            commit_intent: None,
            final_result_ref: None,
        };
        graph.validate()?;
        Ok(graph)
    }

    pub fn claim(
        &mut self,
        expected_revision: u64,
        run_id: RunId,
        previous_run_id: RunId,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        if self.revision != expected_revision
            || self.active_run_id.is_some()
            || !matches!(
                self.status,
                ExecutionGraphStatus::Paused | ExecutionGraphStatus::CommitPrepared
            )
        {
            return Err(ExecutionGraphError::Conflict);
        }
        self.mutate(at, |next| {
            if next.status == ExecutionGraphStatus::Paused {
                next.status = ExecutionGraphStatus::Running;
            }
            next.active_run_id = Some(run_id);
            next.previous_run_id = Some(previous_run_id);
            Ok(())
        })
    }

    pub fn start_node(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Running
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let ready = {
                let node = next
                    .nodes
                    .get(node_id)
                    .ok_or(ExecutionGraphError::NodeNotFound)?;
                node.status == ExecutionNodeStatus::Pending
                    && node.depends_on.iter().all(|dependency| {
                        next.nodes
                            .get(dependency)
                            .is_some_and(|value| value.status == ExecutionNodeStatus::Succeeded)
                    })
            };
            if !ready
                || next
                    .nodes
                    .values()
                    .any(|node| node.status == ExecutionNodeStatus::Running)
            {
                return Err(ExecutionGraphError::NodeNotReady);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            let ordinal = u32::try_from(node.logical_attempts.len())
                .ok()
                .and_then(|value| value.checked_add(1))
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            node.status = ExecutionNodeStatus::Running;
            node.safe_failure = None;
            node.logical_attempts.push(LogicalNodeAttempt {
                run_id: run_id.clone(),
                ordinal,
                started_at: at,
                completed_at: None,
                outcome: LogicalAttemptOutcome::Running,
                failure: None,
            });
            Ok(())
        })
    }

    pub fn set_node_request_snapshot_hash(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        request_snapshot_hash: Sha256Digest,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Running
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            if node.status != ExecutionNodeStatus::Pending {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            node.request_snapshot_hash = request_snapshot_hash;
            Ok(())
        })
    }

    pub fn complete_node(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        checkpoint: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Running
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            let attempt = active_attempt_mut(node, run_id)?;
            attempt.outcome = LogicalAttemptOutcome::Succeeded;
            attempt.completed_at = Some(at);
            node.status = ExecutionNodeStatus::Succeeded;
            node.checkpoint = Some(HashedExecutionPayload::new(checkpoint)?);
            node.safe_failure = None;
            Ok(())
        })
    }

    pub fn pause_after_node_failure(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        failure: ExecutionFailure,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        failure.validate()?;
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Running
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            let attempt = active_attempt_mut(node, run_id)?;
            attempt.outcome = LogicalAttemptOutcome::Failed;
            attempt.completed_at = Some(at);
            attempt.failure = Some(failure.clone());
            node.status = ExecutionNodeStatus::Pending;
            node.safe_failure = Some(failure);
            next.status = ExecutionGraphStatus::Paused;
            next.previous_run_id = next.active_run_id.clone();
            next.active_run_id = None;
            Ok(())
        })
    }

    pub fn pause_pending_node(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        failure: ExecutionFailure,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        failure.validate()?;
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Running
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            if node.status != ExecutionNodeStatus::Pending {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            node.safe_failure = Some(failure);
            next.status = ExecutionGraphStatus::Paused;
            next.previous_run_id = next.active_run_id.clone();
            next.active_run_id = None;
            Ok(())
        })
    }

    pub fn pause_interrupted(
        &mut self,
        run_id: &RunId,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.active_run_id.as_ref() != Some(run_id)
                || !matches!(
                    next.status,
                    ExecutionGraphStatus::Running | ExecutionGraphStatus::PauseRequested
                )
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            interrupt_running_node(next, run_id, at)?;
            next.status = ExecutionGraphStatus::Paused;
            next.previous_run_id = next.active_run_id.clone();
            next.active_run_id = None;
            Ok(())
        })
    }

    pub fn request_pause(
        &mut self,
        run_id: &RunId,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Running
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.status = ExecutionGraphStatus::PauseRequested;
            Ok(())
        })
    }

    pub fn prepare_commit(
        &mut self,
        run_id: &RunId,
        intent: ExecutionCommitIntent,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        intent.validate()?;
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Running
                || next.active_run_id.as_ref() != Some(run_id)
                || next
                    .nodes
                    .values()
                    .any(|node| node.status != ExecutionNodeStatus::Succeeded)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.status = ExecutionGraphStatus::CommitPrepared;
            next.commit_intent = Some(intent);
            Ok(())
        })
    }

    pub fn mark_succeeded(
        &mut self,
        run_id: &RunId,
        final_result_ref: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::CommitPrepared
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.final_result_ref = Some(HashedExecutionPayload::new(final_result_ref)?);
            next.status = ExecutionGraphStatus::Succeeded;
            next.previous_run_id = next.active_run_id.clone();
            next.active_run_id = None;
            Ok(())
        })
    }

    pub fn block_commit(
        &mut self,
        run_id: &RunId,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::CommitPrepared
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.status = ExecutionGraphStatus::CommitBlocked;
            next.previous_run_id = next.active_run_id.clone();
            next.active_run_id = None;
            Ok(())
        })
    }

    pub fn cancel(
        &mut self,
        run_id: Option<&RunId>,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if matches!(
                next.status,
                ExecutionGraphStatus::CommitPrepared | ExecutionGraphStatus::Succeeded
            ) || run_id.is_some() && next.active_run_id.as_ref() != run_id
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            for node in next.nodes.values_mut() {
                if node.status == ExecutionNodeStatus::Running
                    && let Some(attempt) = node.logical_attempts.last_mut()
                {
                    attempt.outcome = LogicalAttemptOutcome::Cancelled;
                    attempt.completed_at = Some(at);
                }
                if node.status != ExecutionNodeStatus::Succeeded {
                    node.status = ExecutionNodeStatus::Cancelled;
                }
            }
            next.status = ExecutionGraphStatus::Cancelled;
            next.active_run_id = None;
            Ok(())
        })
    }

    pub fn recover_stale_claim(&mut self, at: DateTime<Utc>) -> Result<(), ExecutionGraphError> {
        let run_id = self
            .active_run_id
            .clone()
            .ok_or(ExecutionGraphError::InvalidTransition)?;
        self.mutate(at, |next| {
            match next.status {
                ExecutionGraphStatus::Running | ExecutionGraphStatus::PauseRequested => {
                    interrupt_running_node(next, &run_id, at)?;
                    next.status = ExecutionGraphStatus::Paused;
                }
                ExecutionGraphStatus::CommitPrepared => {}
                _ => return Err(ExecutionGraphError::InvalidTransition),
            }
            next.previous_run_id = next.active_run_id.clone();
            next.active_run_id = None;
            Ok(())
        })
    }

    fn mutate(
        &mut self,
        at: DateTime<Utc>,
        apply: impl FnOnce(&mut Self) -> Result<(), ExecutionGraphError>,
    ) -> Result<(), ExecutionGraphError> {
        if at < self.updated_at {
            return Err(ExecutionGraphError::InvalidTimestamp);
        }
        let mut next = self.clone();
        apply(&mut next)?;
        next.revision = next
            .revision
            .checked_add(1)
            .ok_or(ExecutionGraphError::InvalidMetadata)?;
        next.updated_at = at;
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ExecutionGraphError> {
        if self.schema_version != EXECUTION_GRAPH_SCHEMA_VERSION
            || self.revision == 0
            || self.nodes.is_empty()
            || self.nodes.len() > 256
            || self.updated_at < self.created_at
        {
            return Err(ExecutionGraphError::InvalidMetadata);
        }
        self.blueprint.validate()?;
        if let Some(intent) = &self.commit_intent {
            intent.validate()?;
        }
        if let Some(result) = &self.final_result_ref {
            result.validate()?;
        }
        validate_nodes(&self.nodes)?;
        match self.status {
            ExecutionGraphStatus::Running | ExecutionGraphStatus::PauseRequested => {
                if self.active_run_id.is_none()
                    || self.commit_intent.is_some()
                    || self.final_result_ref.is_some()
                {
                    return Err(ExecutionGraphError::InvalidState);
                }
            }
            ExecutionGraphStatus::Paused => {
                if self.active_run_id.is_some()
                    || self.commit_intent.is_some()
                    || self.final_result_ref.is_some()
                    || self
                        .nodes
                        .values()
                        .any(|node| node.status == ExecutionNodeStatus::Running)
                {
                    return Err(ExecutionGraphError::InvalidState);
                }
            }
            ExecutionGraphStatus::CommitPrepared => {
                if self.commit_intent.is_none()
                    || self.final_result_ref.is_some()
                    || self
                        .nodes
                        .values()
                        .any(|node| node.status != ExecutionNodeStatus::Succeeded)
                {
                    return Err(ExecutionGraphError::InvalidState);
                }
            }
            ExecutionGraphStatus::CommitBlocked => {
                if self.active_run_id.is_some()
                    || self.commit_intent.is_none()
                    || self.final_result_ref.is_some()
                {
                    return Err(ExecutionGraphError::InvalidState);
                }
            }
            ExecutionGraphStatus::Succeeded => {
                if self.active_run_id.is_some()
                    || self.commit_intent.is_none()
                    || self.final_result_ref.is_none()
                    || self
                        .nodes
                        .values()
                        .any(|node| node.status != ExecutionNodeStatus::Succeeded)
                {
                    return Err(ExecutionGraphError::InvalidState);
                }
            }
            ExecutionGraphStatus::CancelRequested => {
                if self.active_run_id.is_none() || self.commit_intent.is_some() {
                    return Err(ExecutionGraphError::InvalidState);
                }
            }
            ExecutionGraphStatus::Cancelled => {
                if self.active_run_id.is_some() || self.final_result_ref.is_some() {
                    return Err(ExecutionGraphError::InvalidState);
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn id(&self) -> &ExecutionGraphId {
        &self.execution_graph_id
    }

    #[must_use]
    pub fn owner_feature_id(&self) -> &FeatureId {
        &self.owner_feature_id
    }

    #[must_use]
    pub fn request_snapshot_hash(&self) -> &Sha256Digest {
        &self.request_snapshot_hash
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn status(&self) -> ExecutionGraphStatus {
        self.status
    }

    #[must_use]
    pub fn active_run_id(&self) -> Option<&RunId> {
        self.active_run_id.as_ref()
    }

    #[must_use]
    pub fn previous_run_id(&self) -> Option<&RunId> {
        self.previous_run_id.as_ref()
    }

    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    #[must_use]
    pub fn blueprint(&self) -> &HashedExecutionPayload {
        &self.blueprint
    }

    #[must_use]
    pub fn nodes(&self) -> &BTreeMap<ExecutionNodeId, ExecutionNodeRecord> {
        &self.nodes
    }

    #[must_use]
    pub fn commit_intent(&self) -> Option<&ExecutionCommitIntent> {
        self.commit_intent.as_ref()
    }

    #[must_use]
    pub fn final_result_ref(&self) -> Option<&HashedExecutionPayload> {
        self.final_result_ref.as_ref()
    }
}

impl<'de> Deserialize<'de> for ExecutionGraphRecord {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            execution_graph_id: ExecutionGraphId,
            owner_feature_id: FeatureId,
            request_snapshot_hash: Sha256Digest,
            revision: u64,
            status: ExecutionGraphStatus,
            active_run_id: Option<RunId>,
            previous_run_id: Option<RunId>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
            blueprint: HashedExecutionPayload,
            nodes: BTreeMap<ExecutionNodeId, ExecutionNodeRecord>,
            commit_intent: Option<ExecutionCommitIntent>,
            final_result_ref: Option<HashedExecutionPayload>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let graph = Self {
            schema_version: wire.schema_version,
            execution_graph_id: wire.execution_graph_id,
            owner_feature_id: wire.owner_feature_id,
            request_snapshot_hash: wire.request_snapshot_hash,
            revision: wire.revision,
            status: wire.status,
            active_run_id: wire.active_run_id,
            previous_run_id: wire.previous_run_id,
            created_at: wire.created_at,
            updated_at: wire.updated_at,
            blueprint: wire.blueprint,
            nodes: wire.nodes,
            commit_intent: wire.commit_intent,
            final_result_ref: wire.final_result_ref,
        };
        graph.validate().map_err(serde::de::Error::custom)?;
        Ok(graph)
    }
}

fn active_attempt_mut<'a>(
    node: &'a mut ExecutionNodeRecord,
    run_id: &RunId,
) -> Result<&'a mut LogicalNodeAttempt, ExecutionGraphError> {
    if node.status != ExecutionNodeStatus::Running {
        return Err(ExecutionGraphError::InvalidTransition);
    }
    let attempt = node
        .logical_attempts
        .last_mut()
        .ok_or(ExecutionGraphError::InvalidAttempt)?;
    if &attempt.run_id != run_id || attempt.outcome != LogicalAttemptOutcome::Running {
        return Err(ExecutionGraphError::InvalidAttempt);
    }
    Ok(attempt)
}

fn interrupt_running_node(
    graph: &mut ExecutionGraphRecord,
    run_id: &RunId,
    at: DateTime<Utc>,
) -> Result<(), ExecutionGraphError> {
    for node in graph.nodes.values_mut() {
        if node.status == ExecutionNodeStatus::Running {
            let attempt = active_attempt_mut(node, run_id)?;
            attempt.outcome = LogicalAttemptOutcome::Interrupted;
            attempt.completed_at = Some(at);
            node.status = ExecutionNodeStatus::Pending;
        }
    }
    Ok(())
}

fn validate_nodes(
    nodes: &BTreeMap<ExecutionNodeId, ExecutionNodeRecord>,
) -> Result<(), ExecutionGraphError> {
    let mut indegrees = BTreeMap::<ExecutionNodeId, usize>::new();
    let mut dependents = BTreeMap::<ExecutionNodeId, Vec<ExecutionNodeId>>::new();
    for (id, node) in nodes {
        if id != &node.node_id || !valid_role(&node.role_id) {
            return Err(ExecutionGraphError::InvalidNode);
        }
        let unique = node.depends_on.iter().collect::<BTreeSet<_>>();
        if unique.len() != node.depends_on.len()
            || node
                .depends_on
                .iter()
                .any(|dependency| dependency == id || !nodes.contains_key(dependency))
        {
            return Err(ExecutionGraphError::InvalidDependency);
        }
        indegrees.insert(id.clone(), node.depends_on.len());
        for dependency in &node.depends_on {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(id.clone());
        }
        validate_node_state(node)?;
    }
    let mut ready = indegrees
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<BTreeSet<_>>();
    let mut visited = 0_usize;
    while let Some(id) = ready.pop_first() {
        visited += 1;
        for dependent in dependents.get(&id).into_iter().flatten() {
            let count = indegrees
                .get_mut(dependent)
                .ok_or(ExecutionGraphError::InvalidDependency)?;
            *count -= 1;
            if *count == 0 {
                ready.insert(dependent.clone());
            }
        }
    }
    if visited == nodes.len() {
        Ok(())
    } else {
        Err(ExecutionGraphError::CyclicDependency)
    }
}

fn validate_node_state(node: &ExecutionNodeRecord) -> Result<(), ExecutionGraphError> {
    if node.logical_attempts.len() > u32::MAX as usize
        || node
            .logical_attempts
            .iter()
            .enumerate()
            .any(|(index, attempt)| {
                attempt.ordinal != u32::try_from(index + 1).unwrap_or(u32::MAX)
                    || attempt
                        .completed_at
                        .is_some_and(|value| value < attempt.started_at)
                    || (attempt.outcome == LogicalAttemptOutcome::Running)
                        != attempt.completed_at.is_none()
                    || (attempt.outcome == LogicalAttemptOutcome::Failed)
                        != attempt.failure.is_some()
                    || attempt
                        .failure
                        .as_ref()
                        .is_some_and(|value| value.validate().is_err())
            })
    {
        return Err(ExecutionGraphError::InvalidAttempt);
    }
    let running_attempts = node
        .logical_attempts
        .iter()
        .filter(|attempt| attempt.outcome == LogicalAttemptOutcome::Running)
        .count();
    match node.status {
        ExecutionNodeStatus::Pending if node.checkpoint.is_none() && running_attempts == 0 => {}
        ExecutionNodeStatus::Running
            if node.checkpoint.is_none()
                && running_attempts == 1
                && node
                    .logical_attempts
                    .last()
                    .is_some_and(|attempt| attempt.outcome == LogicalAttemptOutcome::Running) => {}
        ExecutionNodeStatus::Succeeded
            if node.checkpoint.is_some()
                && node.safe_failure.is_none()
                && running_attempts == 0
                && node
                    .logical_attempts
                    .last()
                    .is_some_and(|attempt| attempt.outcome == LogicalAttemptOutcome::Succeeded) => {
        }
        ExecutionNodeStatus::Cancelled if running_attempts == 0 => {}
        _ => return Err(ExecutionGraphError::InvalidState),
    }
    if let Some(checkpoint) = &node.checkpoint {
        checkpoint.validate()?;
    }
    if let Some(failure) = &node.safe_failure {
        failure.validate()?;
    }
    Ok(())
}

fn valid_role(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b':')
        })
}

fn valid_target_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub fn hash_json(value: &impl Serialize) -> Result<Sha256Digest, ExecutionGraphError> {
    let bytes = serde_json::to_vec(value).map_err(ExecutionGraphError::Serialize)?;
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .map_err(|_| ExecutionGraphError::InvalidMetadata)
}

#[derive(Debug, Error)]
pub enum ExecutionGraphError {
    #[error("execution graph metadata is invalid")]
    InvalidMetadata,
    #[error("execution graph state is invalid")]
    InvalidState,
    #[error("execution graph transition is invalid")]
    InvalidTransition,
    #[error("execution graph revision or claim conflicts")]
    Conflict,
    #[error("execution graph node is duplicated")]
    DuplicateNode,
    #[error("execution graph node was not found")]
    NodeNotFound,
    #[error("execution graph node is invalid")]
    InvalidNode,
    #[error("execution graph node is not ready")]
    NodeNotReady,
    #[error("execution graph dependency is invalid")]
    InvalidDependency,
    #[error("execution graph dependencies contain a cycle")]
    CyclicDependency,
    #[error("execution graph logical attempt is invalid")]
    InvalidAttempt,
    #[error("execution graph failure evidence is invalid")]
    InvalidFailure,
    #[error("execution graph timestamp is invalid")]
    InvalidTimestamp,
    #[error("execution graph payload hash differs from its canonical payload")]
    PayloadHashMismatch,
    #[error("execution graph payload serialization failed")]
    Serialize(#[source] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use ats_kernel::{SchemaId, SchemaRef, SchemaVersion};

    use super::*;

    fn digest(value: &str) -> Sha256Digest {
        Sha256Digest::parse(value.repeat(64)).unwrap()
    }

    fn payload(id: &str, value: i32) -> VersionedPayload {
        VersionedPayload::from_typed(
            SchemaRef {
                id: SchemaId::parse(id).unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
            &serde_json::json!({"value": value}),
        )
        .unwrap()
    }

    fn graph() -> (ExecutionGraphRecord, RunId) {
        let run_id = RunId::parse("run-fixture-a").unwrap();
        let graph = ExecutionGraphRecord::new_claimed(
            ExecutionGraphId::parse("graph-fixture").unwrap(),
            FeatureId::parse("composition.plan").unwrap(),
            digest("a"),
            payload("composition.blueprint", 1),
            vec![
                ExecutionNodeSpec {
                    node_id: ExecutionNodeId::parse("node.a").unwrap(),
                    role_id: "item.generate".into(),
                    depends_on: Vec::new(),
                    request_snapshot_hash: digest("b"),
                },
                ExecutionNodeSpec {
                    node_id: ExecutionNodeId::parse("node.b").unwrap(),
                    role_id: "graph.validate".into(),
                    depends_on: vec![ExecutionNodeId::parse("node.a").unwrap()],
                    request_snapshot_hash: digest("c"),
                },
            ],
            run_id.clone(),
            Utc::now(),
        )
        .unwrap();
        (graph, run_id)
    }

    #[test]
    fn enforces_dependencies_checkpoints_and_explicit_resume() {
        let (mut graph, first_run) = graph();
        let node_a = ExecutionNodeId::parse("node.a").unwrap();
        let node_b = ExecutionNodeId::parse("node.b").unwrap();
        assert!(matches!(
            graph.start_node(&node_b, &first_run, Utc::now()),
            Err(ExecutionGraphError::NodeNotReady)
        ));
        graph.start_node(&node_a, &first_run, Utc::now()).unwrap();
        graph
            .pause_after_node_failure(
                &node_a,
                &first_run,
                ExecutionFailure::new(
                    FailureCode::parse("model.transport_failed").unwrap(),
                    "composition.node",
                )
                .unwrap(),
                Utc::now(),
            )
            .unwrap();
        let paused_revision = graph.revision();
        let second_run = RunId::parse("run-fixture-b").unwrap();
        graph
            .claim(paused_revision, second_run.clone(), first_run, Utc::now())
            .unwrap();
        graph.start_node(&node_a, &second_run, Utc::now()).unwrap();
        graph
            .complete_node(
                &node_a,
                &second_run,
                payload("composition.node-checkpoint", 1),
                Utc::now(),
            )
            .unwrap();
        graph.start_node(&node_b, &second_run, Utc::now()).unwrap();
        assert_eq!(graph.nodes()[&node_a].logical_attempts.len(), 2);
    }

    #[test]
    fn commit_prepared_is_roll_forward_only_and_hash_checked() {
        let (mut graph, run_id) = graph();
        for (id, value) in [("node.a", 1), ("node.b", 2)] {
            let id = ExecutionNodeId::parse(id).unwrap();
            graph.start_node(&id, &run_id, Utc::now()).unwrap();
            graph
                .complete_node(
                    &id,
                    &run_id,
                    payload("composition.node-checkpoint", value),
                    Utc::now(),
                )
                .unwrap();
        }
        let draft = payload("composition.canonical-draft", 1);
        let intent = ExecutionCommitIntent::draft(
            CompositionDraftId::parse("draft-fixture").unwrap(),
            draft.clone(),
            hash_json(draft.payload()).unwrap(),
            digest("d"),
        );
        graph.prepare_commit(&run_id, intent, Utc::now()).unwrap();
        assert_eq!(graph.status(), ExecutionGraphStatus::CommitPrepared);
        graph.recover_stale_claim(Utc::now()).unwrap();
        assert_eq!(graph.status(), ExecutionGraphStatus::CommitPrepared);
        let resume = RunId::parse("run-fixture-c").unwrap();
        graph
            .claim(graph.revision(), resume.clone(), run_id, Utc::now())
            .unwrap();
        graph
            .mark_succeeded(&resume, payload("composition.final-result", 1), Utc::now())
            .unwrap();
        assert_eq!(graph.status(), ExecutionGraphStatus::Succeeded);
        assert!(graph.final_result_ref().is_some());
    }

    #[test]
    fn publication_commit_intent_is_hashed_and_exclusive() {
        let payload = payload("composition.publication-intent", 7);
        let intent = ExecutionCommitIntent::publication(
            "fixture-artifact",
            payload.clone(),
            hash_json(payload.payload()).unwrap(),
            digest("e"),
        );
        intent.validate().unwrap();
        let encoded = serde_json::to_vec(&intent).unwrap();
        let decoded: ExecutionCommitIntent = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, intent);

        let mut mixed = intent;
        mixed.draft_id = Some(CompositionDraftId::parse("draft-fixture").unwrap());
        assert!(matches!(
            mixed.validate(),
            Err(ExecutionGraphError::InvalidMetadata)
        ));

        let mut tampered = ExecutionCommitIntent::publication(
            "fixture-artifact",
            payload,
            digest("f"),
            digest("e"),
        );
        assert!(matches!(
            tampered.validate(),
            Err(ExecutionGraphError::PayloadHashMismatch)
        ));
        tampered.publication.as_mut().unwrap().target_id = "unsafe/path".into();
        assert!(matches!(
            tampered.validate(),
            Err(ExecutionGraphError::PayloadHashMismatch)
        ));
    }

    #[test]
    fn rejects_cycles_and_tampered_checkpoint_hashes() {
        let run_id = RunId::parse("run-fixture-cycle").unwrap();
        let error = ExecutionGraphRecord::new_claimed(
            ExecutionGraphId::parse("graph-cycle").unwrap(),
            FeatureId::parse("composition.plan").unwrap(),
            digest("a"),
            payload("composition.blueprint", 1),
            vec![
                ExecutionNodeSpec {
                    node_id: ExecutionNodeId::parse("node.a").unwrap(),
                    role_id: "item.generate".into(),
                    depends_on: vec![ExecutionNodeId::parse("node.b").unwrap()],
                    request_snapshot_hash: digest("b"),
                },
                ExecutionNodeSpec {
                    node_id: ExecutionNodeId::parse("node.b").unwrap(),
                    role_id: "item.generate".into(),
                    depends_on: vec![ExecutionNodeId::parse("node.a").unwrap()],
                    request_snapshot_hash: digest("c"),
                },
            ],
            run_id,
            Utc::now(),
        )
        .unwrap_err();
        assert!(matches!(error, ExecutionGraphError::CyclicDependency));

        let (mut graph, run_id) = graph();
        let node = ExecutionNodeId::parse("node.a").unwrap();
        graph.start_node(&node, &run_id, Utc::now()).unwrap();
        graph
            .complete_node(
                &node,
                &run_id,
                payload("composition.node-checkpoint", 1),
                Utc::now(),
            )
            .unwrap();
        let mut wire = serde_json::to_value(&graph).unwrap();
        wire["nodes"]["node.a"]["checkpoint"]["sha256"] = serde_json::json!("f".repeat(64));
        assert!(serde_json::from_value::<ExecutionGraphRecord>(wire).is_err());
    }
}
