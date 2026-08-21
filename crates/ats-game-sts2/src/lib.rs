//! Trusted STS2 pipeline descriptions. External IO remains behind runtime ports.

mod behavior;

pub use behavior::{
    BEHAVIOR_ADAPTER_ID, BEHAVIOR_ADAPTER_IMPLEMENTATION_SHA256, Sts2BehaviorAdapter,
};

use ats_game_context::{
    GamePipelineProvider, PipelineCheckpointPolicy, PipelineNode, PipelineNodePhase,
    PipelineNodeScope, PipelinePrimitiveBinding, PipelineProviderError, PipelineProviderIdentity,
    PipelinePublishBarrier, PipelineResolveRequest, PipelineRetryClass, PipelineValueContract,
    ResolvedPipelineGraph,
};
use ats_kernel::{
    ExecutionNodeId, FeatureId, PipelineProviderId, PrimitiveId, SchemaId, SchemaRef, SchemaVersion,
};

pub const PROVIDER_ID: &str = "game.sts2";
pub const COMPOSITION_PROFILE_ID: &str = "sts2.composition-generate";

pub struct Sts2PipelineProvider {
    identity: PipelineProviderIdentity,
}

impl Sts2PipelineProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            identity: PipelineProviderIdentity {
                id: PipelineProviderId::parse(PROVIDER_ID)
                    .expect("built-in STS2 Provider ID is valid"),
                version: version(),
            },
        }
    }
}

impl Default for Sts2PipelineProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GamePipelineProvider for Sts2PipelineProvider {
    fn identity(&self) -> &PipelineProviderIdentity {
        &self.identity
    }

