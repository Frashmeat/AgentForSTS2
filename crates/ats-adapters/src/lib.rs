//! Infrastructure implementations selected by the application composition roots.

mod artifact_store;
mod build_runner;
mod package_writer;
mod project_writer;
mod resource_store;
mod truth_store;
mod validation_runner;

use ats_kernel::{PrimitiveId, SchemaRef};

pub use artifact_store::{ArtifactStoreError, FileArtifactStore};
pub use build_runner::RegisteredBuildRunner;
pub use package_writer::ZipPackageWriter;
pub use project_writer::FileProjectWriter;
pub use resource_store::{FileResourceRepository, ResourceStoreError};
pub use truth_store::FileTruthSnapshotRepository;
pub use validation_runner::RegisteredValidationRunner;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AdapterContract {
    pub primitive_id: PrimitiveId,
    pub configuration_schema: SchemaRef,
}
