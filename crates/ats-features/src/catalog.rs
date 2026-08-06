use ats_kernel::ContributionId;

use crate::composition_generate::CompositionGenerateFeature;
use crate::composition_plan::{CompositionPlanFeature, CompositionRetryNodeFeature};
use crate::log_analyze::LogAnalyzeFeature;
use crate::mod_generate_batch::BatchGenerateFeature;
use crate::mod_generate_complex::ComplexGenerateFeature;
use crate::mod_generate_single::SingleGenerateFeature;
use crate::mod_plan::ModPlanFeature;
use crate::project_build::ProjectBuildFeature;
use crate::project_create::ProjectCreateFeature;
use crate::project_package::ProjectPackageFeature;
use crate::resource_prepare::ResourcePrepareFeature;
use crate::{FeatureContract, FeatureRegistry, FeatureRegistryError, FeatureSpec};

#[must_use]
pub fn built_in_feature_contracts() -> Vec<FeatureContract> {
    vec![
        contract::<ProjectCreateFeature>(&["project.create.template"]),
        contract::<ModPlanFeature>(&["mod.plan.guidance"]),
        contract::<CompositionPlanFeature>(&["composition.plan.guidance"]),
        contract::<CompositionRetryNodeFeature>(&[
            "composition.plan.guidance",
            "composition.retry-node.guidance",
        ]),
        contract::<CompositionGenerateFeature>(&[
            "composition.generate",
            "mod.plan.guidance",
            "mod.generate.single",
            "resource.prepare.specs",
            "project.build.recipe",
            "project.package.layout",
        ]),
        contract::<ResourcePrepareFeature>(&["resource.prepare.specs"]),
        contract::<SingleGenerateFeature>(&["mod.generate.single", "resource.prepare.specs"]),
        contract::<BatchGenerateFeature>(&[
            "mod.generate.batch",
            "mod.plan.guidance",
            "mod.generate.single",
            "resource.prepare.specs",
        ]),
        contract::<ComplexGenerateFeature>(&[
            "mod.generate.complex",
            "mod.plan.guidance",
            "mod.generate.batch",
            "mod.generate.single",
            "resource.prepare.specs",
            "project.build.recipe",
            "project.package.layout",
        ]),
        contract::<LogAnalyzeFeature>(&["log.analyze.rules"]),
        contract::<ProjectBuildFeature>(&["project.build.recipe"]),
        contract::<ProjectPackageFeature>(&["project.package.layout"]),
    ]
}

pub fn built_in_feature_registry() -> Result<FeatureRegistry, FeatureRegistryError> {
    let mut registry = FeatureRegistry::new();
    registry.register::<ProjectCreateFeature>()?;
    registry.register::<ModPlanFeature>()?;
    registry.register::<CompositionPlanFeature>()?;
    registry.register::<CompositionRetryNodeFeature>()?;
    registry.register::<CompositionGenerateFeature>()?;
    registry.register::<ResourcePrepareFeature>()?;
    registry.register::<SingleGenerateFeature>()?;
    registry.register::<BatchGenerateFeature>()?;
    registry.register::<ComplexGenerateFeature>()?;
    registry.register::<LogAnalyzeFeature>()?;
    registry.register::<ProjectBuildFeature>()?;
    registry.register::<ProjectPackageFeature>()?;
    Ok(registry)
}

fn contract<F: FeatureSpec>(contributions: &[&str]) -> FeatureContract {
    FeatureContract {
        id: F::id(),
        request_schema: F::request_schema(),
        result_schema: F::result_schema(),
        required_contributions: contributions
            .iter()
            .map(|id| ContributionId::parse(*id).expect("built-in contribution ID is valid"))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn built_in_catalog_has_unique_feature_contracts() {
        let contracts = built_in_feature_contracts();
        assert_eq!(contracts.len(), 12);
        let batch = contracts
            .iter()
            .find(|contract| contract.id.as_str() == "mod.generate.batch")
            .expect("batch contract is built in");
        assert_eq!(
            batch
                .required_contributions
                .iter()
                .map(ContributionId::as_str)
                .collect::<Vec<_>>(),
            vec![
                "mod.generate.batch",
                "mod.plan.guidance",
                "mod.generate.single",
                "resource.prepare.specs",
            ]
        );
        assert_eq!(
            contracts
                .iter()
                .map(|contract| contract.id.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            contracts.len()
        );
        let registry = built_in_feature_registry().unwrap();
        for contract in contracts {
            let payload = ats_runtime::VersionedPayload::from_typed(
                contract.request_schema.clone(),
                &serde_json::json!({}),
            )
            .unwrap();
            assert!(!matches!(
                registry.validate_request(&contract.id, &payload),
                Err(FeatureRegistryError::UnknownFeature)
            ));
        }
    }
}
