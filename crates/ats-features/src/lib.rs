//! Product Feature contracts and vertical workflow ownership.

pub mod log_analyze;
pub mod prompt;
mod registry;

use ats_kernel::{ContributionId, FeatureId, SchemaRef};

pub use registry::{FeatureRegistry, FeatureRegistryError, FeatureSpec};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FeatureContract {
    pub id: FeatureId,
    pub request_schema: SchemaRef,
    pub result_schema: SchemaRef,
    pub required_contributions: Vec<ContributionId>,
}