    fn resolve(
        &self,
        request: &PipelineResolveRequest,
    ) -> Result<ResolvedPipelineGraph, PipelineProviderError> {
        if request.profile_id.as_str() != COMPOSITION_PROFILE_ID
            || request.owner_feature_id
                != FeatureId::parse("composition.generate").expect("built-in Feature ID is valid")
        {
            return Err(PipelineProviderError::UnsupportedProfile);
        }
        if request.work_items.is_empty() {
            return Err(PipelineProviderError::InvalidRequest);
        }

        let mut nodes = Vec::with_capacity(request.work_items.len() * 3 + 5);
        let mut previous_render = None;
        let mut render_outputs = Vec::with_capacity(request.work_items.len());
        for (index, item) in request.work_items.iter().enumerate() {
            let plan_id = node_id(format!("item.{index:03}.plan"))?;
            let behavior_id = node_id(format!("item.{index:03}.behavior"))?;
            let render_id = node_id(format!("item.{index:03}.render"))?;
            let definition_slot = format!("item.{index:03}.definition");
            let plan_slot = format!("item.{index:03}.plan-checkpoint");
            let behavior_slot = format!("item.{index:03}.behavior-checkpoint");
            let render_slot = format!("item.{index:03}.render-checkpoint");
            let plan_dependencies = previous_render.into_iter().collect();
            nodes.push(PipelineNode {
                node_id: plan_id.clone(),
                scope: PipelineNodeScope::Item {
                    item_id: item.item_id.clone(),
                },
                phase: PipelineNodePhase::Prepare,
                primitive_id: primitive("feature.mod-plan"),
                primitive_version: version(),
                consumes: vec![value(&definition_slot, "pipeline.item-definition", 2)],
                produces: value(
                    &plan_slot,
                    "feature.composition-generate-plan-checkpoint",
                    1,
                ),
                depends_on: plan_dependencies,
                checkpoint_policy: PipelineCheckpointPolicy::OnSuccess,
                retry_class: PipelineRetryClass::ProviderTransport,
                validation: Vec::new(),
                publish_barrier: PipelinePublishBarrier::BeforeCommit,
            });
            nodes.push(PipelineNode {
                node_id: behavior_id.clone(),
                scope: PipelineNodeScope::Item {
                    item_id: item.item_id.clone(),
                },
                phase: PipelineNodePhase::Prepare,
                primitive_id: primitive("feature.composition-behavior"),
                primitive_version: version(),
                consumes: vec![
                    value(&definition_slot, "pipeline.item-definition", 2),
                    value(
                        &plan_slot,
                        "feature.composition-generate-plan-checkpoint",
                        1,
                    ),
                ],
                produces: value(
                    &behavior_slot,
                    "feature.composition-generate-behavior-checkpoint",
                    1,
                ),
                depends_on: vec![plan_id.clone()],
                checkpoint_policy: PipelineCheckpointPolicy::OnSuccess,
                retry_class: PipelineRetryClass::SemanticFeedback,
                validation: Vec::new(),
                publish_barrier: PipelinePublishBarrier::BeforeCommit,
            });
            nodes.push(PipelineNode {
                node_id: render_id.clone(),
                scope: PipelineNodeScope::Item {
                    item_id: item.item_id.clone(),
                },
                phase: PipelineNodePhase::Prepare,
                primitive_id: primitive("game.behavior-render"),
                primitive_version: version(),
                consumes: vec![
                    value(&definition_slot, "pipeline.item-definition", 2),
                    value(
                        &behavior_slot,
                        "feature.composition-generate-behavior-checkpoint",
                        1,
                    ),
                ],
                produces: value(
                    &render_slot,
                    "feature.composition-generate-render-checkpoint",
                    1,
                ),
                depends_on: vec![behavior_id],
                checkpoint_policy: PipelineCheckpointPolicy::OnSuccess,
                retry_class: PipelineRetryClass::Never,
                validation: vec![primitive_binding("code.dotnet-validate")],
                publish_barrier: PipelinePublishBarrier::BeforeCommit,
            });
            render_outputs.push(value(
                &render_slot,
                "feature.composition-generate-render-checkpoint",
                1,
            ));
            previous_render = Some(render_id);
        }

        let finalize = node_id("composition.finalize")?;
        nodes.push(stage(
            finalize.clone(),
            primitive("feature.composition-finalize"),
            PipelineNodePhase::Prepare,
            previous_render.into_iter().collect(),
            stage_values(
                render_outputs,
                value(
                    "composition.prepared-checkpoint",
                    "feature.composition-generate-finalize-checkpoint",
                    3,
                ),
            ),
            PipelineRetryClass::Never,
            PipelinePublishBarrier::BeforeCommit,
        ));
        let validate = node_id("composition.validate")?;
        nodes.push(stage(
            validate.clone(),
            primitive("code.dotnet-validate"),
            PipelineNodePhase::Validate,
            vec![finalize],
            stage_values(
                vec![value(
                    "composition.prepared-checkpoint",
                    "feature.composition-generate-finalize-checkpoint",
                    3,
                )],
                value("composition.validated-tree", "pipeline.validated-tree", 1),
            ),
            PipelineRetryClass::SemanticFeedback,
            PipelinePublishBarrier::BeforeCommit,
        ));
        let build = node_id("project.build")?;
        nodes.push(stage(
            build.clone(),
            primitive("feature.project-build"),
            PipelineNodePhase::Deliver,
            vec![validate],
            stage_values(
                vec![value(
                    "composition.validated-tree",
                    "pipeline.validated-tree",
                    1,
                )],
                value("project.build-output", "pipeline.build-output", 1),
            ),
            PipelineRetryClass::LocalTransient,
            PipelinePublishBarrier::BeforeCommit,
        ));
        let package = node_id("project.package")?;
        nodes.push(stage(
            package.clone(),
            primitive("feature.project-package"),
            PipelineNodePhase::Deliver,
            vec![build],
            stage_values(
                vec![value("project.build-output", "pipeline.build-output", 1)],
                value("project.package-output", "pipeline.package-output", 1),
            ),
            PipelineRetryClass::LocalTransient,
            PipelinePublishBarrier::BeforeCommit,
        ));
        nodes.push(stage(
            node_id("composition.publish")?,
            primitive("storage.atomic-publish"),
            PipelineNodePhase::Publish,
            vec![package],
            stage_values(
                vec![value(
                    "project.package-output",
                    "pipeline.package-output",
                    1,
                )],
                value("composition.publication", "pipeline.publication", 1),
            ),
            PipelineRetryClass::LocalTransient,
            PipelinePublishBarrier::Commit,
        ));

        ResolvedPipelineGraph::new(self.identity.clone(), request, nodes).map_err(Into::into)
    }
}

