//! Product Feature contracts and vertical workflow ownership.

pub mod log_analyze;
pub mod mod_generate_batch;
pub mod mod_generate_complex;
pub mod mod_generate_single;
pub mod mod_plan;
pub mod project_build;
pub mod project_package;
pub mod prompt;
mod registry;
pub mod resource_prepare;

use ats_kernel::{ContributionId, FeatureId, SchemaRef};

pub use registry::{FeatureRegistry, FeatureRegistryError, FeatureSpec};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FeatureContract {
    pub id: FeatureId,
    pub request_schema: SchemaRef,
    pub result_schema: SchemaRef,
    pub required_contributions: Vec<ContributionId>,
}
