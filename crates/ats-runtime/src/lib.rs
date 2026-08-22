//! Game-neutral execution contracts for Run, Artifact and external capability ports.

mod artifact;
mod delivery;
mod execution;
mod execution_graph;
mod media;
mod model;
mod payload;
mod repository;
mod run;

use ats_kernel::{PrimitiveId, SchemaRef};

pub use artifact::{
    ARTIFACT_MANIFEST_SCHEMA_VERSION, ArtifactContractError, ArtifactFileInput, ArtifactFileRecord,
    ArtifactManifest, ArtifactPublishRequest, ArtifactPublisher, PublishedArtifact,
    normalize_relative_path,
};
pub use delivery::{
    BuildError, BuildRunner, BuildStepReport, BuildStepRequest, PackageEntry, PackageError,
    PackagePrepareRequest, PackageReport, PackageWriter, PendingPackageOutput,
    validate_package_request,
};
pub use execution::{
    CancellationToken, PendingProjectStage, PendingProjectWrites, ProjectFileWrite,
    ProjectFileWriter, ProjectStageError, ProjectStageRequest, ProjectStager, ProjectWriteError,
    ValidationError, ValidationIssue, ValidationIssueRepairability, ValidationIssueSeverity,
    ValidationReport, ValidationRequest, ValidationRunner, validate_project_writes,
};
pub use execution_graph::{
    EXECUTION_GRAPH_SCHEMA_VERSION, ExecutionAdjustment, ExecutionCommitIntent, ExecutionFailure,
    ExecutionFeedbackPhase, ExecutionGraphError, ExecutionGraphRecord, ExecutionGraphStatus,
    ExecutionNodeFeedbackState, ExecutionNodeRecord, ExecutionNodeSpec, ExecutionNodeStatus,
    ExecutionOutputFeedback, ExecutionPublicationIntent, ExecutionRepairCampaign,
    ExecutionRepairTarget, ExecutionRepairTargetSpec, ExecutionRepairTargetStatus,
    HashedExecutionPayload, LogicalAttemptOutcome, LogicalNodeAttempt, MAX_SEMANTIC_REQUESTS,
    RepairCause, hash_json,
};
pub use media::{MediaClient, MediaError, MediaRequest, MediaRequestSnapshot, MediaResponse};
pub use model::{
    FinishReason, MAX_MODEL_OUTPUT_TOKENS, ModelClient, ModelContextBinding, ModelError,
    ModelGamePackRef, ModelMessage, ModelMessageRole, ModelOutputContract, ModelRequest,
    ModelRequestCommitment, ModelRequestCommitmentIdentity, ModelRequestContextBinding,
    ModelRequestError, ModelRequestLimits, ModelRequestSnapshot, ModelResourceRef, ModelResponse,
    ModelStream, ModelStreamEvent, RecipeRef, TokenUsage,
};
pub use payload::{PayloadError, VersionedPayload};
pub use repository::{
    ExecutionGraphRecovery, ExecutionGraphRepository, ExecutionGraphRepositoryError, RunRepository,
    RunRepositoryError,
};
pub use run::{
    CancellationReason, RUN_RECORD_SCHEMA_VERSION, RunFailure, RunId, RunLifecycleError,
    RunProgress, RunRecord, RunStatus, RunSummary, RunTimelineEvent, RunTimelineEventKind,
    RunTransition,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PrimitiveContract {
    pub id: PrimitiveId,
    pub request_schema: SchemaRef,
    pub result_schema: SchemaRef,
}