fn stage(
    node_id: ExecutionNodeId,
    primitive_id: PrimitiveId,
    phase: PipelineNodePhase,
    depends_on: Vec<ExecutionNodeId>,
    values: PipelineStageValues,
    retry_class: PipelineRetryClass,
    publish_barrier: PipelinePublishBarrier,
) -> PipelineNode {
    PipelineNode {
        node_id,
        scope: PipelineNodeScope::Composition,
        phase,
        primitive_id,
        primitive_version: version(),
        consumes: values.consumes,
        produces: values.produces,
        depends_on,
        checkpoint_policy: PipelineCheckpointPolicy::OnSuccess,
        retry_class,
        validation: Vec::new(),
        publish_barrier,
    }
}

struct PipelineStageValues {
    consumes: Vec<PipelineValueContract>,
    produces: PipelineValueContract,
}

fn stage_values(
    consumes: Vec<PipelineValueContract>,
    produces: PipelineValueContract,
) -> PipelineStageValues {
    PipelineStageValues { consumes, produces }
}

fn node_id(value: impl Into<String>) -> Result<ExecutionNodeId, PipelineProviderError> {
    ExecutionNodeId::parse(value).map_err(|_| PipelineProviderError::InvalidRequest)
}

fn primitive(value: &str) -> PrimitiveId {
    PrimitiveId::parse(value).expect("built-in pipeline Primitive ID is valid")
}

fn primitive_binding(value: &str) -> PipelinePrimitiveBinding {
    PipelinePrimitiveBinding {
        id: primitive(value),
        version: version(),
    }
}

fn value(slot_id: &str, schema_id: &str, schema_version: u32) -> PipelineValueContract {
    PipelineValueContract {
        slot_id: slot_id.into(),
        schema: SchemaRef {
            id: SchemaId::parse(schema_id).expect("built-in pipeline schema ID is valid"),
            version: SchemaVersion::new(schema_version)
                .expect("built-in pipeline schema version is valid"),
        },
    }
}

