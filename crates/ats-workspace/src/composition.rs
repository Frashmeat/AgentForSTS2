use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{CompositionDraftId, ExecutionGraphId, GamePackId, ItemId, Sha256Digest};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{ItemCompositionProfile, ItemDefinition, ItemReferenceBinding};

pub const COMPOSITION_DRAFT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionDraftNode {
    pub definition: ItemDefinition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_current_definition_hash: Option<Sha256Digest>,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionDraft {
    schema_version: u32,
    pub draft_id: CompositionDraftId,
    pub revision: u64,
    pub game_pack_id: GamePackId,
    pub game_pack_sha256: Sha256Digest,
    pub root_item_id: ItemId,
    pub profile: ItemCompositionProfile,
    pub nodes: BTreeMap<ItemId, CompositionDraftNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_execution_graph_id: Option<ExecutionGraphId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validated_content_digest: Option<Sha256Digest>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CompositionDraft {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        draft_id: CompositionDraftId,
        game_pack_id: GamePackId,
        game_pack_sha256: Sha256Digest,
        root_item_id: ItemId,
        profile: ItemCompositionProfile,
        mut nodes: BTreeMap<ItemId, CompositionDraftNode>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, CompositionDraftError> {
        rebind_internal_pinned_hashes(&mut nodes)?;
        let draft = Self {
            schema_version: COMPOSITION_DRAFT_SCHEMA_VERSION,
            draft_id,
            revision: 1,
            game_pack_id,
            game_pack_sha256,
            root_item_id,
            profile,
            nodes,
            source_execution_graph_id: None,
            validated_content_digest: None,
            created_at,
            updated_at: created_at,
        };
        draft.validate()?;
        Ok(draft)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_staged(
        draft_id: CompositionDraftId,
        game_pack_id: GamePackId,
        game_pack_sha256: Sha256Digest,
        root_item_id: ItemId,
        profile: ItemCompositionProfile,
        nodes: BTreeMap<ItemId, CompositionDraftNode>,
        source_execution_graph_id: ExecutionGraphId,
        validated_content_digest: Sha256Digest,
        created_at: DateTime<Utc>,
    ) -> Result<Self, CompositionDraftError> {
        let mut draft = Self::new(
            draft_id,
            game_pack_id,
            game_pack_sha256,
            root_item_id,
            profile,
            nodes,
            created_at,
        )?;
        draft.source_execution_graph_id = Some(source_execution_graph_id);
        draft.validated_content_digest = Some(validated_content_digest);
        draft.validate()?;
        Ok(draft)
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn validate(&self) -> Result<(), CompositionDraftError> {
        if self.schema_version != COMPOSITION_DRAFT_SCHEMA_VERSION {
            return Err(CompositionDraftError::UnsupportedSchema);
        }
        if self.revision == 0
            || self.nodes.is_empty()
            || self.nodes.len() > 128
            || self.updated_at < self.created_at
        {
            return Err(CompositionDraftError::InvalidMetadata);
        }
        if self.source_execution_graph_id.is_some() != self.validated_content_digest.is_some() {
            return Err(CompositionDraftError::InvalidMetadata);
        }
        let root = self
            .nodes
            .get(&self.root_item_id)
            .ok_or(CompositionDraftError::MissingRoot)?;
        if root.definition.composition_profile.as_ref() != Some(&self.profile) {
            return Err(CompositionDraftError::InvalidRootProfile);
        }
        if self.nodes.iter().any(|(item_id, node)| {
            item_id != &node.definition.item_id || node.definition.validate().is_err()
        }) {
            return Err(CompositionDraftError::InvalidNode);
        }
        Ok(())
    }

    pub fn payload_sha256(&self) -> Result<Sha256Digest, CompositionDraftError> {
        let value =
            serde_json::to_value(self).map_err(|_| CompositionDraftError::InvalidMetadata)?;
        let bytes =
            serde_json::to_vec(&value).map_err(|_| CompositionDraftError::InvalidMetadata)?;
        Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
            .map_err(|_| CompositionDraftError::InvalidMetadata)
    }

    pub fn revised(
        &self,
        mut nodes: BTreeMap<ItemId, CompositionDraftNode>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, CompositionDraftError> {
        rebind_internal_pinned_hashes(&mut nodes)?;
        let mut next = self.clone();
        next.revision = self
            .revision
            .checked_add(1)
            .ok_or(CompositionDraftError::InvalidMetadata)?;
        next.nodes = nodes;
        next.updated_at = updated_at;
        next.validate()?;
        Ok(next)
    }
}

fn rebind_internal_pinned_hashes(
    nodes: &mut BTreeMap<ItemId, CompositionDraftNode>,
) -> Result<(), CompositionDraftError> {
    fn resolve(
        item_id: &ItemId,
        nodes: &mut BTreeMap<ItemId, CompositionDraftNode>,
        active: &mut BTreeSet<ItemId>,
        resolved: &mut BTreeMap<ItemId, Sha256Digest>,
    ) -> Result<Sha256Digest, CompositionDraftError> {
        if let Some(hash) = resolved.get(item_id) {
            return Ok(hash.clone());
        }
        if !active.insert(item_id.clone()) {
            return Err(CompositionDraftError::PinnedCycle);
        }
        let internal_targets = nodes
            .get(item_id)
            .ok_or(CompositionDraftError::InvalidNode)?
            .definition
            .reference_bindings
            .values()
            .flatten()
            .filter_map(|binding| match binding {
                ItemReferenceBinding::Pinned { item_id, .. } if nodes.contains_key(item_id) => {
                    Some(item_id.clone())
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        for target in internal_targets {
            resolve(&target, nodes, active, resolved)?;
        }
        let node = nodes
            .get_mut(item_id)
            .ok_or(CompositionDraftError::InvalidNode)?;
        for binding in node.definition.reference_bindings.values_mut().flatten() {
            if let ItemReferenceBinding::Pinned {
                item_id,
                definition_hash,
                ..
            } = binding
                && let Some(target_hash) = resolved.get(item_id)
            {
                *definition_hash = target_hash.clone();
            }
        }
        let hash = node
            .definition
            .definition_hash()
            .map_err(|_| CompositionDraftError::InvalidNode)?;
        active.remove(item_id);
        resolved.insert(item_id.clone(), hash.clone());
        Ok(hash)
    }

    let item_ids = nodes.keys().cloned().collect::<Vec<_>>();
    let mut active = BTreeSet::new();
    let mut resolved = BTreeMap::new();
    for item_id in item_ids {
        resolve(&item_id, nodes, &mut active, &mut resolved)?;
    }
    Ok(())
}

impl<'de> Deserialize<'de> for CompositionDraft {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            draft_id: CompositionDraftId,
            revision: u64,
            game_pack_id: GamePackId,
            game_pack_sha256: Sha256Digest,
            root_item_id: ItemId,
            profile: ItemCompositionProfile,
            nodes: BTreeMap<ItemId, CompositionDraftNode>,
            #[serde(default)]
            source_execution_graph_id: Option<ExecutionGraphId>,
            #[serde(default)]
            validated_content_digest: Option<Sha256Digest>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let draft = Self {
            schema_version: wire.schema_version,
            draft_id: wire.draft_id,
            revision: wire.revision,
            game_pack_id: wire.game_pack_id,
            game_pack_sha256: wire.game_pack_sha256,
            root_item_id: wire.root_item_id,
            profile: wire.profile,
            nodes: wire.nodes,
            source_execution_graph_id: wire.source_execution_graph_id,
            validated_content_digest: wire.validated_content_digest,
            created_at: wire.created_at,
            updated_at: wire.updated_at,
        };
        draft.validate().map_err(serde::de::Error::custom)?;
        Ok(draft)
    }
}

pub trait CompositionDraftRepository: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn classify_error(_error: &Self::Error) -> CompositionDraftRepositoryErrorKind {
        CompositionDraftRepositoryErrorKind::Storage
    }

    fn create(&self, draft: &CompositionDraft) -> Result<(), Self::Error>;
    fn create_or_match(
        &self,
        draft: &CompositionDraft,
        expected_payload_sha256: &Sha256Digest,
    ) -> Result<CompositionDraftCreateOrMatch, Self::Error>;
    fn load(&self, draft_id: &CompositionDraftId) -> Result<CompositionDraft, Self::Error>;
    fn compare_and_set(
        &self,
        expected_revision: u64,
        next: &CompositionDraft,
    ) -> Result<(), Self::Error>;
    fn list(&self) -> Result<Vec<CompositionDraft>, Self::Error>;
    fn delete(
        &self,
        draft_id: &CompositionDraftId,
        expected_revision: u64,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CompositionDraftCreateOrMatch {
    Created,
    Matched,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CompositionDraftRepositoryErrorKind {
    NotFound,
    Conflict,
    Storage,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CompositionDraftError {
    #[error("composition draft schema version is unsupported")]
    UnsupportedSchema,
    #[error("composition draft metadata is invalid")]
    InvalidMetadata,
    #[error("composition draft root is missing")]
    MissingRoot,
    #[error("composition draft root profile is invalid")]
    InvalidRootProfile,
    #[error("composition draft node is invalid")]
    InvalidNode,
    #[error("composition draft pinned references contain a cycle")]
    PinnedCycle,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ats_kernel::{
        CompositionId, CompositionParameterId, CompositionProfileId, ItemReferenceSlotId,
        ItemTypeId, ResourceId,
    };

    use super::*;
    use crate::{ItemCompositionSource, ItemDefinition, ItemReferenceBinding, ItemResourceBinding};

    fn draft() -> CompositionDraft {
        let root_id = ItemId::parse("fixture-root").unwrap();
        let profile = ItemCompositionProfile {
            composition_id: CompositionId::parse("fixture-suite").unwrap(),
            source: ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            },
            parameters: BTreeMap::from([(CompositionParameterId::parse("node_count").unwrap(), 1)]),
        };
        let mut definition =
            ItemDefinition::new(root_id.clone(), ItemTypeId::parse("fixture").unwrap());
        definition.composition_profile = Some(profile.clone());
        CompositionDraft::new(
            CompositionDraftId::parse("fixture-draft").unwrap(),
            GamePackId::parse("fixture-game").unwrap(),
            Sha256Digest::parse("a".repeat(64)).unwrap(),
            root_id.clone(),
            profile,
            BTreeMap::from([(
                root_id,
                CompositionDraftNode {
                    definition,
                    expected_current_definition_hash: None,
                },
            )]),
            Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn draft_round_trip_preserves_profile_nodes_and_revision() {
        let draft = draft();
        let wire = serde_json::to_value(&draft).unwrap();
        assert_eq!(wire["schemaVersion"], 2);
        assert_eq!(wire["revision"], 1);
        assert_eq!(wire["rootItemId"], "fixture-root");
        assert!(wire.get("root_item_id").is_none());
        let decoded: CompositionDraft = serde_json::from_value(wire).unwrap();
        assert_eq!(decoded, draft);
    }

    #[test]
    fn draft_rejects_a_root_without_matching_profile_provenance() {
        let mut draft = draft();
        draft
            .nodes
            .get_mut(&draft.root_item_id)
            .unwrap()
            .definition
            .composition_profile = None;
        assert_eq!(
            draft.validate(),
            Err(CompositionDraftError::InvalidRootProfile)
        );
    }

    #[test]
    fn revision_preserves_identity_and_advances_cas_version() {
        let draft = draft();
        let updated_at = draft.updated_at + chrono::Duration::seconds(1);
        let revised = draft.revised(draft.nodes.clone(), updated_at).unwrap();
        assert_eq!(revised.revision, 2);
        assert_eq!(revised.draft_id, draft.draft_id);
        assert_eq!(revised.created_at, draft.created_at);
        assert_eq!(revised.updated_at, updated_at);
    }

    #[test]
    fn revision_rebinds_changed_leaf_hashes_through_the_pinned_dag() {
        let root_id = ItemId::parse("fixture-root").unwrap();
        let branch_id = ItemId::parse("fixture-branch").unwrap();
        let leaf_id = ItemId::parse("fixture-leaf").unwrap();
        let profile = ItemCompositionProfile {
            composition_id: CompositionId::parse("fixture-suite").unwrap(),
            source: ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            },
            parameters: BTreeMap::from([(CompositionParameterId::parse("node_count").unwrap(), 3)]),
        };
        let pinned = |item_id| ItemReferenceBinding::Pinned {
            item_id,
            definition_hash: Sha256Digest::parse("0".repeat(64)).unwrap(),
            quantity: 1,
        };
        let mut root = ItemDefinition::new(
            root_id.clone(),
            ItemTypeId::parse("fixture-root-type").unwrap(),
        );
        root.composition_profile = Some(profile.clone());
        root.reference_bindings.insert(
            ItemReferenceSlotId::parse("children").unwrap(),
            vec![pinned(branch_id.clone())],
        );
        let mut branch = ItemDefinition::new(
            branch_id.clone(),
            ItemTypeId::parse("fixture-branch-type").unwrap(),
        );
        branch.reference_bindings.insert(
            ItemReferenceSlotId::parse("children").unwrap(),
            vec![pinned(leaf_id.clone())],
        );
        let leaf = ItemDefinition::new(
            leaf_id.clone(),
            ItemTypeId::parse("fixture-leaf-type").unwrap(),
        );
        let expected_current = Sha256Digest::parse("b".repeat(64)).unwrap();
        let draft = CompositionDraft::new(
            CompositionDraftId::parse("fixture-draft-dag").unwrap(),
            GamePackId::parse("fixture-game").unwrap(),
            Sha256Digest::parse("a".repeat(64)).unwrap(),
            root_id.clone(),
            profile,
            BTreeMap::from([
                (
                    root_id.clone(),
                    CompositionDraftNode {
                        definition: root,
                        expected_current_definition_hash: None,
                    },
                ),
                (
                    branch_id.clone(),
                    CompositionDraftNode {
                        definition: branch,
                        expected_current_definition_hash: None,
                    },
                ),
                (
                    leaf_id.clone(),
                    CompositionDraftNode {
                        definition: leaf,
                        expected_current_definition_hash: Some(expected_current.clone()),
                    },
                ),
            ]),
            Utc::now(),
        )
        .unwrap();
        let old_branch_hash = draft.nodes[&branch_id]
            .definition
            .definition_hash()
            .unwrap();
        let old_root_hash = draft.nodes[&root_id].definition.definition_hash().unwrap();

        let mut nodes = draft.nodes.clone();
        nodes
            .get_mut(&leaf_id)
            .unwrap()
            .definition
            .resource_bindings
            .insert(
                ResourceId::parse("fixture.icon").unwrap(),
                ItemResourceBinding {
                    resource_id: ResourceId::parse("resource.fixture").unwrap(),
                    selected_version: Sha256Digest::parse("c".repeat(64)).unwrap(),
                },
            );
        let revised = draft
            .revised(nodes, draft.updated_at + chrono::Duration::seconds(1))
            .unwrap();
        let leaf_hash = revised.nodes[&leaf_id]
            .definition
            .definition_hash()
            .unwrap();
        let branch_hash = revised.nodes[&branch_id]
            .definition
            .definition_hash()
            .unwrap();
        let root_hash = revised.nodes[&root_id]
            .definition
            .definition_hash()
            .unwrap();

        assert_ne!(branch_hash, old_branch_hash);
        assert_ne!(root_hash, old_root_hash);
        assert_eq!(
            revised.nodes[&branch_id].definition.reference_bindings
                [&ItemReferenceSlotId::parse("children").unwrap()][0],
            ItemReferenceBinding::Pinned {
                item_id: leaf_id.clone(),
                definition_hash: leaf_hash,
                quantity: 1,
            }
        );
        assert_eq!(
            revised.nodes[&root_id].definition.reference_bindings
                [&ItemReferenceSlotId::parse("children").unwrap()][0],
            ItemReferenceBinding::Pinned {
                item_id: branch_id,
                definition_hash: branch_hash,
                quantity: 1,
            }
        );
        assert_eq!(
            revised.nodes[&leaf_id].expected_current_definition_hash,
            Some(expected_current)
        );
    }
}
