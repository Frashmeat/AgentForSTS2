//! Game-neutral execution contracts for Run, Artifact and external capability ports.

mod artifact;
mod payload;
mod run;

use ats_kernel::{PrimitiveId, SchemaRef};

pub use artifact::{
    ARTIFACT_MANIFEST_SCHEMA_VERSION, ArtifactContractError, ArtifactFileInput, ArtifactFileRecord,
    ArtifactManifest, ArtifactPublishRequest, ArtifactPublisher, PublishedArtifact,
    normalize_relative_path,
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
