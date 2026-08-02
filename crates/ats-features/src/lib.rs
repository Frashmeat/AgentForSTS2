//! Product Feature contracts and vertical workflow ownership.

use ats_kernel::{ContributionId, FeatureId, SchemaRef};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FeatureContract {
    pub id: FeatureId,
    pub request_schema: SchemaRef,
    pub result_schema: SchemaRef,
    pub required_contributions: Vec<ContributionId>,
}
