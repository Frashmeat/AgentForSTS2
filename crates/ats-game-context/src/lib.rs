//! Validated Game Pack contribution and immutable Truth Evidence contracts.

use ats_kernel::{ContributionId, FeatureId, PrimitiveId, SchemaRef};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ContributionContract {
    pub id: ContributionId,
    pub feature_id: FeatureId,
    pub schema: SchemaRef,
    pub required_primitives: Vec<PrimitiveId>,
}