fn version() -> SchemaVersion {
    SchemaVersion::new(1).expect("built-in version is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ats_game_context::{
        GamePipelineRegistry, PipelineNodeScope, PipelineSelection, PipelineWorkItem,
    };
    use ats_kernel::{GamePackId, ItemId, PipelineProfileId, Sha256Digest};

    fn request(items: usize) -> PipelineResolveRequest {
        PipelineResolveRequest {
            owner_feature_id: FeatureId::parse("composition.generate").unwrap(),
            game_pack_id: GamePackId::parse("sts2").unwrap(),
            game_pack_sha256: Sha256Digest::parse("1".repeat(64)).unwrap(),
            truth_snapshot_id: Sha256Digest::parse("2".repeat(64)).unwrap(),
            source_graph_digest: Sha256Digest::parse("3".repeat(64)).unwrap(),
            profile_id: PipelineProfileId::parse(COMPOSITION_PROFILE_ID).unwrap(),
            work_items: (0..items)
                .map(|index| PipelineWorkItem {
                    item_id: ItemId::parse(format!("item-{index:03}")).unwrap(),
                    definition_hash: Sha256Digest::parse(format!("{index:064x}")).unwrap(),
                    depends_on: Vec::new(),
                })
                .collect(),
        }
    }

    fn primitives() -> Vec<(PrimitiveId, SchemaVersion)> {
        [
            "feature.mod-plan",
            "feature.composition-behavior",
            "game.behavior-render",
            "feature.composition-finalize",
            "code.dotnet-validate",
            "feature.project-build",
            "feature.project-package",
            "storage.atomic-publish",
        ]
        .into_iter()
        .map(|id| (PrimitiveId::parse(id).unwrap(), version()))
        .collect()
    }

    #[test]
    fn materializes_stable_sts2_pipeline_with_delivery_behind_one_commit_barrier() {
        let mut registry = GamePipelineRegistry::new(primitives()).unwrap();
        registry.register(Sts2PipelineProvider::new()).unwrap();
        let selection = PipelineSelection {
            provider: Sts2PipelineProvider::new().identity().clone(),
            profile_id: PipelineProfileId::parse(COMPOSITION_PROFILE_ID).unwrap(),
        };
        let first = registry.resolve(&selection, &request(2)).unwrap();
        let second = registry.resolve(&selection, &request(2)).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.nodes.len(), 11);
        assert_eq!(
            first
                .nodes
                .iter()
                .filter(|node| matches!(node.scope, PipelineNodeScope::Item { .. }))
                .count(),
            6
        );
        assert_eq!(
            first
                .nodes
                .iter()
                .filter(|node| node.publish_barrier == PipelinePublishBarrier::Commit)
                .count(),
            1
        );
    }

    struct DataOnlyProvider {
        identity: PipelineProviderIdentity,
    }

    impl GamePipelineProvider for DataOnlyProvider {
        fn identity(&self) -> &PipelineProviderIdentity {
            &self.identity
        }

        fn resolve(
            &self,
            request: &PipelineResolveRequest,
        ) -> Result<ResolvedPipelineGraph, PipelineProviderError> {
            let render = node_id("data.render")?;
            ResolvedPipelineGraph::new(
                self.identity.clone(),
                request,
                vec![
                    stage(
                        render.clone(),
                        primitive("data.render-json"),
                        PipelineNodePhase::Prepare,
                        Vec::new(),
                        stage_values(
                            Vec::new(),
                            value(
                                "composition.prepared-checkpoint",
                                "feature.composition-generate-finalize-checkpoint",
                                3,
                            ),
                        ),
                        PipelineRetryClass::Never,
                        PipelinePublishBarrier::BeforeCommit,
                    ),
                    stage(
                        node_id("data.publish")?,
                        primitive("storage.atomic-publish"),
                        PipelineNodePhase::Publish,
                        vec![render],
                        stage_values(
                            vec![value(
                                "composition.prepared-checkpoint",
                                "feature.composition-generate-finalize-checkpoint",
                                3,
                            )],
                            value("data.publication", "pipeline.publication", 1),
                        ),
                        PipelineRetryClass::LocalTransient,
                        PipelinePublishBarrier::Commit,
                    ),
                ],
            )
            .map_err(Into::into)
        }
    }

    #[test]
    fn data_only_provider_has_no_model_or_sts2_toolchain_node() {
        let provider = DataOnlyProvider {
            identity: PipelineProviderIdentity {
                id: PipelineProviderId::parse("fixture.data-only").unwrap(),
                version: version(),
            },
        };
        let profile_id = PipelineProfileId::parse("fixture.data-only").unwrap();
        let mut request = request(1);
        request.profile_id = profile_id.clone();
        request.game_pack_id = GamePackId::parse("fixture-game").unwrap();
        let mut registry = GamePipelineRegistry::new([
            (primitive("data.render-json"), version()),
            (primitive("storage.atomic-publish"), version()),
        ])
        .unwrap();
        let selection = PipelineSelection {
            provider: provider.identity().clone(),
            profile_id,
        };
        registry.register(provider).unwrap();
        let graph = registry.resolve(&selection, &request).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.nodes.iter().all(|node| {
            !node.primitive_id.as_str().contains("model")
                && !node.primitive_id.as_str().contains("dotnet")
                && !node.primitive_id.as_str().contains("godot")
        }));
    }
}
