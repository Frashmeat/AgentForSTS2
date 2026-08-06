use std::collections::{BTreeMap, BTreeSet};

use ats_game_context::{
    ItemCapabilityCatalog, ItemReferenceKind, LoadedGamePack, VerifiedContributionSet,
    VerifiedTruthSnapshot,
};
use ats_kernel::{
    CompositionDraftId, GamePackId, ItemId, ItemReferenceSlotId, ItemTypeId, Sha256Digest,
};
use ats_workspace::{
    AtomicItemRepository, AtomicItemSaveError, AtomicItemSaveRequest, CompositionDraft,
    ItemCompositionProfile, ItemReferenceBinding, ItemRepository, ItemRepositoryErrorKind,
    ResourceRepository, StoredItemDefinition,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::item_definition::{ItemDefinitionValidationMode, ItemDefinitionValidator};
use crate::mod_generate_single::validate_definition_resources;

pub const RESOLVED_ITEM_GRAPH_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionDraftRef {
    pub draft_id: CompositionDraftId,
    pub revision: u64,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedPinnedEdge {
    pub source_item_id: ItemId,
    pub source_definition_hash: Sha256Digest,
    pub slot_id: ItemReferenceSlotId,
    pub target_item_id: ItemId,
    pub target_definition_hash: Sha256Digest,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedIdentityEdge {
    pub source_item_id: ItemId,
    pub source_definition_hash: Sha256Digest,
    pub slot_id: ItemReferenceSlotId,
    pub target_item_id: ItemId,
    pub expected_item_type: ItemTypeId,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedItemGraph {
    pub schema_version: u32,
    pub game_pack_id: GamePackId,
    pub game_pack_sha256: Sha256Digest,
    pub truth_snapshot_id: Sha256Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft: Option<CompositionDraftRef>,
    pub root_item_id: ItemId,
    pub root_definition_hash: Sha256Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub composition_profile: Option<ItemCompositionProfile>,
    pub nodes: Vec<StoredItemDefinition>,
    pub pinned_edges: Vec<ResolvedPinnedEdge>,
    pub identity_edges: Vec<ResolvedIdentityEdge>,
    pub graph_digest: Sha256Digest,
}

impl ResolvedItemGraph {
    pub fn resolve<I, R>(
        pack: &LoadedGamePack,
        truth: &VerifiedTruthSnapshot,
        resource_contributions: &VerifiedContributionSet,
        item_repository: &I,
        resource_repository: &R,
        root: StoredItemDefinition,
        draft: Option<CompositionDraftRef>,
    ) -> Result<Self, CompositionGraphError>
    where
        I: ItemRepository + ?Sized,
        R: ResourceRepository + ?Sized,
    {
        if truth.manifest().game_pack_id() != pack.id()
            || truth.manifest().game_pack_sha256() != pack.content_sha256()
        {
            return Err(CompositionGraphError::ContextMismatch);
        }
        if draft.as_ref().is_some_and(|value| value.revision == 0) {
            return Err(CompositionGraphError::InvalidProvenance);
        }
        let capabilities = ItemCapabilityCatalog::evaluate(pack, Some(truth))
            .map_err(|_| CompositionGraphError::ContextMismatch)?;
        let mut load = |item_id: &ItemId, hash: &Sha256Digest| {
            item_repository
                .load_version(item_id, hash)
                .map_err(|error| match I::classify_error(&error) {
                    ItemRepositoryErrorKind::NotFound => CompositionGraphError::ItemMissing,
                    ItemRepositoryErrorKind::Storage => CompositionGraphError::Storage,
                })
        };
        let mut readiness = |definition: &StoredItemDefinition| {
            let ready = capabilities.item_types.iter().any(|capability| {
                capability.descriptor.id() == &definition.definition.item_type && capability.ready
            });
            if !ready {
                return Err(CompositionGraphError::Readiness);
            }
            validate_definition_resources(
                pack,
                resource_contributions,
                resource_repository,
                definition,
            )
            .map_err(|_| CompositionGraphError::Readiness)
        };
        let collected = collect_graph(pack, vec![root.clone()], &mut load, &mut readiness)?;
        build_resolved_graph(pack, truth, root, draft, collected)
    }
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionConfirmation {
    pub draft: CompositionDraftRef,
    pub definitions: Vec<StoredItemDefinition>,
    pub confirmation_digest: Sha256Digest,
}

pub struct CompositionConfirmationService;

impl CompositionConfirmationService {
    pub fn confirm<R>(
        pack: &LoadedGamePack,
        draft: &CompositionDraft,
        selected_item_ids: &[ItemId],
        repository: &R,
    ) -> Result<CompositionConfirmation, CompositionConfirmationError>
    where
        R: AtomicItemRepository + ?Sized,
    {
        draft
            .validate()
            .map_err(|_| CompositionConfirmationError::InvalidDraft)?;
        if &draft.game_pack_id != pack.id() || &draft.game_pack_sha256 != pack.content_sha256() {
            return Err(CompositionConfirmationError::ContextMismatch);
        }
        let selected = selected_item_ids.iter().collect::<BTreeSet<_>>();
        if selected.is_empty()
            || selected.len() != selected_item_ids.len()
            || selected
                .iter()
                .any(|item_id| !draft.nodes.contains_key(item_id))
        {
            return Err(CompositionConfirmationError::InvalidSelection);
        }
        for node in draft.nodes.values() {
            ItemDefinitionValidator::validate(
                pack,
                &node.definition,
                ItemDefinitionValidationMode::Draft,
            )
            .map_err(|_| CompositionConfirmationError::InvalidDraft)?;
        }

        let mut candidates = BTreeMap::new();
        for item_id in selected {
            let node = draft
                .nodes
                .get(item_id)
                .ok_or(CompositionConfirmationError::InvalidSelection)?;
            ItemDefinitionValidator::validate(
                pack,
                &node.definition,
                ItemDefinitionValidationMode::Ready,
            )
            .map_err(|_| CompositionConfirmationError::NotReady)?;
            let stored = StoredItemDefinition {
                definition_hash: node
                    .definition
                    .definition_hash()
                    .map_err(|_| CompositionConfirmationError::InvalidDraft)?,
                definition: node.definition.clone(),
            };
            candidates.insert(item_id.clone(), stored);
        }
        let seeds = candidates.values().cloned().collect::<Vec<_>>();
        let mut load = |item_id: &ItemId, hash: &Sha256Digest| {
            if let Some(candidate) = candidates.get(item_id) {
                if &candidate.definition_hash != hash {
                    return Err(CompositionGraphError::HashMismatch);
                }
                Ok(candidate.clone())
            } else {
                repository.load_version(item_id, hash).map_err(|error| {
                    match R::classify_error(&error) {
                        ItemRepositoryErrorKind::NotFound => CompositionGraphError::ItemMissing,
                        ItemRepositoryErrorKind::Storage => CompositionGraphError::Storage,
                    }
                })
            }
        };
        let mut readiness = |definition: &StoredItemDefinition| {
            ItemDefinitionValidator::validate(
                pack,
                &definition.definition,
                ItemDefinitionValidationMode::Ready,
            )
            .map_err(|_| CompositionGraphError::Readiness)
        };
        collect_graph(pack, seeds, &mut load, &mut readiness)
            .map_err(CompositionConfirmationError::Graph)?;

        let definitions = candidates
            .into_values()
            .map(|stored| stored.definition)
            .collect::<Vec<_>>();
        let expected_current = definitions
            .iter()
            .map(|definition| {
                let expected = draft
                    .nodes
                    .get(&definition.item_id)
                    .expect("selected definitions originate from Draft nodes")
                    .expected_current_definition_hash
                    .clone();
                (definition.item_id.clone(), expected)
            })
            .collect();
        let request = AtomicItemSaveRequest {
            definitions,
            expected_current,
        };
        let mut stored = repository
            .save_batch(&request)
            .map_err(|error| match error {
                AtomicItemSaveError::InvalidRequest => CompositionConfirmationError::InvalidDraft,
                AtomicItemSaveError::Conflict => CompositionConfirmationError::Conflict,
                AtomicItemSaveError::Repository(_) => CompositionConfirmationError::Storage,
            })?;
        stored.sort_by(|left, right| left.definition.item_id.cmp(&right.definition.item_id));
        let draft_ref = CompositionDraftRef {
            draft_id: draft.draft_id.clone(),
            revision: draft.revision,
        };
        let bytes = serde_json::to_vec(&(&draft_ref, &stored))
            .map_err(|_| CompositionConfirmationError::Digest)?;
        let confirmation_digest =
            digest(&bytes).map_err(|_| CompositionConfirmationError::Digest)?;
        Ok(CompositionConfirmation {
            draft: draft_ref,
            definitions: stored,
            confirmation_digest,
        })
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CompositionGraphError {
    #[error("composition graph context identities do not match")]
    ContextMismatch,
    #[error("composition graph provenance is invalid")]
    InvalidProvenance,
    #[error("composition graph ItemDefinition is missing")]
    ItemMissing,
    #[error("composition graph ItemDefinition storage failed")]
    Storage,
    #[error("composition graph ItemDefinition hash does not match")]
    HashMismatch,
    #[error("composition graph contains incompatible versions of one Item identity")]
    VersionConflict,
    #[error("composition graph reference target type does not match the Pack slot")]
    TypeMismatch,
    #[error("composition graph ItemDefinition is not ready")]
    Readiness,
    #[error("composition graph pinned references contain a cycle")]
    PinnedCycle,
    #[error("composition graph exceeds the maximum node count")]
    NodeLimit,
    #[error("composition graph digest cannot be represented")]
    Digest,
}

impl CompositionGraphError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ContextMismatch => "composition.graph.context_mismatch",
            Self::InvalidProvenance => "composition.graph.provenance_invalid",
            Self::ItemMissing => "composition.graph.item_missing",
            Self::Storage => "composition.graph.storage_failed",
            Self::HashMismatch => "composition.graph.hash_mismatch",
            Self::VersionConflict => "composition.graph.version_conflict",
            Self::TypeMismatch => "composition.graph.type_mismatch",
            Self::Readiness => "composition.graph.not_ready",
            Self::PinnedCycle => "composition.graph.cycle",
            Self::NodeLimit => "composition.graph.node_limit",
            Self::Digest => "composition.graph.digest_failed",
        }
    }
}

#[derive(Debug, Error)]
pub enum CompositionConfirmationError {
    #[error("composition Draft is invalid")]
    InvalidDraft,
    #[error("composition Draft belongs to a different Game Pack")]
    ContextMismatch,
    #[error("composition Draft selection is invalid")]
    InvalidSelection,
    #[error("composition Draft selection is not ready")]
    NotReady,
    #[error("composition Draft confirmation graph is invalid")]
    Graph(#[source] CompositionGraphError),
    #[error("composition Draft current pointers changed")]
    Conflict,
    #[error("composition Draft confirmation storage failed")]
    Storage,
    #[error("composition Draft confirmation digest failed")]
    Digest,
}

impl CompositionConfirmationError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidDraft => "composition.draft.invalid",
            Self::ContextMismatch => "composition.draft.context_mismatch",
            Self::InvalidSelection => "composition.confirm.selection_invalid",
            Self::NotReady => "composition.confirm.not_ready",
            Self::Graph(error) => error.code(),
            Self::Conflict => "composition.confirm.conflict",
            Self::Storage => "composition.confirm.storage_failed",
            Self::Digest => "composition.confirm.digest_failed",
        }
    }
}

struct CollectedGraph {
    nodes: Vec<StoredItemDefinition>,
    pinned_edges: Vec<ResolvedPinnedEdge>,
    identity_edges: Vec<ResolvedIdentityEdge>,
}

fn collect_graph<L, F>(
    pack: &LoadedGamePack,
    seeds: Vec<StoredItemDefinition>,
    load: &mut L,
    readiness: &mut F,
) -> Result<CollectedGraph, CompositionGraphError>
where
    L: FnMut(&ItemId, &Sha256Digest) -> Result<StoredItemDefinition, CompositionGraphError>,
    F: FnMut(&StoredItemDefinition) -> Result<(), CompositionGraphError>,
{
    let mut nodes = BTreeMap::new();
    let mut active = BTreeSet::new();
    let mut completed = BTreeSet::new();
    let mut pinned_edges = Vec::new();
    let mut identity_edges = Vec::new();
    for seed in seeds {
        visit_definition(
            pack,
            seed,
            load,
            readiness,
            &mut nodes,
            &mut active,
            &mut completed,
            &mut pinned_edges,
            &mut identity_edges,
        )?;
    }
    for edge in &identity_edges {
        let target = nodes
            .get(&edge.target_item_id)
            .ok_or(CompositionGraphError::ItemMissing)?;
        if target.definition.item_type != edge.expected_item_type {
            return Err(CompositionGraphError::TypeMismatch);
        }
    }
    pinned_edges.sort();
    identity_edges.sort();
    ensure_pinned_edges_acyclic(&pinned_edges)?;
    Ok(CollectedGraph {
        nodes: nodes.into_values().collect(),
        pinned_edges,
        identity_edges,
    })
}

fn ensure_pinned_edges_acyclic(edges: &[ResolvedPinnedEdge]) -> Result<(), CompositionGraphError> {
    fn visit(
        item_id: &ItemId,
        adjacency: &BTreeMap<ItemId, Vec<ItemId>>,
        active: &mut BTreeSet<ItemId>,
        completed: &mut BTreeSet<ItemId>,
    ) -> Result<(), CompositionGraphError> {
        if active.contains(item_id) {
            return Err(CompositionGraphError::PinnedCycle);
        }
        if completed.contains(item_id) {
            return Ok(());
        }
        active.insert(item_id.clone());
        if let Some(targets) = adjacency.get(item_id) {
            for target in targets {
                visit(target, adjacency, active, completed)?;
            }
        }
        active.remove(item_id);
        completed.insert(item_id.clone());
        Ok(())
    }

    let mut adjacency = BTreeMap::<ItemId, Vec<ItemId>>::new();
    for edge in edges {
        adjacency
            .entry(edge.source_item_id.clone())
            .or_default()
            .push(edge.target_item_id.clone());
    }
    let mut active = BTreeSet::new();
    let mut completed = BTreeSet::new();
    for item_id in adjacency.keys() {
        visit(item_id, &adjacency, &mut active, &mut completed)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn visit_definition<L, F>(
    pack: &LoadedGamePack,
    definition: StoredItemDefinition,
    load: &mut L,
    readiness: &mut F,
    nodes: &mut BTreeMap<ItemId, StoredItemDefinition>,
    active: &mut BTreeSet<ItemId>,
    completed: &mut BTreeSet<ItemId>,
    pinned_edges: &mut Vec<ResolvedPinnedEdge>,
    identity_edges: &mut Vec<ResolvedIdentityEdge>,
) -> Result<(), CompositionGraphError>
where
    L: FnMut(&ItemId, &Sha256Digest) -> Result<StoredItemDefinition, CompositionGraphError>,
    F: FnMut(&StoredItemDefinition) -> Result<(), CompositionGraphError>,
{
    definition
        .validate()
        .map_err(|_| CompositionGraphError::HashMismatch)?;
    let item_id = definition.definition.item_id.clone();
    if active.contains(&item_id) {
        return Err(CompositionGraphError::PinnedCycle);
    }
    if let Some(existing) = nodes.get(&item_id) {
        if existing.definition_hash != definition.definition_hash {
            return Err(CompositionGraphError::VersionConflict);
        }
        if completed.contains(&item_id) {
            return Ok(());
        }
    }
    if nodes.len() >= 128 && !nodes.contains_key(&item_id) {
        return Err(CompositionGraphError::NodeLimit);
    }
    ItemDefinitionValidator::validate(
        pack,
        &definition.definition,
        ItemDefinitionValidationMode::Ready,
    )
    .map_err(|_| CompositionGraphError::Readiness)?;
    readiness(&definition)?;
    let descriptor = pack
        .item_type(&definition.definition.item_type)
        .ok_or(CompositionGraphError::TypeMismatch)?;
    nodes.insert(item_id.clone(), definition.clone());
    active.insert(item_id.clone());

    for (slot_id, bindings) in &definition.definition.reference_bindings {
        let slot = descriptor
            .reference_slots()
            .iter()
            .find(|candidate| candidate.id() == slot_id)
            .ok_or(CompositionGraphError::TypeMismatch)?;
        for binding in bindings {
            match (slot.kind(), binding) {
                (
                    ItemReferenceKind::Pinned,
                    ItemReferenceBinding::Pinned {
                        item_id: target_item_id,
                        definition_hash: target_hash,
                        quantity,
                    },
                ) => {
                    let target = load(target_item_id, target_hash)?;
                    if target.definition.item_id != *target_item_id
                        || target.definition_hash != *target_hash
                    {
                        return Err(CompositionGraphError::HashMismatch);
                    }
                    if !slot
                        .allowed_item_types()
                        .contains(&target.definition.item_type)
                    {
                        return Err(CompositionGraphError::TypeMismatch);
                    }
                    pinned_edges.push(ResolvedPinnedEdge {
                        source_item_id: item_id.clone(),
                        source_definition_hash: definition.definition_hash.clone(),
                        slot_id: slot_id.clone(),
                        target_item_id: target_item_id.clone(),
                        target_definition_hash: target_hash.clone(),
                        quantity: *quantity,
                    });
                    visit_definition(
                        pack,
                        target,
                        load,
                        readiness,
                        nodes,
                        active,
                        completed,
                        pinned_edges,
                        identity_edges,
                    )?;
                }
                (
                    ItemReferenceKind::Identity,
                    ItemReferenceBinding::Identity {
                        item_id: target_item_id,
                        expected_item_type,
                    },
                ) => identity_edges.push(ResolvedIdentityEdge {
                    source_item_id: item_id.clone(),
                    source_definition_hash: definition.definition_hash.clone(),
                    slot_id: slot_id.clone(),
                    target_item_id: target_item_id.clone(),
                    expected_item_type: expected_item_type.clone(),
                }),
                _ => return Err(CompositionGraphError::TypeMismatch),
            }
        }
    }
    active.remove(&item_id);
    completed.insert(item_id);
    Ok(())
}

fn build_resolved_graph(
    pack: &LoadedGamePack,
    truth: &VerifiedTruthSnapshot,
    root: StoredItemDefinition,
    draft: Option<CompositionDraftRef>,
    collected: CollectedGraph,
) -> Result<ResolvedItemGraph, CompositionGraphError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DigestMaterial<'a> {
        schema_version: u32,
        game_pack_id: &'a GamePackId,
        game_pack_sha256: &'a Sha256Digest,
        truth_snapshot_id: &'a Sha256Digest,
        draft: &'a Option<CompositionDraftRef>,
        root_item_id: &'a ItemId,
        root_definition_hash: &'a Sha256Digest,
        composition_profile: &'a Option<ItemCompositionProfile>,
        nodes: &'a [StoredItemDefinition],
        pinned_edges: &'a [ResolvedPinnedEdge],
        identity_edges: &'a [ResolvedIdentityEdge],
    }

    let truth_snapshot_id = truth.manifest().snapshot_id().clone();
    let composition_profile = root.definition.composition_profile.clone();
    let bytes = serde_json::to_vec(&DigestMaterial {
        schema_version: RESOLVED_ITEM_GRAPH_SCHEMA_VERSION,
        game_pack_id: pack.id(),
        game_pack_sha256: pack.content_sha256(),
        truth_snapshot_id: &truth_snapshot_id,
        draft: &draft,
        root_item_id: &root.definition.item_id,
        root_definition_hash: &root.definition_hash,
        composition_profile: &composition_profile,
        nodes: &collected.nodes,
        pinned_edges: &collected.pinned_edges,
        identity_edges: &collected.identity_edges,
    })
    .map_err(|_| CompositionGraphError::Digest)?;
    Ok(ResolvedItemGraph {
        schema_version: RESOLVED_ITEM_GRAPH_SCHEMA_VERSION,
        game_pack_id: pack.id().clone(),
        game_pack_sha256: pack.content_sha256().clone(),
        truth_snapshot_id,
        draft,
        root_item_id: root.definition.item_id,
        root_definition_hash: root.definition_hash,
        composition_profile,
        nodes: collected.nodes,
        pinned_edges: collected.pinned_edges,
        identity_edges: collected.identity_edges,
        graph_digest: digest(&bytes).map_err(|_| CompositionGraphError::Digest)?,
    })
}

fn digest(bytes: &[u8]) -> Result<Sha256Digest, ats_kernel::ContractValueError> {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use ats_game_context::{
        ContributionResolver, GamePackLoader, TruthEvidenceRecord, TruthSnapshotIndex,
        TruthSnapshotManifest, TruthSnapshotSource,
    };
    use ats_kernel::{
        CompositionId, CompositionParameterId, CompositionProfileId, ItemTypeId, ResourceId,
    };
    use ats_workspace::{
        CompositionDraftNode, ItemCompositionSource, ItemDefinition, ResourceAsset,
        ResourceBytesIngestRequest, ResourceDeriveRequest,
    };
    use chrono::Utc;

    use super::*;
    use crate::{FeatureSpec, resource_prepare::ResourcePrepareFeature};

    #[derive(Debug, Error)]
    #[error("memory repository failure")]
    struct MemoryError;

    #[derive(Default)]
    struct MemoryItems {
        versions: Mutex<BTreeMap<(ItemId, Sha256Digest), StoredItemDefinition>>,
        current: Mutex<BTreeMap<ItemId, Sha256Digest>>,
    }

    impl MemoryItems {
        fn insert(&self, stored: StoredItemDefinition) {
            self.versions.lock().unwrap().insert(
                (
                    stored.definition.item_id.clone(),
                    stored.definition_hash.clone(),
                ),
                stored.clone(),
            );
            self.current
                .lock()
                .unwrap()
                .insert(stored.definition.item_id.clone(), stored.definition_hash);
        }
    }

    impl ItemRepository for MemoryItems {
        type Error = MemoryError;

        fn classify_error(_: &Self::Error) -> ItemRepositoryErrorKind {
            ItemRepositoryErrorKind::NotFound
        }

        fn save(&self, definition: &ItemDefinition) -> Result<StoredItemDefinition, Self::Error> {
            let stored = stored(definition.clone());
            self.insert(stored.clone());
            Ok(stored)
        }

        fn load_current(&self, item_id: &ItemId) -> Result<StoredItemDefinition, Self::Error> {
            let hash = self
                .current
                .lock()
                .unwrap()
                .get(item_id)
                .cloned()
                .ok_or(MemoryError)?;
            self.load_version(item_id, &hash)
        }

        fn load_version(
            &self,
            item_id: &ItemId,
            definition_hash: &Sha256Digest,
        ) -> Result<StoredItemDefinition, Self::Error> {
            self.versions
                .lock()
                .unwrap()
                .get(&(item_id.clone(), definition_hash.clone()))
                .cloned()
                .ok_or(MemoryError)
        }

        fn list_current(&self) -> Result<Vec<StoredItemDefinition>, Self::Error> {
            let current = self.current.lock().unwrap().clone();
            current
                .iter()
                .map(|(item_id, hash)| self.load_version(item_id, hash))
                .collect()
        }
    }

    impl AtomicItemRepository for MemoryItems {
        fn save_batch(
            &self,
            request: &AtomicItemSaveRequest,
        ) -> Result<Vec<StoredItemDefinition>, AtomicItemSaveError<Self::Error>> {
            request
                .validate()
                .map_err(|_| AtomicItemSaveError::InvalidRequest)?;
            let mut current = self.current.lock().unwrap();
            if request
                .expected_current
                .iter()
                .any(|(item_id, expected)| current.get(item_id) != expected.as_ref())
            {
                return Err(AtomicItemSaveError::Conflict);
            }
            let definitions = request
                .definitions
                .iter()
                .cloned()
                .map(stored)
                .collect::<Vec<_>>();
            let mut versions = self.versions.lock().unwrap();
            for definition in &definitions {
                versions.insert(
                    (
                        definition.definition.item_id.clone(),
                        definition.definition_hash.clone(),
                    ),
                    definition.clone(),
                );
                current.insert(
                    definition.definition.item_id.clone(),
                    definition.definition_hash.clone(),
                );
            }
            Ok(definitions)
        }
    }

    struct EmptyResources;

    struct FailingItems;

    impl ItemRepository for FailingItems {
        type Error = MemoryError;

        fn save(&self, _: &ItemDefinition) -> Result<StoredItemDefinition, Self::Error> {
            Err(MemoryError)
        }

        fn load_current(&self, _: &ItemId) -> Result<StoredItemDefinition, Self::Error> {
            Err(MemoryError)
        }

        fn load_version(
            &self,
            _: &ItemId,
            _: &Sha256Digest,
        ) -> Result<StoredItemDefinition, Self::Error> {
            Err(MemoryError)
        }

        fn list_current(&self) -> Result<Vec<StoredItemDefinition>, Self::Error> {
            Err(MemoryError)
        }
    }

    impl ResourceRepository for EmptyResources {
        type Error = MemoryError;

        fn ingest_bytes(
            &self,
            _: ResourceBytesIngestRequest,
        ) -> Result<ResourceAsset, Self::Error> {
            Err(MemoryError)
        }

        fn ingest_batch(
            &self,
            _: Vec<ResourceBytesIngestRequest>,
        ) -> Result<Vec<ResourceAsset>, Self::Error> {
            Err(MemoryError)
        }

        fn add_version(&self, _: ResourceDeriveRequest) -> Result<ResourceAsset, Self::Error> {
            Err(MemoryError)
        }

        fn select(&self, _: &ResourceId, _: &Sha256Digest) -> Result<ResourceAsset, Self::Error> {
            Err(MemoryError)
        }

        fn load(&self, _: &ResourceId) -> Result<ResourceAsset, Self::Error> {
            Err(MemoryError)
        }

        fn read_selected_bytes(
            &self,
            _: &ResourceId,
            _: &Sha256Digest,
        ) -> Result<Vec<u8>, Self::Error> {
            Err(MemoryError)
        }

        fn read_version_bytes(
            &self,
            _: &ResourceId,
            _: &Sha256Digest,
        ) -> Result<Vec<u8>, Self::Error> {
            Err(MemoryError)
        }

        fn list(&self) -> Result<Vec<ResourceAsset>, Self::Error> {
            Ok(Vec::new())
        }
    }

    fn pack() -> LoadedGamePack {
        let value = serde_json::json!({
            "schemaVersion":4,
            "id":"fixture-game",
            "displayName":"Fixture",
            "itemTypes":[
                {
                    "id":"character",
                    "displayNames":{"eng":"Character"},
                    "referenceSlots":[{
                        "id":"starting_deck","displayNames":{"eng":"Starting deck"},
                        "kind":"pinned","allowedItemTypes":["card"],
                        "minItems":1,"maxItems":8,"minQuantity":1,"maxQuantity":10
                    }],
                    "evidenceQueries":[{"symbols":["Character.Symbol"],"terms":[]}]
                },
                {
                    "id":"card",
                    "displayNames":{"eng":"Card"},
                    "referenceSlots":[{
                        "id":"owner","displayNames":{"eng":"Owner"},
                        "kind":"identity","allowedItemTypes":["character"],
                        "minItems":1,"maxItems":1,"minQuantity":1,"maxQuantity":1
                    }],
                    "evidenceQueries":[{"symbols":["Card.Symbol"],"terms":[]}]
                },
                {
                    "id":"relic",
                    "displayNames":{"eng":"Relic"},
                    "evidenceQueries":[{"symbols":["Relic.Symbol"],"terms":[]}]
                }
            ],
            "compositionProfiles":[{
                "id":"character_suite","displayNames":{"eng":"Character suite"},
                "rootItemType":"character","defaultProfile":"standard",
                "customBaseProfile":"standard","maxNodes":128,"baseNodeCount":1,
                "parameters":[{
                    "id":"card_count","displayNames":{"eng":"Card count"},
                    "min":1,"max":8,"nodeWeight":1
                }],
                "profiles":[{
                    "id":"standard","displayNames":{"eng":"Standard"},
                    "values":{"card_count":1}
                }],
                "constraints":[]
            }],
            "contributions":[{
                "slotId":"resource.prepare.specs",
                "featureId":"resource.prepare",
                "schema":{"id":"pack.resource-specs","version":2},
                "requiredPrimitives":[],
                "payload":{"roles":[{
                    "id":"fixture.icon","mediaTypes":["image/png"],
                    "width":1,"height":1,"requireAlpha":false,
                    "source":{"kind":"master"}
                }]}
            }]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        GamePackLoader::load(&bytes, &digest(&bytes).unwrap()).unwrap()
    }

    fn truth(pack: &LoadedGamePack) -> VerifiedTruthSnapshot {
        let records = ["Character.Symbol", "Card.Symbol", "Relic.Symbol"]
            .into_iter()
            .map(|symbol| TruthEvidenceRecord {
                source_id: "fixture".into(),
                symbol: symbol.into(),
                purpose: "fixture readiness".into(),
                bounded_excerpt: format!("{symbol} is available."),
                relative_path: format!("indexes/{symbol}.cs"),
            })
            .collect::<Vec<_>>();
        let index_bytes = serde_json::to_vec(&records).unwrap();
        let manifest = TruthSnapshotManifest::new(
            pack,
            vec![TruthSnapshotSource {
                id: "fixture".into(),
                kind: "local_file".into(),
                version: Some("1".into()),
                relative_path: "sources/fixture.bin".into(),
                sha256: digest(b"fixture").unwrap(),
                byte_length: 7,
            }],
            vec![TruthSnapshotIndex {
                id: "symbols".into(),
                provider: ats_kernel::PrimitiveId::parse("truth.fixture-indexer").unwrap(),
                relative_path: "indexes/symbols.json".into(),
                sha256: digest(&index_bytes).unwrap(),
                record_count: records.len() as u32,
            }],
            BTreeMap::from([("indexer".into(), "1".into())]),
            Utc::now(),
        )
        .unwrap();
        VerifiedTruthSnapshot::verify(
            pack,
            manifest,
            BTreeMap::from([("symbols".into(), records)]),
        )
        .unwrap()
    }

    fn profile() -> ItemCompositionProfile {
        ItemCompositionProfile {
            composition_id: CompositionId::parse("character_suite").unwrap(),
            source: ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            },
            parameters: BTreeMap::from([(CompositionParameterId::parse("card_count").unwrap(), 1)]),
        }
    }

    fn card(character_id: &ItemId) -> ItemDefinition {
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture-card").unwrap(),
            ItemTypeId::parse("card").unwrap(),
        );
        definition.behavior_intent = vec!["Deal a bounded amount of damage.".into()];
        definition.reference_bindings.insert(
            ItemReferenceSlotId::parse("owner").unwrap(),
            vec![ItemReferenceBinding::Identity {
                item_id: character_id.clone(),
                expected_item_type: ItemTypeId::parse("character").unwrap(),
            }],
        );
        definition
    }

    fn character(card: &StoredItemDefinition) -> ItemDefinition {
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture-character").unwrap(),
            ItemTypeId::parse("character").unwrap(),
        );
        definition.behavior_intent = vec!["Provide a playable character.".into()];
        definition.reference_bindings.insert(
            ItemReferenceSlotId::parse("starting_deck").unwrap(),
            vec![ItemReferenceBinding::Pinned {
                item_id: card.definition.item_id.clone(),
                definition_hash: card.definition_hash.clone(),
                quantity: 4,
            }],
        );
        definition.composition_profile = Some(profile());
        definition
    }

    fn stored(definition: ItemDefinition) -> StoredItemDefinition {
        StoredItemDefinition {
            definition_hash: definition.definition_hash().unwrap(),
            definition,
        }
    }

    fn contributions(pack: &LoadedGamePack) -> VerifiedContributionSet {
        ContributionResolver::default()
            .resolve(
                pack,
                &ResourcePrepareFeature::id(),
                &[ResourcePrepareFeature::contribution_requirement()],
            )
            .unwrap()
    }

    fn draft(root: ItemDefinition, card: ItemDefinition) -> CompositionDraft {
        let root_id = root.item_id.clone();
        CompositionDraft::new(
            CompositionDraftId::parse("fixture-draft").unwrap(),
            GamePackId::parse("fixture-game").unwrap(),
            pack().content_sha256().clone(),
            root_id.clone(),
            profile(),
            BTreeMap::from([
                (
                    root_id,
                    CompositionDraftNode {
                        definition: root,
                        expected_current_definition_hash: None,
                    },
                ),
                (
                    card.item_id.clone(),
                    CompositionDraftNode {
                        definition: card,
                        expected_current_definition_hash: None,
                    },
                ),
            ]),
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn resolves_pinned_closure_identity_affiliation_and_stable_digest() {
        let pack = pack();
        let truth = truth(&pack);
        let character_id = ItemId::parse("fixture-character").unwrap();
        let card = stored(card(&character_id));
        let root = stored(character(&card));
        let items = MemoryItems::default();
        items.insert(card);
        let draft_ref = Some(CompositionDraftRef {
            draft_id: CompositionDraftId::parse("fixture-draft").unwrap(),
            revision: 1,
        });
        let first = ResolvedItemGraph::resolve(
            &pack,
            &truth,
            &contributions(&pack),
            &items,
            &EmptyResources,
            root.clone(),
            draft_ref.clone(),
        )
        .unwrap();
        let second = ResolvedItemGraph::resolve(
            &pack,
            &truth,
            &contributions(&pack),
            &items,
            &EmptyResources,
            root,
            draft_ref,
        )
        .unwrap();
        assert_eq!(first.graph_digest, second.graph_digest);
        assert_eq!(first.nodes.len(), 2);
        assert_eq!(first.pinned_edges.len(), 1);
        assert_eq!(first.identity_edges.len(), 1);
        assert_eq!(first.identity_edges[0].target_item_id, character_id);
    }

    #[test]
    fn confirmation_requires_a_closed_selection_and_saves_all_currents() {
        let pack = pack();
        let character_id = ItemId::parse("fixture-character").unwrap();
        let card_definition = card(&character_id);
        let card_stored = stored(card_definition.clone());
        let root = character(&card_stored);
        let draft = draft(root, card_definition);
        let repository = MemoryItems::default();
        assert!(matches!(
            CompositionConfirmationService::confirm(
                &pack,
                &draft,
                std::slice::from_ref(&draft.root_item_id),
                &repository,
            ),
            Err(CompositionConfirmationError::Graph(
                CompositionGraphError::ItemMissing
            ))
        ));
        assert!(repository.list_current().unwrap().is_empty());

        let selected = draft.nodes.keys().cloned().collect::<Vec<_>>();
        let confirmation =
            CompositionConfirmationService::confirm(&pack, &draft, &selected, &repository).unwrap();
        assert_eq!(confirmation.definitions.len(), 2);
        assert_eq!(repository.list_current().unwrap().len(), 2);
        assert_eq!(confirmation.draft.revision, 1);
    }

    #[test]
    fn graph_reports_missing_type_readiness_and_cycles_with_stable_codes() {
        assert_eq!(
            CompositionGraphError::ItemMissing.code(),
            "composition.graph.item_missing"
        );
        assert_eq!(
            CompositionGraphError::TypeMismatch.code(),
            "composition.graph.type_mismatch"
        );
        assert_eq!(
            CompositionGraphError::Readiness.code(),
            "composition.graph.not_ready"
        );
        let hash = digest(b"edge").unwrap();
        let edge = |source: &str, target: &str| ResolvedPinnedEdge {
            source_item_id: ItemId::parse(source).unwrap(),
            source_definition_hash: hash.clone(),
            slot_id: ItemReferenceSlotId::parse("dependency").unwrap(),
            target_item_id: ItemId::parse(target).unwrap(),
            target_definition_hash: hash.clone(),
            quantity: 1,
        };
        assert_eq!(
            ensure_pinned_edges_acyclic(&[edge("node-a", "node-b"), edge("node-b", "node-a")]),
            Err(CompositionGraphError::PinnedCycle)
        );
    }

    #[test]
    fn resolver_rejects_missing_wrong_type_tampered_hash_and_not_ready_nodes() {
        let pack = pack();
        let truth = truth(&pack);
        let resources = EmptyResources;
        let resource_contributions = contributions(&pack);
        let character_id = ItemId::parse("fixture-character").unwrap();
        let ready_card = stored(card(&character_id));
        let root = stored(character(&ready_card));

        let missing = ResolvedItemGraph::resolve(
            &pack,
            &truth,
            &resource_contributions,
            &MemoryItems::default(),
            &resources,
            root.clone(),
            None,
        );
        assert_eq!(missing, Err(CompositionGraphError::ItemMissing));
        assert_eq!(
            ResolvedItemGraph::resolve(
                &pack,
                &truth,
                &resource_contributions,
                &FailingItems,
                &resources,
                root.clone(),
                None,
            ),
            Err(CompositionGraphError::Storage)
        );

        let mut relic_definition = ItemDefinition::new(
            ready_card.definition.item_id.clone(),
            ItemTypeId::parse("relic").unwrap(),
        );
        relic_definition.behavior_intent = vec!["Provide one observable effect.".into()];
        let relic = stored(relic_definition);
        let wrong_type_root = stored(character(&relic));
        let wrong_type_items = MemoryItems::default();
        wrong_type_items.insert(relic);
        assert_eq!(
            ResolvedItemGraph::resolve(
                &pack,
                &truth,
                &resource_contributions,
                &wrong_type_items,
                &resources,
                wrong_type_root,
                None,
            ),
            Err(CompositionGraphError::TypeMismatch)
        );

        let tampered_items = MemoryItems::default();
        let mut tampered = ready_card.clone();
        tampered.definition_hash = digest(b"tampered").unwrap();
        tampered_items.versions.lock().unwrap().insert(
            (
                ready_card.definition.item_id.clone(),
                ready_card.definition_hash.clone(),
            ),
            tampered,
        );
        assert_eq!(
            ResolvedItemGraph::resolve(
                &pack,
                &truth,
                &resource_contributions,
                &tampered_items,
                &resources,
                root.clone(),
                None,
            ),
            Err(CompositionGraphError::HashMismatch)
        );

        let mut incomplete_definition = card(&character_id);
        incomplete_definition.behavior_intent.clear();
        let incomplete = stored(incomplete_definition);
        let not_ready_root = stored(character(&incomplete));
        let not_ready_items = MemoryItems::default();
        not_ready_items.insert(incomplete);
        assert_eq!(
            ResolvedItemGraph::resolve(
                &pack,
                &truth,
                &resource_contributions,
                &not_ready_items,
                &resources,
                not_ready_root,
                None,
            ),
            Err(CompositionGraphError::Readiness)
        );
    }

    #[test]
    fn confirmation_surfaces_stale_current_as_typed_conflict() {
        let pack = pack();
        let character_id = ItemId::parse("fixture-character").unwrap();
        let card_definition = card(&character_id);
        let card_stored = stored(card_definition.clone());
        let mut draft = draft(character(&card_stored), card_definition);
        let repository = MemoryItems::default();
        let changed = stored({
            let mut value = draft
                .nodes
                .get(&draft.root_item_id)
                .unwrap()
                .definition
                .clone();
            value
                .behavior_intent
                .push("Changed outside the Draft.".into());
            value
        });
        repository.insert(changed);
        draft
            .nodes
            .get_mut(&draft.root_item_id)
            .unwrap()
            .expected_current_definition_hash = None;
        let selected = draft.nodes.keys().cloned().collect::<Vec<_>>();
        let error = CompositionConfirmationService::confirm(&pack, &draft, &selected, &repository)
            .unwrap_err();
        assert!(matches!(error, CompositionConfirmationError::Conflict));
        assert_eq!(error.code(), "composition.confirm.conflict");
    }
}
