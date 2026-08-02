//! Game-neutral execution contracts for Run, Artifact and external capability ports.

mod artifact;
mod execution;
mod media;
mod model;
mod payload;
mod run;

use ats_kernel::{PrimitiveId, SchemaRef};

pub use artifact::{
    ARTIFACT_MANIFEST_SCHEMA_VERSION, ArtifactContractError, ArtifactFileInput, ArtifactFileRecord,
    ArtifactManifest, ArtifactPublishRequest, ArtifactPublisher, PublishedArtifact,
    normalize_relative_path,
};
pub use execution::{
    CancellationToken, PendingProjectWrites, ProjectFileWrite, ProjectFileWriter,
    ProjectWriteError, ValidationError, ValidationReport, ValidationRequest, ValidationRunner,
    validate_project_writes,
};
pub use media::{MediaClient, MediaError, MediaRequest, MediaRequestSnapshot, MediaResponse};
pub use model::{
    FinishReason, ModelClient, ModelError, ModelGamePackRef, ModelMessage, ModelMessageRole,
    ModelOutputContract, ModelRequest, ModelRequestError, ModelRequestSnapshot, ModelResourceRef,
    ModelResponse, ModelStream, ModelStreamEvent, RecipeRef, TokenUsage,
};
pub use payload::{PayloadError, VersionedPayload};
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
