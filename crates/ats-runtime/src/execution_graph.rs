use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{
    CompositionDraftId, ExecutionGraphId, ExecutionNodeId, FailureCode, FeatureId, ItemId,
    Sha256Digest,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{RunId, VersionedPayload};

pub const EXECUTION_GRAPH_SCHEMA_VERSION: u32 = 5;
pub const MAX_SEMANTIC_REQUESTS: u32 = 20;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionGraphStatus {
    Running,
    Validating,
    Repairing,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionFeedbackPhase {
    OutputContract,
    GeneratedContent,
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
    pub attempt_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_attempt: Option<LogicalNodeAttempt>,
    pub request_snapshot_hash: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_checkpoint: Option<HashedExecutionPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safe_failure: Option<ExecutionFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feedback_state: Option<ExecutionNodeFeedbackState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionNodeFeedbackState {
    pub round: u32,
    pub phase: ExecutionFeedbackPhase,
    pub diagnostic_fingerprint: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_sha256: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_hash: Option<Sha256Digest>,
    pub feedback: HashedExecutionPayload,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionOutputFeedback {
    pub diagnostic_fingerprint: Sha256Digest,
    pub candidate_sha256: Sha256Digest,
    pub feedback: VersionedPayload,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionRepairTargetStatus {
    Pending,
    Active,
    Completed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionRepairTargetSpec {
    pub item_id: ItemId,
    pub node_id: ExecutionNodeId,
    pub checkpoint_hash: Sha256Digest,
    pub diagnostic_fingerprints: Vec<Sha256Digest>,
    pub feedback: VersionedPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionRepairTarget {
    pub item_id: ItemId,
    pub node_id: ExecutionNodeId,
    pub checkpoint_hash: Sha256Digest,
    pub diagnostic_fingerprints: Vec<Sha256Digest>,
    pub status: ExecutionRepairTargetStatus,
    pub feedback: HashedExecutionPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionAdjustment {
    pub item_id: ItemId,
    pub expected_definition_hash: Sha256Digest,
    pub instruction_sha256: Sha256Digest,
    pub created_at: DateTime<Utc>,
    pub feedback: HashedExecutionPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionRepairCampaign {
    pub validation_fingerprint: Sha256Digest,
    pub targets: Vec<ExecutionRepairTarget>,
    pub current_target: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<ExecutionAdjustment>,
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
    semantic_request_count: u32,
    semantic_feedback_count: u32,
    repair_campaign: Option<ExecutionRepairCampaign>,
    graph_failure: Option<ExecutionFailure>,
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
                        attempt_count: 0,
                        active_attempt: None,
                        request_snapshot_hash: spec.request_snapshot_hash,
                        active_checkpoint: None,
                        safe_failure: None,
                        feedback_state: None,
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
            semantic_request_count: 0,
            semantic_feedback_count: 0,
            repair_campaign: None,
            graph_failure: None,
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
                next.status = if next.repair_campaign.is_some() {
                    ExecutionGraphStatus::Repairing
                } else {
                    ExecutionGraphStatus::Running
                };
                next.graph_failure = None;
            }
            next.active_run_id = Some(run_id);
            next.previous_run_id = Some(previous_run_id);
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim_adjustment(
        &mut self,
        expected_revision: u64,
        run_id: RunId,
        previous_run_id: RunId,
        validation_fingerprint: Sha256Digest,
        target: ExecutionRepairTargetSpec,
        adjustment: ExecutionAdjustment,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        if self.revision != expected_revision
            || self.status != ExecutionGraphStatus::Paused
            || self.active_run_id.is_some()
            || self.repair_campaign.is_some()
            || self.commit_intent.is_some()
            || self.final_result_ref.is_some()
        {
            return Err(ExecutionGraphError::Conflict);
        }
        self.mutate(at, |next| {
            if target.item_id != adjustment.item_id
                || target.diagnostic_fingerprints.len() != 1
                || target.feedback != adjustment.feedback.payload
            {
                return Err(ExecutionGraphError::InvalidMetadata);
            }
            let node = next
                .nodes
                .get(&target.node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            if node.status != ExecutionNodeStatus::Succeeded
                || node.active_checkpoint.as_ref().map(|value| &value.sha256)
                    != Some(&target.checkpoint_hash)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.repair_campaign = Some(ExecutionRepairCampaign {
                validation_fingerprint,
                targets: vec![ExecutionRepairTarget {
                    item_id: target.item_id,
                    node_id: target.node_id,
                    checkpoint_hash: target.checkpoint_hash,
                    diagnostic_fingerprints: target.diagnostic_fingerprints,
                    status: ExecutionRepairTargetStatus::Pending,
                    feedback: HashedExecutionPayload::new(target.feedback)?,
                }],
                current_target: 0,
                adjustment: Some(adjustment),
            });
            next.status = ExecutionGraphStatus::Repairing;
            next.active_run_id = Some(run_id);
            next.previous_run_id = Some(previous_run_id);
            next.graph_failure = None;
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn derive_adjustment(
        &self,
        execution_graph_id: ExecutionGraphId,
        request_snapshot_hash: Sha256Digest,
        run_id: RunId,
        validation_fingerprint: Sha256Digest,
        target: ExecutionRepairTargetSpec,
        adjustment: ExecutionAdjustment,
        at: DateTime<Utc>,
    ) -> Result<Self, ExecutionGraphError> {
        if self.status != ExecutionGraphStatus::Succeeded
            || self.repair_campaign.is_some()
            || self.final_result_ref.is_none()
            || target.item_id != adjustment.item_id
            || target.diagnostic_fingerprints.len() != 1
            || target.feedback != adjustment.feedback.payload
        {
            return Err(ExecutionGraphError::InvalidTransition);
        }
        let mut next = self.clone();
        let node = next
            .nodes
            .get(&target.node_id)
            .ok_or(ExecutionGraphError::NodeNotFound)?;
        if node.status != ExecutionNodeStatus::Succeeded
            || node.active_checkpoint.as_ref().map(|value| &value.sha256)
                != Some(&target.checkpoint_hash)
        {
            return Err(ExecutionGraphError::InvalidTransition);
        }
        next.execution_graph_id = execution_graph_id;
        next.request_snapshot_hash = request_snapshot_hash;
        next.revision = 1;
        next.status = ExecutionGraphStatus::Repairing;
        next.active_run_id = Some(run_id);
        next.previous_run_id = self.previous_run_id.clone();
        next.created_at = at;
        next.updated_at = at;
        next.semantic_request_count = 0;
        next.semantic_feedback_count = 0;
        next.repair_campaign = Some(ExecutionRepairCampaign {
            validation_fingerprint,
            targets: vec![ExecutionRepairTarget {
                item_id: target.item_id,
                node_id: target.node_id,
                checkpoint_hash: target.checkpoint_hash,
                diagnostic_fingerprints: target.diagnostic_fingerprints,
                status: ExecutionRepairTargetStatus::Pending,
                feedback: HashedExecutionPayload::new(target.feedback)?,
            }],
            current_target: 0,
            adjustment: Some(adjustment),
        });
        next.graph_failure = None;
        next.commit_intent = None;
        next.final_result_ref = None;
        next.validate()?;
        Ok(next)
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
            let ordinal = node
                .attempt_count
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            node.attempt_count = ordinal;
            node.status = ExecutionNodeStatus::Running;
            node.safe_failure = None;
            node.active_attempt = Some(LogicalNodeAttempt {
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

    pub fn record_semantic_baseline_request(
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
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            let _ = active_attempt_mut(node, run_id)?;
            if node.feedback_state.is_some() {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.semantic_request_count = next
                .semantic_request_count
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            Ok(())
        })
    }

    pub fn update_running_node_request_snapshot_hash(
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
            let _ = active_attempt_mut(node, run_id)?;
            if node.active_checkpoint.is_some() {
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
            node.active_checkpoint = Some(HashedExecutionPayload::new(checkpoint)?);
            node.active_attempt = None;
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
            node.active_attempt = None;
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
                    ExecutionGraphStatus::Running
                        | ExecutionGraphStatus::Validating
                        | ExecutionGraphStatus::Repairing
                        | ExecutionGraphStatus::PauseRequested
                )
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            interrupt_running_node(next, run_id, at)?;
            interrupt_active_repair_target(next)?;
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
            if !matches!(
                next.status,
                ExecutionGraphStatus::Running
                    | ExecutionGraphStatus::Validating
                    | ExecutionGraphStatus::Repairing
            ) || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.status = ExecutionGraphStatus::PauseRequested;
            Ok(())
        })
    }

    pub fn begin_validation(
        &mut self,
        run_id: &RunId,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if !matches!(
                next.status,
                ExecutionGraphStatus::Running | ExecutionGraphStatus::Repairing
            ) || next.active_run_id.as_ref() != Some(run_id)
                || next
                    .nodes
                    .values()
                    .any(|node| node.status != ExecutionNodeStatus::Succeeded)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            if next.status == ExecutionGraphStatus::Repairing
                && let Some(campaign) = next.repair_campaign.as_ref()
            {
                if usize::try_from(campaign.current_target).ok() != Some(campaign.targets.len())
                    || campaign
                        .targets
                        .iter()
                        .any(|target| target.status != ExecutionRepairTargetStatus::Completed)
                {
                    return Err(ExecutionGraphError::InvalidTransition);
                }
                next.repair_campaign = None;
            }
            next.status = ExecutionGraphStatus::Validating;
            Ok(())
        })
    }

    pub fn begin_repair_campaign(
        &mut self,
        run_id: &RunId,
        validation_fingerprint: Sha256Digest,
        targets: Vec<ExecutionRepairTargetSpec>,
        adjustment: Option<ExecutionAdjustment>,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Validating
                || next.active_run_id.as_ref() != Some(run_id)
                || targets.is_empty()
                || targets.len() > next.nodes.len()
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let mut item_ids = BTreeSet::new();
            let mut node_ids = BTreeSet::new();
            let mut campaign_targets = Vec::with_capacity(targets.len());
            for target in targets {
                if !item_ids.insert(target.item_id.clone())
                    || !node_ids.insert(target.node_id.clone())
                    || target.diagnostic_fingerprints.is_empty()
                    || target.diagnostic_fingerprints.len() > 256
                {
                    return Err(ExecutionGraphError::InvalidMetadata);
                }
                let unique = target
                    .diagnostic_fingerprints
                    .iter()
                    .collect::<BTreeSet<_>>();
                let node = next
                    .nodes
                    .get(&target.node_id)
                    .ok_or(ExecutionGraphError::NodeNotFound)?;
                if unique.len() != target.diagnostic_fingerprints.len()
                    || node.status != ExecutionNodeStatus::Succeeded
                    || node.active_checkpoint.as_ref().map(|value| &value.sha256)
                        != Some(&target.checkpoint_hash)
                {
                    return Err(ExecutionGraphError::InvalidTransition);
                }
                campaign_targets.push(ExecutionRepairTarget {
                    item_id: target.item_id,
                    node_id: target.node_id,
                    checkpoint_hash: target.checkpoint_hash,
                    diagnostic_fingerprints: target.diagnostic_fingerprints,
                    status: ExecutionRepairTargetStatus::Pending,
                    feedback: HashedExecutionPayload::new(target.feedback)?,
                });
            }
            if let Some(value) = &adjustment {
                value.feedback.validate()?;
                if campaign_targets.len() != 1 || campaign_targets[0].item_id != value.item_id {
                    return Err(ExecutionGraphError::InvalidMetadata);
                }
            }
            next.repair_campaign = Some(ExecutionRepairCampaign {
                validation_fingerprint,
                targets: campaign_targets,
                current_target: 0,
                adjustment,
            });
            next.status = ExecutionGraphStatus::Repairing;
            next.graph_failure = None;
            Ok(())
        })
    }

    pub fn activate_repair_target(
        &mut self,
        run_id: &RunId,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Repairing
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let campaign = next
                .repair_campaign
                .as_mut()
                .ok_or(ExecutionGraphError::InvalidTransition)?;
            let index = usize::try_from(campaign.current_target)
                .map_err(|_| ExecutionGraphError::InvalidMetadata)?;
            let target = campaign
                .targets
                .get_mut(index)
                .ok_or(ExecutionGraphError::InvalidTransition)?;
            if target.status != ExecutionRepairTargetStatus::Pending {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let is_adjustment = campaign.adjustment.is_some();
            if !is_adjustment && next.semantic_feedback_count >= MAX_SEMANTIC_REQUESTS {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            target.status = ExecutionRepairTargetStatus::Active;
            next.semantic_request_count = next
                .semantic_request_count
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            if !is_adjustment {
                next.semantic_feedback_count = next
                    .semantic_feedback_count
                    .checked_add(1)
                    .ok_or(ExecutionGraphError::InvalidAttempt)?;
            }
            Ok(())
        })
    }

    pub fn complete_repair_target(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        checkpoint: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.complete_repair_target_inner(node_id, run_id, None, checkpoint, at)
    }

    pub fn complete_repair_target_with_request(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        request_snapshot_hash: Sha256Digest,
        checkpoint: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.complete_repair_target_inner(
            node_id,
            run_id,
            Some(request_snapshot_hash),
            checkpoint,
            at,
        )
    }

    fn complete_repair_target_inner(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        request_snapshot_hash: Option<Sha256Digest>,
        checkpoint: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Repairing
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let campaign = next
                .repair_campaign
                .as_mut()
                .ok_or(ExecutionGraphError::InvalidTransition)?;
            let index = usize::try_from(campaign.current_target)
                .map_err(|_| ExecutionGraphError::InvalidMetadata)?;
            let target = campaign
                .targets
                .get_mut(index)
                .ok_or(ExecutionGraphError::InvalidTransition)?;
            if target.status != ExecutionRepairTargetStatus::Active || &target.node_id != node_id {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            if node.status != ExecutionNodeStatus::Succeeded
                || node.active_checkpoint.as_ref().map(|value| &value.sha256)
                    != Some(&target.checkpoint_hash)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let replacement = HashedExecutionPayload::new(checkpoint)?;
            if replacement.sha256 == target.checkpoint_hash {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            target.checkpoint_hash = replacement.sha256.clone();
            target.status = ExecutionRepairTargetStatus::Completed;
            if let Some(request_snapshot_hash) = request_snapshot_hash {
                node.request_snapshot_hash = request_snapshot_hash;
            }
            node.active_checkpoint = Some(replacement);
            node.safe_failure = None;
            campaign.current_target = campaign
                .current_target
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidMetadata)?;
            Ok(())
        })
    }

    pub fn record_output_feedback(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        output: ExecutionOutputFeedback,
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
            let _ = active_attempt_mut(node, run_id)?;
            if node.active_checkpoint.is_some()
                || node.feedback_state.as_ref().is_some_and(|state| {
                    state.phase != ExecutionFeedbackPhase::OutputContract
                        || state.diagnostic_fingerprint == output.diagnostic_fingerprint
                            && state.candidate_sha256.as_ref() == Some(&output.candidate_sha256)
                })
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let round = node
                .feedback_state
                .as_ref()
                .map_or(0, |state| state.round)
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            if next.semantic_feedback_count >= MAX_SEMANTIC_REQUESTS {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.semantic_request_count = next
                .semantic_request_count
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            next.semantic_feedback_count = next
                .semantic_feedback_count
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            node.feedback_state = Some(ExecutionNodeFeedbackState {
                round,
                phase: ExecutionFeedbackPhase::OutputContract,
                diagnostic_fingerprint: output.diagnostic_fingerprint,
                candidate_sha256: Some(output.candidate_sha256),
                checkpoint_hash: None,
                feedback: HashedExecutionPayload::new(output.feedback)?,
            });
            Ok(())
        })
    }

    pub fn record_repair_output_feedback(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        checkpoint_hash: Sha256Digest,
        output: ExecutionOutputFeedback,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Repairing
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            if node.status != ExecutionNodeStatus::Succeeded
                || node.active_checkpoint.as_ref().map(|value| &value.sha256)
                    != Some(&checkpoint_hash)
                || node.feedback_state.as_ref().is_some_and(|state| {
                    state.phase == ExecutionFeedbackPhase::OutputContract
                        && state.diagnostic_fingerprint == output.diagnostic_fingerprint
                        && state.candidate_sha256.as_ref() == Some(&output.candidate_sha256)
                        && state.checkpoint_hash.as_ref() == Some(&checkpoint_hash)
                })
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let round = node
                .feedback_state
                .as_ref()
                .map_or(0, |state| state.round)
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            if next.semantic_feedback_count >= MAX_SEMANTIC_REQUESTS {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.semantic_request_count = next
                .semantic_request_count
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            next.semantic_feedback_count = next
                .semantic_feedback_count
                .checked_add(1)
                .ok_or(ExecutionGraphError::InvalidAttempt)?;
            node.feedback_state = Some(ExecutionNodeFeedbackState {
                round,
                phase: ExecutionFeedbackPhase::OutputContract,
                diagnostic_fingerprint: output.diagnostic_fingerprint,
                candidate_sha256: Some(output.candidate_sha256),
                checkpoint_hash: Some(checkpoint_hash),
                feedback: HashedExecutionPayload::new(output.feedback)?,
            });
            Ok(())
        })
    }

    pub fn pause_after_graph_failure(
        &mut self,
        run_id: &RunId,
        failure: ExecutionFailure,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        failure.validate()?;
        self.mutate(at, |next| {
            if !matches!(
                next.status,
                ExecutionGraphStatus::Validating | ExecutionGraphStatus::Repairing
            ) || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            next.graph_failure = Some(failure);
            interrupt_active_repair_target(next)?;
            next.status = ExecutionGraphStatus::Paused;
            next.previous_run_id = next.active_run_id.clone();
            next.active_run_id = None;
            Ok(())
        })
    }

    pub fn replace_checkpoint(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        checkpoint: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.replace_checkpoint_inner(node_id, run_id, None, checkpoint, at)
    }

    pub fn replace_checkpoint_with_request(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        request_snapshot_hash: Sha256Digest,
        checkpoint: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.replace_checkpoint_inner(node_id, run_id, Some(request_snapshot_hash), checkpoint, at)
    }

    fn replace_checkpoint_inner(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        request_snapshot_hash: Option<Sha256Digest>,
        checkpoint: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Repairing
                || next.active_run_id.as_ref() != Some(run_id)
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            if node.status != ExecutionNodeStatus::Succeeded || node.active_checkpoint.is_none() {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            if let Some(request_snapshot_hash) = request_snapshot_hash {
                node.request_snapshot_hash = request_snapshot_hash;
            }
            node.active_checkpoint = Some(HashedExecutionPayload::new(checkpoint)?);
            node.safe_failure = None;
            Ok(())
        })
    }

    pub fn replace_checkpoint_while_running(
        &mut self,
        node_id: &ExecutionNodeId,
        run_id: &RunId,
        checkpoint: VersionedPayload,
        at: DateTime<Utc>,
    ) -> Result<(), ExecutionGraphError> {
        self.mutate(at, |next| {
            if next.status != ExecutionGraphStatus::Running
                || next.active_run_id.as_ref() != Some(run_id)
                || !next
                    .nodes
                    .values()
                    .any(|node| node.feedback_state.is_some())
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            let node = next
                .nodes
                .get_mut(node_id)
                .ok_or(ExecutionGraphError::NodeNotFound)?;
            if node.status != ExecutionNodeStatus::Succeeded
                || node.active_checkpoint.is_none()
                || node.feedback_state.is_some()
            {
                return Err(ExecutionGraphError::InvalidTransition);
            }
            node.active_checkpoint = Some(HashedExecutionPayload::new(checkpoint)?);
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
            if next.status != ExecutionGraphStatus::Validating
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
                    && let Some(attempt) = node.active_attempt.as_mut()
                {
                    attempt.outcome = LogicalAttemptOutcome::Cancelled;
                    attempt.completed_at = Some(at);
                }
                node.active_attempt = None;
                if node.status != ExecutionNodeStatus::Succeeded {
                    node.status = ExecutionNodeStatus::Cancelled;
                }
            }
            interrupt_active_repair_target(next)?;
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
                ExecutionGraphStatus::Running
                | ExecutionGraphStatus::Validating
                | ExecutionGraphStatus::Repairing
                | ExecutionGraphStatus::PauseRequested => {
                    interrupt_running_node(next, &run_id, at)?;
                    interrupt_active_repair_target(next)?;
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
        if self.semantic_feedback_count > MAX_SEMANTIC_REQUESTS
            || self.semantic_feedback_count > self.semantic_request_count
        {
            return Err(ExecutionGraphError::InvalidAttempt);
        }
        if let Some(campaign) = &self.repair_campaign {
            validate_repair_campaign(campaign, &self.nodes, self.status)?;
        }
        match self.status {
            ExecutionGraphStatus::Running
            | ExecutionGraphStatus::Validating
            | ExecutionGraphStatus::Repairing
            | ExecutionGraphStatus::PauseRequested => {
                if self.active_run_id.is_none()
                    || self.graph_failure.is_some()
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
                    || self.graph_failure.is_some()
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
                    || self.graph_failure.is_some()
                    || self.commit_intent.is_none()
                    || self.final_result_ref.is_some()
                {
                    return Err(ExecutionGraphError::InvalidState);
                }
            }
            ExecutionGraphStatus::Succeeded => {
                if self.active_run_id.is_some()
                    || self.graph_failure.is_some()
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
                if self.active_run_id.is_none()
                    || self.commit_intent.is_some()
                    || self.graph_failure.is_some()
                {
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
    pub const fn semantic_request_count(&self) -> u32 {
        self.semantic_request_count
    }

    #[must_use]
    pub const fn semantic_feedback_count(&self) -> u32 {
        self.semantic_feedback_count
    }

    #[must_use]
    pub fn repair_campaign(&self) -> Option<&ExecutionRepairCampaign> {
        self.repair_campaign.as_ref()
    }

    #[must_use]
    pub fn graph_failure(&self) -> Option<&ExecutionFailure> {
        self.graph_failure.as_ref()
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
            semantic_request_count: u32,
            semantic_feedback_count: u32,
            repair_campaign: Option<ExecutionRepairCampaign>,
            graph_failure: Option<ExecutionFailure>,
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
            semantic_request_count: wire.semantic_request_count,
            semantic_feedback_count: wire.semantic_feedback_count,
            repair_campaign: wire.repair_campaign,
            graph_failure: wire.graph_failure,
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
        .active_attempt
        .as_mut()
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
            node.active_attempt = None;
            node.status = ExecutionNodeStatus::Pending;
        }
    }
    Ok(())
}

fn interrupt_active_repair_target(
    graph: &mut ExecutionGraphRecord,
) -> Result<(), ExecutionGraphError> {
    let Some(campaign) = graph.repair_campaign.as_mut() else {
        return Ok(());
    };
    let index = usize::try_from(campaign.current_target)
        .map_err(|_| ExecutionGraphError::InvalidMetadata)?;
    if let Some(target) = campaign.targets.get_mut(index)
        && target.status == ExecutionRepairTargetStatus::Active
    {
        target.status = ExecutionRepairTargetStatus::Pending;
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

fn validate_repair_campaign(
    campaign: &ExecutionRepairCampaign,
    nodes: &BTreeMap<ExecutionNodeId, ExecutionNodeRecord>,
    graph_status: ExecutionGraphStatus,
) -> Result<(), ExecutionGraphError> {
    if campaign.targets.is_empty() || campaign.targets.len() > nodes.len() {
        return Err(ExecutionGraphError::InvalidMetadata);
    }
    let current = usize::try_from(campaign.current_target)
        .map_err(|_| ExecutionGraphError::InvalidMetadata)?;
    if current > campaign.targets.len() {
        return Err(ExecutionGraphError::InvalidMetadata);
    }
    let mut item_ids = BTreeSet::new();
    let mut node_ids = BTreeSet::new();
    for (index, target) in campaign.targets.iter().enumerate() {
        if !item_ids.insert(target.item_id.clone())
            || !node_ids.insert(target.node_id.clone())
            || target.diagnostic_fingerprints.is_empty()
            || target.diagnostic_fingerprints.len() > 256
            || target.feedback.validate().is_err()
        {
            return Err(ExecutionGraphError::InvalidMetadata);
        }
        let unique = target
            .diagnostic_fingerprints
            .iter()
            .collect::<BTreeSet<_>>();
        let node = nodes
            .get(&target.node_id)
            .ok_or(ExecutionGraphError::NodeNotFound)?;
        if unique.len() != target.diagnostic_fingerprints.len()
            || node.status != ExecutionNodeStatus::Succeeded
            || node.active_checkpoint.as_ref().map(|value| &value.sha256)
                != Some(&target.checkpoint_hash)
        {
            return Err(ExecutionGraphError::InvalidState);
        }
        let status_is_valid = if index < current {
            target.status == ExecutionRepairTargetStatus::Completed
        } else if index > current {
            target.status == ExecutionRepairTargetStatus::Pending
        } else {
            match graph_status {
                ExecutionGraphStatus::Repairing | ExecutionGraphStatus::PauseRequested => matches!(
                    target.status,
                    ExecutionRepairTargetStatus::Pending | ExecutionRepairTargetStatus::Active
                ),
                ExecutionGraphStatus::Paused | ExecutionGraphStatus::Cancelled => {
                    target.status == ExecutionRepairTargetStatus::Pending
                }
                _ => false,
            }
        };
        if !status_is_valid {
            return Err(ExecutionGraphError::InvalidState);
        }
    }
    if current == campaign.targets.len()
        && campaign
            .targets
            .iter()
            .any(|target| target.status != ExecutionRepairTargetStatus::Completed)
    {
        return Err(ExecutionGraphError::InvalidState);
    }
    if let Some(adjustment) = &campaign.adjustment {
        adjustment.feedback.validate()?;
        if campaign.targets.len() != 1 || campaign.targets[0].item_id != adjustment.item_id {
            return Err(ExecutionGraphError::InvalidMetadata);
        }
    }
    Ok(())
}

fn validate_node_state(node: &ExecutionNodeRecord) -> Result<(), ExecutionGraphError> {
    if node.active_attempt.as_ref().is_some_and(|attempt| {
        attempt.ordinal != node.attempt_count
            || attempt.completed_at.is_some()
            || attempt.outcome != LogicalAttemptOutcome::Running
            || attempt.failure.is_some()
    }) {
        return Err(ExecutionGraphError::InvalidAttempt);
    }
    if let Some(state) = &node.feedback_state {
        if state.round == 0
            || state.phase == ExecutionFeedbackPhase::OutputContract
                && state.candidate_sha256.is_none()
            || state.phase == ExecutionFeedbackPhase::GeneratedContent
                && (state.candidate_sha256.is_some() || state.checkpoint_hash.is_none())
        {
            return Err(ExecutionGraphError::InvalidAttempt);
        }
        state.feedback.validate()?;
    }
    match node.status {
        ExecutionNodeStatus::Pending
            if node.active_checkpoint.is_none() && node.active_attempt.is_none() => {}
        ExecutionNodeStatus::Running
            if node.active_checkpoint.is_none() && node.active_attempt.is_some() => {}
        ExecutionNodeStatus::Succeeded
            if node.active_checkpoint.is_some()
                && node.safe_failure.is_none()
                && node.active_attempt.is_none() => {}
        ExecutionNodeStatus::Cancelled if node.active_attempt.is_none() => {}
        _ => return Err(ExecutionGraphError::InvalidState),
    }
    if let Some(checkpoint) = &node.active_checkpoint {
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

    fn feedback(value: i32) -> VersionedPayload {
        payload("generation.feedback", value)
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

    fn completed_graph() -> (ExecutionGraphRecord, RunId) {
        let (mut graph, run_id) = graph();
        for (id, value) in [("node.a", 1), ("node.b", 2)] {
            let node_id = ExecutionNodeId::parse(id).unwrap();
            graph.start_node(&node_id, &run_id, Utc::now()).unwrap();
            graph
                .complete_node(
                    &node_id,
                    &run_id,
                    payload("composition.node-checkpoint", value),
                    Utc::now(),
                )
                .unwrap();
        }
        graph.begin_validation(&run_id, Utc::now()).unwrap();
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
        assert_eq!(graph.nodes()[&node_a].attempt_count, 2);
        assert!(graph.nodes()[&node_a].active_attempt.is_none());
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
        graph.begin_validation(&run_id, Utc::now()).unwrap();
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
    fn repair_campaign_advances_serially_and_preserves_other_checkpoints() {
        let (mut graph, run_id) = completed_graph();
        let node_a = ExecutionNodeId::parse("node.a").unwrap();
        let node_b = ExecutionNodeId::parse("node.b").unwrap();
        let original_a = graph.nodes()[&node_a]
            .active_checkpoint
            .as_ref()
            .unwrap()
            .sha256
            .clone();
        let original_b = graph.nodes()[&node_b]
            .active_checkpoint
            .as_ref()
            .unwrap()
            .sha256
            .clone();
        graph
            .begin_repair_campaign(
                &run_id,
                digest("9"),
                vec![
                    ExecutionRepairTargetSpec {
                        item_id: ItemId::parse("item-a").unwrap(),
                        node_id: node_a.clone(),
                        checkpoint_hash: original_a.clone(),
                        diagnostic_fingerprints: vec![digest("3")],
                        feedback: feedback(10),
                    },
                    ExecutionRepairTargetSpec {
                        item_id: ItemId::parse("item-b").unwrap(),
                        node_id: node_b.clone(),
                        checkpoint_hash: original_b.clone(),
                        diagnostic_fingerprints: vec![digest("4"), digest("5")],
                        feedback: feedback(11),
                    },
                ],
                None,
                Utc::now(),
            )
            .unwrap();
        assert_eq!(graph.status(), ExecutionGraphStatus::Repairing);
        graph.activate_repair_target(&run_id, Utc::now()).unwrap();
        assert_eq!(graph.semantic_request_count(), 1);
        graph
            .complete_repair_target(
                &node_a,
                &run_id,
                payload("composition.node-checkpoint", 3),
                Utc::now(),
            )
            .unwrap();
        assert_ne!(
            graph.nodes()[&node_a]
                .active_checkpoint
                .as_ref()
                .unwrap()
                .sha256,
            original_a
        );
        assert_eq!(
            graph.nodes()[&node_b]
                .active_checkpoint
                .as_ref()
                .unwrap()
                .sha256,
            original_b
        );
        graph.activate_repair_target(&run_id, Utc::now()).unwrap();
        graph
            .complete_repair_target(
                &node_b,
                &run_id,
                payload("composition.node-checkpoint", 4),
                Utc::now(),
            )
            .unwrap();
        assert_eq!(graph.semantic_request_count(), 2);
        assert_eq!(graph.repair_campaign().unwrap().current_target, 2);
        graph.begin_validation(&run_id, Utc::now()).unwrap();
        assert!(graph.repair_campaign().is_none());
    }

    #[test]
    fn adjustment_derivation_is_atomic_and_resets_publication_state() {
        let (mut source, source_run_id) = completed_graph();
        let node_id = ExecutionNodeId::parse("node.a").unwrap();
        let original_checkpoint_hash = source.nodes()[&node_id]
            .active_checkpoint
            .as_ref()
            .unwrap()
            .sha256
            .clone();
        let repair_target = ExecutionRepairTargetSpec {
            item_id: ItemId::parse("item-a").unwrap(),
            node_id: node_id.clone(),
            checkpoint_hash: original_checkpoint_hash,
            diagnostic_fingerprints: vec![digest("3")],
            feedback: feedback(10),
        };

        assert!(matches!(
            source.derive_adjustment(
                ExecutionGraphId::parse("graph-adjustment-too-early").unwrap(),
                digest("4"),
                RunId::parse("run-adjustment-too-early").unwrap(),
                digest("5"),
                repair_target.clone(),
                ExecutionAdjustment {
                    item_id: ItemId::parse("item-a").unwrap(),
                    expected_definition_hash: digest("6"),
                    instruction_sha256: digest("5"),
                    created_at: Utc::now(),
                    feedback: HashedExecutionPayload::new(feedback(10)).unwrap(),
                },
                Utc::now(),
            ),
            Err(ExecutionGraphError::InvalidTransition)
        ));

        source
            .begin_repair_campaign(
                &source_run_id,
                digest("7"),
                vec![repair_target],
                None,
                Utc::now(),
            )
            .unwrap();
        source
            .activate_repair_target(&source_run_id, Utc::now())
            .unwrap();
        source
            .complete_repair_target(
                &node_id,
                &source_run_id,
                payload("composition.node-checkpoint", 11),
                Utc::now(),
            )
            .unwrap();
        source.begin_validation(&source_run_id, Utc::now()).unwrap();
        let publication = payload("composition.publication", 12);
        source
            .prepare_commit(
                &source_run_id,
                ExecutionCommitIntent::publication(
                    "artifact-source",
                    publication.clone(),
                    hash_json(publication.payload()).unwrap(),
                    digest("8"),
                ),
                Utc::now(),
            )
            .unwrap();
        source
            .mark_succeeded(
                &source_run_id,
                payload("composition.final-result", 13),
                Utc::now(),
            )
            .unwrap();
        assert_eq!(source.semantic_request_count(), 1);
        assert!(source.commit_intent().is_some());
        assert!(source.final_result_ref().is_some());

        let source_before_derivation = source.clone();
        let derived_graph_id = ExecutionGraphId::parse("graph-adjustment-derived").unwrap();
        let adjustment_run_id = RunId::parse("run-adjustment-derived").unwrap();
        let instruction_sha256 = digest("9");
        let adjustment_feedback = feedback(14);
        let checkpoint_hash = source.nodes()[&node_id]
            .active_checkpoint
            .as_ref()
            .unwrap()
            .sha256
            .clone();
        let derived = source
            .derive_adjustment(
                derived_graph_id.clone(),
                digest("a"),
                adjustment_run_id.clone(),
                instruction_sha256.clone(),
                ExecutionRepairTargetSpec {
                    item_id: ItemId::parse("item-a").unwrap(),
                    node_id,
                    checkpoint_hash,
                    diagnostic_fingerprints: vec![instruction_sha256.clone()],
                    feedback: adjustment_feedback.clone(),
                },
                ExecutionAdjustment {
                    item_id: ItemId::parse("item-a").unwrap(),
                    expected_definition_hash: digest("b"),
                    instruction_sha256,
                    created_at: Utc::now(),
                    feedback: HashedExecutionPayload::new(adjustment_feedback).unwrap(),
                },
                Utc::now(),
            )
            .unwrap();

        assert_eq!(source, source_before_derivation);
        assert_eq!(derived.id(), &derived_graph_id);
        assert_eq!(derived.revision(), 1);
        assert_eq!(derived.status(), ExecutionGraphStatus::Repairing);
        assert_eq!(derived.active_run_id(), Some(&adjustment_run_id));
        assert_eq!(derived.previous_run_id(), Some(&source_run_id));
        assert_eq!(derived.semantic_request_count(), 0);
        assert!(derived.commit_intent().is_none());
        assert!(derived.final_result_ref().is_none());
        let campaign = derived.repair_campaign().unwrap();
        assert_eq!(campaign.targets.len(), 1);
        assert_eq!(campaign.current_target, 0);
        assert_eq!(
            campaign.targets[0].status,
            ExecutionRepairTargetStatus::Pending
        );
        assert!(campaign.adjustment.is_some());
    }

    #[test]
    fn repair_campaign_resume_retries_only_active_target_without_refunding_budget() {
        let (mut graph, first_run) = completed_graph();
        let node = ExecutionNodeId::parse("node.a").unwrap();
        let checkpoint_hash = graph.nodes()[&node]
            .active_checkpoint
            .as_ref()
            .unwrap()
            .sha256
            .clone();
        graph
            .begin_repair_campaign(
                &first_run,
                digest("9"),
                vec![ExecutionRepairTargetSpec {
                    item_id: ItemId::parse("item-a").unwrap(),
                    node_id: node,
                    checkpoint_hash,
                    diagnostic_fingerprints: vec![digest("3")],
                    feedback: feedback(10),
                }],
                None,
                Utc::now(),
            )
            .unwrap();
        graph
            .activate_repair_target(&first_run, Utc::now())
            .unwrap();
        graph.recover_stale_claim(Utc::now()).unwrap();
        assert_eq!(graph.status(), ExecutionGraphStatus::Paused);
        assert_eq!(graph.semantic_request_count(), 1);
        assert_eq!(
            graph.repair_campaign().unwrap().targets[0].status,
            ExecutionRepairTargetStatus::Pending
        );
        let second_run = RunId::parse("run-campaign-resume").unwrap();
        graph
            .claim(graph.revision(), second_run.clone(), first_run, Utc::now())
            .unwrap();
        assert_eq!(graph.status(), ExecutionGraphStatus::Repairing);
        graph
            .activate_repair_target(&second_run, Utc::now())
            .unwrap();
        assert_eq!(graph.semantic_request_count(), 2);
    }

    #[test]
    fn repair_campaign_rejects_duplicate_targets_and_tampered_cursor() {
        let (mut campaign_graph, run_id) = completed_graph();
        let node = ExecutionNodeId::parse("node.a").unwrap();
        let checkpoint_hash = campaign_graph.nodes()[&node]
            .active_checkpoint
            .as_ref()
            .unwrap()
            .sha256
            .clone();
        let target = ExecutionRepairTargetSpec {
            item_id: ItemId::parse("item-a").unwrap(),
            node_id: node,
            checkpoint_hash,
            diagnostic_fingerprints: vec![digest("3")],
            feedback: feedback(10),
        };
        assert!(matches!(
            campaign_graph.begin_repair_campaign(
                &run_id,
                digest("9"),
                vec![target.clone(), target],
                None,
                Utc::now(),
            ),
            Err(ExecutionGraphError::InvalidMetadata)
        ));

        let (graph, _) = graph();
        let mut encoded = serde_json::to_value(graph).unwrap();
        encoded["schemaVersion"] = serde_json::json!(3);
        assert!(serde_json::from_value::<ExecutionGraphRecord>(encoded).is_err());
    }

    #[test]
    fn semantic_baselines_do_not_consume_the_shared_feedback_allowance() {
        let run_id = RunId::parse("run-many-baselines").unwrap();
        let mut previous = None;
        let nodes = (0..=MAX_SEMANTIC_REQUESTS)
            .map(|index| {
                let node_id = ExecutionNodeId::parse(format!("item.{index:03}.behavior")).unwrap();
                let spec = ExecutionNodeSpec {
                    node_id: node_id.clone(),
                    role_id: "composition.behavior".into(),
                    depends_on: previous.iter().cloned().collect(),
                    request_snapshot_hash: digest("b"),
                };
                previous = Some(node_id);
                spec
            })
            .collect::<Vec<_>>();
        let mut graph = ExecutionGraphRecord::new_claimed(
            ExecutionGraphId::parse("graph-many-baselines").unwrap(),
            FeatureId::parse("composition.generate").unwrap(),
            digest("a"),
            payload("composition.blueprint", 1),
            nodes.clone(),
            run_id.clone(),
            Utc::now(),
        )
        .unwrap();

        for (index, node) in nodes.iter().enumerate() {
            graph
                .start_node(&node.node_id, &run_id, Utc::now())
                .unwrap();
            graph
                .record_semantic_baseline_request(&node.node_id, &run_id, Utc::now())
                .unwrap();
            graph
                .complete_node(
                    &node.node_id,
                    &run_id,
                    payload("composition.behavior-checkpoint", index as i32),
                    Utc::now(),
                )
                .unwrap();
        }

        assert_eq!(graph.semantic_request_count(), MAX_SEMANTIC_REQUESTS + 1);
        assert_eq!(graph.semantic_feedback_count(), 0);
    }

    #[test]
    fn output_feedback_is_hashed_recoverable_and_stops_only_on_identical_output() {
        let (mut graph, first_run) = graph();
        let node = ExecutionNodeId::parse("node.a").unwrap();
        let fingerprint = digest("d");
        let first_candidate = digest("e");
        graph.start_node(&node, &first_run, Utc::now()).unwrap();
        graph
            .record_output_feedback(
                &node,
                &first_run,
                ExecutionOutputFeedback {
                    diagnostic_fingerprint: fingerprint.clone(),
                    candidate_sha256: first_candidate.clone(),
                    feedback: feedback(1),
                },
                Utc::now(),
            )
            .unwrap();
        let state = graph.nodes()[&node].feedback_state.as_ref().unwrap();
        assert_eq!(state.round, 1);
        assert_eq!(state.phase, ExecutionFeedbackPhase::OutputContract);
        assert_eq!(state.candidate_sha256.as_ref(), Some(&first_candidate));
        assert!(matches!(
            graph.record_output_feedback(
                &node,
                &first_run,
                ExecutionOutputFeedback {
                    diagnostic_fingerprint: fingerprint.clone(),
                    candidate_sha256: first_candidate,
                    feedback: feedback(2),
                },
                Utc::now(),
            ),
            Err(ExecutionGraphError::InvalidTransition)
        ));
        graph
            .record_output_feedback(
                &node,
                &first_run,
                ExecutionOutputFeedback {
                    diagnostic_fingerprint: fingerprint,
                    candidate_sha256: digest("f"),
                    feedback: feedback(2),
                },
                Utc::now(),
            )
            .unwrap();
        assert_eq!(
            graph.nodes()[&node].feedback_state.as_ref().unwrap().round,
            2
        );

        graph.pause_interrupted(&first_run, Utc::now()).unwrap();
        assert_eq!(graph.nodes()[&node].status, ExecutionNodeStatus::Pending);
        assert_eq!(
            graph.nodes()[&node].feedback_state.as_ref().unwrap().round,
            2
        );
        let second_run = RunId::parse("run-fixture-feedback-resume").unwrap();
        graph
            .claim(graph.revision(), second_run.clone(), first_run, Utc::now())
            .unwrap();
        graph.start_node(&node, &second_run, Utc::now()).unwrap();
        graph
            .complete_node(
                &node,
                &second_run,
                payload("composition.node-checkpoint", 1),
                Utc::now(),
            )
            .unwrap();
        assert_eq!(
            graph.nodes()[&node].feedback_state.as_ref().unwrap().round,
            2
        );
    }

    #[test]
    fn pause_interrupted_accepts_validation_and_repair_feedback_phases() {
        for repair in [false, true] {
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
            graph.begin_validation(&run_id, Utc::now()).unwrap();
            if repair {
                let node = ExecutionNodeId::parse("node.a").unwrap();
                let checkpoint_hash = graph.nodes()[&node]
                    .active_checkpoint
                    .as_ref()
                    .unwrap()
                    .sha256
                    .clone();
                graph
                    .begin_repair_campaign(
                        &run_id,
                        digest("d"),
                        vec![ExecutionRepairTargetSpec {
                            item_id: ItemId::parse("item-a").unwrap(),
                            node_id: node,
                            checkpoint_hash,
                            diagnostic_fingerprints: vec![digest("d")],
                            feedback: feedback(1),
                        }],
                        None,
                        Utc::now(),
                    )
                    .unwrap();
            }

            graph.pause_interrupted(&run_id, Utc::now()).unwrap();
            assert_eq!(graph.status(), ExecutionGraphStatus::Paused);
            assert!(graph.active_run_id().is_none());
            assert!(
                graph
                    .nodes()
                    .values()
                    .all(|node| node.status == ExecutionNodeStatus::Succeeded)
            );
        }
    }

    #[test]
    fn repair_output_feedback_retains_checkpoint_identity_across_recovery() {
        let (mut graph, first_run) = graph();
        for (id, value) in [("node.a", 1), ("node.b", 2)] {
            let id = ExecutionNodeId::parse(id).unwrap();
            graph.start_node(&id, &first_run, Utc::now()).unwrap();
            graph
                .complete_node(
                    &id,
                    &first_run,
                    payload("composition.node-checkpoint", value),
                    Utc::now(),
                )
                .unwrap();
        }
        graph.begin_validation(&first_run, Utc::now()).unwrap();
        let node = ExecutionNodeId::parse("node.a").unwrap();
        let checkpoint_hash = graph.nodes()[&node]
            .active_checkpoint
            .as_ref()
            .unwrap()
            .sha256
            .clone();
        graph
            .begin_repair_campaign(
                &first_run,
                digest("d"),
                vec![ExecutionRepairTargetSpec {
                    item_id: ItemId::parse("item-a").unwrap(),
                    node_id: node.clone(),
                    checkpoint_hash: checkpoint_hash.clone(),
                    diagnostic_fingerprints: vec![digest("d")],
                    feedback: feedback(1),
                }],
                None,
                Utc::now(),
            )
            .unwrap();
        graph
            .activate_repair_target(&first_run, Utc::now())
            .unwrap();
        graph
            .record_repair_output_feedback(
                &node,
                &first_run,
                checkpoint_hash.clone(),
                ExecutionOutputFeedback {
                    diagnostic_fingerprint: digest("e"),
                    candidate_sha256: digest("f"),
                    feedback: feedback(2),
                },
                Utc::now(),
            )
            .unwrap();
        let state = graph.nodes()[&node].feedback_state.as_ref().unwrap();
        assert_eq!(state.round, 1);
        assert_eq!(state.phase, ExecutionFeedbackPhase::OutputContract);
        assert_eq!(state.candidate_sha256.as_ref(), Some(&digest("f")));
        assert_eq!(state.checkpoint_hash.as_ref(), Some(&checkpoint_hash));

        graph.recover_stale_claim(Utc::now()).unwrap();
        let second_run = RunId::parse("run-fixture-repair-resume").unwrap();
        graph
            .claim(graph.revision(), second_run.clone(), first_run, Utc::now())
            .unwrap();
        assert_eq!(graph.status(), ExecutionGraphStatus::Repairing);
        assert_eq!(graph.semantic_request_count(), 2);
    }

    #[test]
    fn rejects_tampered_feedback_hash_and_legacy_schema() {
        let (mut tampered_graph, run_id) = graph();
        let node = ExecutionNodeId::parse("node.a").unwrap();
        tampered_graph
            .start_node(&node, &run_id, Utc::now())
            .unwrap();
        tampered_graph
            .record_output_feedback(
                &node,
                &run_id,
                ExecutionOutputFeedback {
                    diagnostic_fingerprint: digest("d"),
                    candidate_sha256: digest("e"),
                    feedback: feedback(1),
                },
                Utc::now(),
            )
            .unwrap();
        tampered_graph
            .nodes
            .get_mut(&node)
            .unwrap()
            .feedback_state
            .as_mut()
            .unwrap()
            .feedback
            .sha256 = digest("f");
        assert!(matches!(
            tampered_graph.validate(),
            Err(ExecutionGraphError::PayloadHashMismatch)
        ));

        let (graph, _) = graph();
        let mut encoded = serde_json::to_value(graph).unwrap();
        encoded["schemaVersion"] = serde_json::json!(2);
        assert!(matches!(
            serde_json::from_value::<ExecutionGraphRecord>(encoded),
            Err(error) if error.to_string().contains("execution graph metadata is invalid")
        ));
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
        wire["nodes"]["node.a"]["activeCheckpoint"]["sha256"] = serde_json::json!("f".repeat(64));
        assert!(serde_json::from_value::<ExecutionGraphRecord>(wire).is_err());
    }
}
