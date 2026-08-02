//! Infrastructure implementations selected by the application composition roots.

mod artifact_store;
mod resource_store;
mod truth_store;

use ats_kernel::{PrimitiveId, SchemaRef};

pub use artifact_store::{ArtifactStoreError, FileArtifactStore};
pub use resource_store::{FileResourceRepository, ResourceStoreError};
pub use truth_store::FileTruthSnapshotRepository;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AdapterContract {
    pub primitive_id: PrimitiveId,
    pub configuration_schema: SchemaRef,
}
