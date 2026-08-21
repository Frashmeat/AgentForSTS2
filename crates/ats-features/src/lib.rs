//! Product Feature contracts and vertical workflow ownership.

mod catalog;
pub mod composition;
pub mod composition_generate;
pub mod composition_plan;
pub mod generation_feedback;
pub mod item_definition;
pub mod log_analyze;
pub mod mod_generate_batch;
pub mod mod_generate_complex;
pub mod mod_generate_single;
pub mod mod_plan;
pub mod project_build;
pub mod project_create;
pub mod project_package;
pub mod prompt;
mod registry;
pub mod resource_prepare;

use ats_kernel::{ContributionId, FeatureId, SchemaRef};
use serde::{Deserialize, Serialize};

pub use catalog::{built_in_feature_contracts, built_in_feature_registry};
pub use registry::{FeatureRegistry, FeatureRegistryError, FeatureSpec};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeatureContract {
    pub id: FeatureId,
    pub request_schema: SchemaRef,
    pub result_schema: SchemaRef,
    pub required_contributions: Vec<ContributionId>,
}

#[cfg(test)]
pub(crate) fn fixture_behavior_json(item_type: &str) -> serde_json::Value {
    serde_json::json!({
        "adapter": {
            "id": "game.fixture.behavior",
            "version": 1,
            "implementationSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        "catalog": {
            "schemaVersion": 1,
            "id": "game.fixture.capabilities",
            "version": 1,
            "adapter": {
                "id": "game.fixture.behavior",
                "version": 1,
                "implementationSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            "capabilities": [{
                "id": "fixture.noop",
                "description": "Fixture no-op capability.",
                "maxInvocationsPerItem": 1,
                "parameters": []
            }],
            "itemTypes": [{
                "itemType": item_type,
                "allowedCapabilities": ["fixture.noop"],
                "minInvocations": 0,
                "maxInvocations": 1
            }]
        }
    })
}
