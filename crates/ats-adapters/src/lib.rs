//! Infrastructure implementations selected by the application composition roots.

mod artifact_store;

use ats_kernel::{PrimitiveId, SchemaRef};

pub use artifact_store::{ArtifactStoreError, FileArtifactStore};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AdapterContract {
    pub primitive_id: PrimitiveId,
    pub configuration_schema: SchemaRef,
}
