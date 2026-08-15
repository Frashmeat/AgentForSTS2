use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{
    ExecutionNodeId, FeatureId, GamePackId, ItemId, PipelineProfileId, PipelineProviderId,
    PrimitiveId, SchemaRef, SchemaVersion, Sha256Digest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelineProviderIdentity {
    pub id: PipelineProviderId,
    pub version: SchemaVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelineSelection {
    pub provider: PipelineProviderIdentity,
    pub profile_id: PipelineProfileId,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelineWorkItem {
    pub item_id: ItemId,
    pub definition_hash: Sha256Digest,
    #[serde(default)]
    pub depends_on: Vec<ItemId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelineResolveRequest {
    pub owner_feature_id: FeatureId,
    pub game_pack_id: GamePackId,
    pub game_pack_sha256: Sha256Digest,
    pub truth_snapshot_id: Sha256Digest,
    pub source_graph_digest: Sha256Digest,
    pub profile_id: PipelineProfileId,
    pub work_items: Vec<PipelineWorkItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelineValueContract {
    pub slot_id: String,
    pub schema: SchemaRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelinePrimitiveBinding {
    pub id: PrimitiveId,
    pub version: SchemaVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PipelineNodeScope {
    Composition,
    Item { item_id: ItemId },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum PipelineNodePhase {
    Prepare,
    Validate,
    Deliver,
    Publish,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineCheckpointPolicy {
    None,
    OnSuccess,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelineRetryClass {
    Never,
    ProviderTransport,
    SemanticFeedback,
    LocalTransient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PipelinePublishBarrier {
    BeforeCommit,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PipelineNode {
    pub node_id: ExecutionNodeId,
    pub scope: PipelineNodeScope,
    pub phase: PipelineNodePhase,
    pub primitive_id: PrimitiveId,
    pub primitive_version: SchemaVersion,
    #[serde(default)]
    pub consumes: Vec<PipelineValueContract>,
    pub produces: PipelineValueContract,
    #[serde(default)]
    pub depends_on: Vec<ExecutionNodeId>,
    pub checkpoint_policy: PipelineCheckpointPolicy,
    pub retry_class: PipelineRetryClass,
    #[serde(default)]
    pub validation: Vec<PipelinePrimitiveBinding>,
    pub publish_barrier: PipelinePublishBarrier,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedPipelineGraph {
    pub schema_version: u32,
    pub provider: PipelineProviderIdentity,
    pub profile_id: PipelineProfileId,
    pub owner_feature_id: FeatureId,
    pub game_pack_id: GamePackId,
    pub game_pack_sha256: Sha256Digest,
    pub truth_snapshot_id: Sha256Digest,
    pub source_graph_digest: Sha256Digest,
    pub nodes: Vec<PipelineNode>,
    pub graph_digest: Sha256Digest,
}

impl ResolvedPipelineGraph {
    pub fn new(
        provider: PipelineProviderIdentity,
        request: &PipelineResolveRequest,
        mut nodes: Vec<PipelineNode>,
    ) -> Result<Self, PipelineGraphError> {
        nodes.sort_by(|left, right| left.node_id.cmp(&right.node_id));
        for node in &mut nodes {
            node.depends_on.sort();
            node.consumes.sort_by(|left, right| {
                left.slot_id
                    .cmp(&right.slot_id)
                    .then_with(|| left.schema.id.cmp(&right.schema.id))
                    .then_with(|| left.schema.version.cmp(&right.schema.version))
            });
            node.validation.sort();
        }
        let mut graph = Self {
            schema_version: 1,
            provider,
            profile_id: request.profile_id.clone(),
            owner_feature_id: request.owner_feature_id.clone(),
            game_pack_id: request.game_pack_id.clone(),
            game_pack_sha256: request.game_pack_sha256.clone(),
            truth_snapshot_id: request.truth_snapshot_id.clone(),
            source_graph_digest: request.source_graph_digest.clone(),
            nodes,
            graph_digest: zero_digest(),
        };
        graph.graph_digest = graph.compute_digest()?;
        graph.validate()?;
        Ok(graph)
    }

    pub fn validate(&self) -> Result<(), PipelineGraphError> {
        if self.schema_version != 1 || self.nodes.is_empty() || self.nodes.len() > 512 {
            return Err(PipelineGraphError::InvalidGraph);
        }
        if self.compute_digest()? != self.graph_digest {
            return Err(PipelineGraphError::DigestMismatch);
        }

        let mut nodes = BTreeMap::new();
        let mut produced_slots = BTreeMap::new();
        for node in &self.nodes {
            validate_node(node)?;
            if nodes.insert(node.node_id.clone(), node).is_some() {
                return Err(PipelineGraphError::DuplicateNode);
            }
            if produced_slots
                .insert(
                    node.produces.slot_id.clone(),
                    (node.node_id.clone(), node.produces.schema.clone()),
                )
                .is_some()
            {
                return Err(PipelineGraphError::InvalidValueFlow);
            }
        }

        let mut incoming = BTreeMap::new();
        let mut dependents: BTreeMap<ExecutionNodeId, Vec<ExecutionNodeId>> = BTreeMap::new();
        for node in &self.nodes {
            let mut unique = BTreeSet::new();
            for dependency in &node.depends_on {
                if dependency == &node.node_id
                    || !nodes.contains_key(dependency)
                    || !unique.insert(dependency)
                    || nodes
                        .get(dependency)
                        .is_some_and(|parent| parent.phase > node.phase)
                {
                    return Err(PipelineGraphError::InvalidDependency);
                }
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .push(node.node_id.clone());
            }
            incoming.insert(node.node_id.clone(), node.depends_on.len());
        }

        let mut ready = incoming
            .iter()
            .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
            .collect::<BTreeSet<_>>();
        let mut visited = 0usize;
        while let Some(node_id) = ready.pop_first() {
            visited += 1;
            for dependent in dependents.get(&node_id).into_iter().flatten() {
                let count = incoming
                    .get_mut(dependent)
                    .ok_or(PipelineGraphError::InvalidDependency)?;
                *count -= 1;
                if *count == 0 {
                    ready.insert(dependent.clone());
                }
            }
        }
        if visited != self.nodes.len() {
            return Err(PipelineGraphError::Cycle);
        }

        for node in &self.nodes {
            for input in &node.consumes {
                if let Some((producer, schema)) = produced_slots.get(&input.slot_id)
                    && (schema != &input.schema || !is_ancestor(&nodes, producer, &node.node_id))
                {
                    return Err(PipelineGraphError::InvalidValueFlow);
                }
            }
        }

        let commit_nodes = self
            .nodes
            .iter()
            .filter(|node| node.publish_barrier == PipelinePublishBarrier::Commit)
            .collect::<Vec<_>>();
        if commit_nodes.len() != 1
            || commit_nodes[0].phase != PipelineNodePhase::Publish
            || self.nodes.iter().any(|node| {
                (node.phase == PipelineNodePhase::Publish)
                    != (node.publish_barrier == PipelinePublishBarrier::Commit)
            })
            || dependents
                .get(&commit_nodes[0].node_id)
                .is_some_and(|values| !values.is_empty())
            || ancestor_closure(&nodes, &commit_nodes[0].node_id).len() != self.nodes.len()
        {
            return Err(PipelineGraphError::InvalidPublishBarrier);
        }
        Ok(())
    }

    fn compute_digest(&self) -> Result<Sha256Digest, PipelineGraphError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct DigestInput<'a> {
            schema_version: u32,
            provider: &'a PipelineProviderIdentity,
            profile_id: &'a PipelineProfileId,
            owner_feature_id: &'a FeatureId,
            game_pack_id: &'a GamePackId,
            game_pack_sha256: &'a Sha256Digest,
            truth_snapshot_id: &'a Sha256Digest,
            source_graph_digest: &'a Sha256Digest,
            nodes: &'a [PipelineNode],
        }
        let bytes = serde_json::to_vec(&DigestInput {
            schema_version: self.schema_version,
            provider: &self.provider,
            profile_id: &self.profile_id,
            owner_feature_id: &self.owner_feature_id,
            game_pack_id: &self.game_pack_id,
            game_pack_sha256: &self.game_pack_sha256,
            truth_snapshot_id: &self.truth_snapshot_id,
            source_graph_digest: &self.source_graph_digest,
            nodes: &self.nodes,
        })
        .map_err(|_| PipelineGraphError::Serialization)?;
        Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
            .map_err(|_| PipelineGraphError::Serialization)
    }
}

pub trait GamePipelineProvider: Send + Sync {
    fn identity(&self) -> &PipelineProviderIdentity;

    fn resolve(
        &self,
        request: &PipelineResolveRequest,
    ) -> Result<ResolvedPipelineGraph, PipelineProviderError>;
}

#[derive(Default)]
pub struct GamePipelineRegistry {
    providers: BTreeMap<PipelineProviderIdentity, Box<dyn GamePipelineProvider>>,
    primitives: BTreeMap<PrimitiveId, SchemaVersion>,
}

impl GamePipelineRegistry {
    pub fn new(
        primitives: impl IntoIterator<Item = (PrimitiveId, SchemaVersion)>,
    ) -> Result<Self, PipelineRegistryError> {
        let mut registered = BTreeMap::new();
        for (id, version) in primitives {
            if registered.insert(id, version).is_some() {
                return Err(PipelineRegistryError::DuplicatePrimitive);
            }
        }
        Ok(Self {
            providers: BTreeMap::new(),
            primitives: registered,
        })
    }

    pub fn register(
        &mut self,
        provider: impl GamePipelineProvider + 'static,
    ) -> Result<(), PipelineRegistryError> {
        let identity = provider.identity().clone();
        if self.providers.contains_key(&identity) {
            return Err(PipelineRegistryError::DuplicateProvider);
        }
        self.providers.insert(identity, Box::new(provider));
        Ok(())
    }

    pub fn resolve(
        &self,
        selection: &PipelineSelection,
        request: &PipelineResolveRequest,
    ) -> Result<ResolvedPipelineGraph, PipelineRegistryError> {
        if selection.profile_id != request.profile_id {
            return Err(PipelineRegistryError::ContextMismatch);
        }
        validate_request(request)?;
        let provider = self
            .providers
            .get(&selection.provider)
            .ok_or(PipelineRegistryError::UnknownProvider)?;
        let graph = provider.resolve(request)?;
        self.validate_resolved(selection, &graph)?;
        if graph.provider != selection.provider
            || graph.profile_id != selection.profile_id
            || graph.owner_feature_id != request.owner_feature_id
            || graph.game_pack_id != request.game_pack_id
            || graph.game_pack_sha256 != request.game_pack_sha256
            || graph.truth_snapshot_id != request.truth_snapshot_id
            || graph.source_graph_digest != request.source_graph_digest
            || graph.nodes.iter().any(|node| match &node.scope {
                PipelineNodeScope::Composition => false,
                PipelineNodeScope::Item { item_id } => !request
                    .work_items
                    .iter()
                    .any(|item| &item.item_id == item_id),
            })
        {
            return Err(PipelineRegistryError::ContextMismatch);
        }
        Ok(graph)
    }

    pub fn validate_resolved(
        &self,
        selection: &PipelineSelection,
        graph: &ResolvedPipelineGraph,
    ) -> Result<(), PipelineRegistryError> {
        graph.validate()?;
        if graph.provider != selection.provider || graph.profile_id != selection.profile_id {
            return Err(PipelineRegistryError::ContextMismatch);
        }
        if !self.providers.contains_key(&selection.provider) {
            return Err(PipelineRegistryError::UnknownProvider);
        }
        for node in &graph.nodes {
            if self.primitives.get(&node.primitive_id) != Some(&node.primitive_version)
                || node
                    .validation
                    .iter()
                    .any(|primitive| self.primitives.get(&primitive.id) != Some(&primitive.version))
            {
                return Err(PipelineRegistryError::PrimitiveUnavailable);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PipelineProviderError {
    #[error("pipeline profile is unsupported")]
    UnsupportedProfile,
    #[error("pipeline request is invalid")]
    InvalidRequest,
    #[error("resolved pipeline graph is invalid")]
    InvalidGraph(#[from] PipelineGraphError),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PipelineRegistryError {
    #[error("pipeline provider identity is already registered")]
    DuplicateProvider,
    #[error("pipeline primitive identity is registered more than once")]
    DuplicatePrimitive,
    #[error("pipeline provider identity is not registered")]
    UnknownProvider,
    #[error("pipeline provider context does not match the pinned request")]
    ContextMismatch,
    #[error("pipeline request work-item graph is invalid")]
    InvalidRequest,
    #[error("pipeline primitive or version is unavailable")]
    PrimitiveUnavailable,
    #[error(transparent)]
    Provider(#[from] PipelineProviderError),
    #[error(transparent)]
    Graph(#[from] PipelineGraphError),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum PipelineGraphError {
    #[error("pipeline graph metadata or node count is invalid")]
    InvalidGraph,
    #[error("pipeline graph contains a duplicate node")]
    DuplicateNode,
    #[error("pipeline node dependency is missing, duplicated or self-referential")]
    InvalidDependency,
    #[error("pipeline graph contains a dependency cycle")]
    Cycle,
    #[error("pipeline node contract is invalid")]
    InvalidNode,
    #[error("pipeline typed value flow is invalid")]
    InvalidValueFlow,
    #[error("pipeline publish barrier is invalid")]
    InvalidPublishBarrier,
    #[error("pipeline graph digest does not match its canonical content")]
    DigestMismatch,
    #[error("pipeline graph cannot be serialized canonically")]
    Serialization,
}

fn ancestor_closure<'a>(
    nodes: &'a BTreeMap<ExecutionNodeId, &'a PipelineNode>,
    node_id: &ExecutionNodeId,
) -> BTreeSet<ExecutionNodeId> {
    let mut closure = BTreeSet::new();
    let mut pending = vec![node_id.clone()];
    while let Some(current) = pending.pop() {
        if !closure.insert(current.clone()) {
            continue;
        }
        if let Some(node) = nodes.get(&current) {
            pending.extend(node.depends_on.iter().cloned());
        }
    }
    closure
}

fn is_ancestor(
    nodes: &BTreeMap<ExecutionNodeId, &PipelineNode>,
    candidate: &ExecutionNodeId,
    node_id: &ExecutionNodeId,
) -> bool {
    candidate != node_id && ancestor_closure(nodes, node_id).contains(candidate)
}

fn validate_node(node: &PipelineNode) -> Result<(), PipelineGraphError> {
    if !valid_slot(&node.produces.slot_id)
        || node
            .consumes
            .iter()
            .any(|input| !valid_slot(&input.slot_id))
        || node
            .consumes
            .iter()
            .map(|input| &input.slot_id)
            .collect::<BTreeSet<_>>()
            .len()
            != node.consumes.len()
        || node.validation.iter().collect::<BTreeSet<_>>().len() != node.validation.len()
        || (node.publish_barrier == PipelinePublishBarrier::Commit
            && (node.checkpoint_policy != PipelineCheckpointPolicy::OnSuccess
                || !matches!(node.scope, PipelineNodeScope::Composition)))
    {
        return Err(PipelineGraphError::InvalidNode);
    }
    Ok(())
}

fn validate_request(request: &PipelineResolveRequest) -> Result<(), PipelineRegistryError> {
    if request.work_items.len() > 128 {
        return Err(PipelineRegistryError::InvalidRequest);
    }
    let ids = request
        .work_items
        .iter()
        .map(|item| &item.item_id)
        .collect::<BTreeSet<_>>();
    if ids.len() != request.work_items.len()
        || request.work_items.iter().any(|item| {
            let dependencies = item.depends_on.iter().collect::<BTreeSet<_>>();
            dependencies.len() != item.depends_on.len()
                || dependencies.contains(&item.item_id)
                || dependencies
                    .iter()
                    .any(|dependency| !ids.contains(*dependency))
        })
    {
        return Err(PipelineRegistryError::InvalidRequest);
    }
    let mut incoming = request
        .work_items
        .iter()
        .map(|item| (item.item_id.clone(), item.depends_on.len()))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<ItemId, Vec<ItemId>>::new();
    for item in &request.work_items {
        for dependency in &item.depends_on {
            dependents
                .entry(dependency.clone())
                .or_default()
                .push(item.item_id.clone());
        }
    }
    let mut ready = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<BTreeSet<_>>();
    let mut visited = 0usize;
    while let Some(item_id) = ready.pop_first() {
        visited += 1;
        for dependent in dependents.get(&item_id).into_iter().flatten() {
            let count = incoming
                .get_mut(dependent)
                .ok_or(PipelineRegistryError::InvalidRequest)?;
            *count -= 1;
            if *count == 0 {
                ready.insert(dependent.clone());
            }
        }
    }
    if visited != request.work_items.len() {
        return Err(PipelineRegistryError::InvalidRequest);
    }
    Ok(())
}

fn valid_slot(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn zero_digest() -> Sha256Digest {
    Sha256Digest::parse("0".repeat(64)).expect("zero SHA-256 is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ats_kernel::{ItemId, SchemaId, SchemaVersion};

    struct FixtureProvider {
        identity: PipelineProviderIdentity,
    }

    impl GamePipelineProvider for FixtureProvider {
        fn identity(&self) -> &PipelineProviderIdentity {
            &self.identity
        }

        fn resolve(
            &self,
            request: &PipelineResolveRequest,
        ) -> Result<ResolvedPipelineGraph, PipelineProviderError> {
            ResolvedPipelineGraph::new(
                self.identity.clone(),
                request,
                vec![node(
                    "fixture.commit",
                    Vec::new(),
                    PipelinePublishBarrier::Commit,
                )],
            )
            .map_err(Into::into)
        }
    }

    fn identity() -> PipelineProviderIdentity {
        PipelineProviderIdentity {
            id: PipelineProviderId::parse("fixture.provider").unwrap(),
            version: SchemaVersion::new(1).unwrap(),
        }
    }

    fn request() -> PipelineResolveRequest {
        PipelineResolveRequest {
            owner_feature_id: FeatureId::parse("composition.generate").unwrap(),
            game_pack_id: GamePackId::parse("fixture-game").unwrap(),
            game_pack_sha256: Sha256Digest::parse("1".repeat(64)).unwrap(),
            truth_snapshot_id: Sha256Digest::parse("2".repeat(64)).unwrap(),
            source_graph_digest: Sha256Digest::parse("3".repeat(64)).unwrap(),
            profile_id: PipelineProfileId::parse("fixture.data-only").unwrap(),
            work_items: Vec::new(),
        }
    }

    fn schema() -> SchemaRef {
        SchemaRef {
            id: SchemaId::parse("pipeline.fixture-output").unwrap(),
            version: SchemaVersion::new(1).unwrap(),
        }
    }

    fn node(
        id: &str,
        depends_on: Vec<ExecutionNodeId>,
        barrier: PipelinePublishBarrier,
    ) -> PipelineNode {
        PipelineNode {
            node_id: ExecutionNodeId::parse(id).unwrap(),
            scope: PipelineNodeScope::Composition,
            phase: if barrier == PipelinePublishBarrier::Commit {
                PipelineNodePhase::Publish
            } else {
                PipelineNodePhase::Prepare
            },
            primitive_id: PrimitiveId::parse("fixture.execute").unwrap(),
            primitive_version: SchemaVersion::new(1).unwrap(),
            consumes: Vec::new(),
            produces: PipelineValueContract {
                slot_id: format!("{id}.output"),
                schema: schema(),
            },
            depends_on,
            checkpoint_policy: PipelineCheckpointPolicy::OnSuccess,
            retry_class: PipelineRetryClass::Never,
            validation: Vec::new(),
            publish_barrier: barrier,
        }
    }

    #[test]
    fn registry_resolves_exact_provider_profile_and_primitive_version() {
        let version = SchemaVersion::new(1).unwrap();
        let mut registry =
            GamePipelineRegistry::new([(PrimitiveId::parse("fixture.execute").unwrap(), version)])
                .unwrap();
        registry
            .register(FixtureProvider {
                identity: identity(),
            })
            .unwrap();
        let selection = PipelineSelection {
            provider: identity(),
            profile_id: request().profile_id.clone(),
        };
        let graph = registry.resolve(&selection, &request()).unwrap();
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.graph_digest, graph.compute_digest().unwrap());
    }

    #[test]
    fn rejects_cycles_digest_drift_and_unknown_primitives() {
        let mut graph = ResolvedPipelineGraph::new(
            identity(),
            &request(),
            vec![node(
                "fixture.commit",
                Vec::new(),
                PipelinePublishBarrier::Commit,
            )],
        )
        .unwrap();
        graph.source_graph_digest = Sha256Digest::parse("4".repeat(64)).unwrap();
        assert_eq!(graph.validate(), Err(PipelineGraphError::DigestMismatch));

        let first = ExecutionNodeId::parse("fixture.first").unwrap();
        let second = ExecutionNodeId::parse("fixture.second").unwrap();
        assert_eq!(
            ResolvedPipelineGraph::new(
                identity(),
                &request(),
                vec![
                    node(
                        first.as_str(),
                        vec![second.clone()],
                        PipelinePublishBarrier::BeforeCommit,
                    ),
                    node(
                        second.as_str(),
                        vec![first],
                        PipelinePublishBarrier::BeforeCommit,
                    ),
                    node("fixture.commit", Vec::new(), PipelinePublishBarrier::Commit),
                ],
            ),
            Err(PipelineGraphError::Cycle)
        );

        let mut registry =
            GamePipelineRegistry::new(Vec::<(PrimitiveId, SchemaVersion)>::new()).unwrap();
        registry
            .register(FixtureProvider {
                identity: identity(),
            })
            .unwrap();
        let selection = PipelineSelection {
            provider: identity(),
            profile_id: request().profile_id.clone(),
        };
        assert_eq!(
            registry.resolve(&selection, &request()),
            Err(PipelineRegistryError::PrimitiveUnavailable)
        );

        let mut cyclic = request();
        cyclic.work_items = vec![
            PipelineWorkItem {
                item_id: ItemId::parse("first").unwrap(),
                definition_hash: Sha256Digest::parse("5".repeat(64)).unwrap(),
                depends_on: vec![ItemId::parse("second").unwrap()],
            },
            PipelineWorkItem {
                item_id: ItemId::parse("second").unwrap(),
                definition_hash: Sha256Digest::parse("6".repeat(64)).unwrap(),
                depends_on: vec![ItemId::parse("first").unwrap()],
            },
        ];
        assert_eq!(
            registry.resolve(&selection, &cyclic),
            Err(PipelineRegistryError::InvalidRequest)
        );
    }

    #[test]
    fn rejects_typed_value_drift_duplicate_outputs_and_incomplete_publish_closure() {
        let first_id = ExecutionNodeId::parse("fixture.first").unwrap();
        let mut wrong_schema_commit = node(
            "fixture.commit",
            vec![first_id.clone()],
            PipelinePublishBarrier::Commit,
        );
        wrong_schema_commit.consumes = vec![PipelineValueContract {
            slot_id: "fixture.first.output".into(),
            schema: SchemaRef {
                id: SchemaId::parse("pipeline.wrong-output").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
        }];
        assert_eq!(
            ResolvedPipelineGraph::new(
                identity(),
                &request(),
                vec![
                    node(
                        first_id.as_str(),
                        Vec::new(),
                        PipelinePublishBarrier::BeforeCommit,
                    ),
                    wrong_schema_commit,
                ],
            ),
            Err(PipelineGraphError::InvalidValueFlow)
        );

        let first = node(
            first_id.as_str(),
            Vec::new(),
            PipelinePublishBarrier::BeforeCommit,
        );
        let mut duplicate = node(
            "fixture.commit",
            vec![first_id],
            PipelinePublishBarrier::Commit,
        );
        duplicate.produces = first.produces.clone();
        assert_eq!(
            ResolvedPipelineGraph::new(identity(), &request(), vec![first, duplicate]),
            Err(PipelineGraphError::InvalidValueFlow)
        );

        assert_eq!(
            ResolvedPipelineGraph::new(
                identity(),
                &request(),
                vec![
                    node(
                        "fixture.orphan",
                        Vec::new(),
                        PipelinePublishBarrier::BeforeCommit,
                    ),
                    node(
                        "fixture.commit",
                        Vec::new(),
                        PipelinePublishBarrier::Commit,
                    ),
                ],
            ),
            Err(PipelineGraphError::InvalidPublishBarrier)
        );
    }

    #[test]
    fn duplicate_registration_is_rejected_without_replacing_the_original() {
        let primitive = PrimitiveId::parse("fixture.execute").unwrap();
        assert!(matches!(
            GamePipelineRegistry::new([
                (primitive.clone(), SchemaVersion::new(1).unwrap()),
                (primitive, SchemaVersion::new(2).unwrap()),
            ]),
            Err(PipelineRegistryError::DuplicatePrimitive)
        ));

        let mut registry = GamePipelineRegistry::new([(
            PrimitiveId::parse("fixture.execute").unwrap(),
            SchemaVersion::new(1).unwrap(),
        )])
        .unwrap();
        registry
            .register(FixtureProvider {
                identity: identity(),
            })
            .unwrap();
        assert_eq!(
            registry.register(FixtureProvider {
                identity: identity(),
            }),
            Err(PipelineRegistryError::DuplicateProvider)
        );
        let selection = PipelineSelection {
            provider: identity(),
            profile_id: request().profile_id.clone(),
        };
        assert!(registry.resolve(&selection, &request()).is_ok());
    }
}
