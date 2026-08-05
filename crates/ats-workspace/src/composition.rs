use std::collections::BTreeMap;

use ats_kernel::{CompositionDraftId, GamePackId, ItemId, Sha256Digest};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::{ItemCompositionProfile, ItemDefinition};

pub const COMPOSITION_DRAFT_SCHEMA_VERSION: u32 = 1;

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
        nodes: BTreeMap<ItemId, CompositionDraftNode>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, CompositionDraftError> {
        let draft = Self {
            schema_version: COMPOSITION_DRAFT_SCHEMA_VERSION,
            draft_id,
            revision: 1,
            game_pack_id,
            game_pack_sha256,
            root_item_id,
            profile,
            nodes,
            created_at,
            updated_at: created_at,
        };
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
            created_at: wire.created_at,
            updated_at: wire.updated_at,
        };
        draft.validate().map_err(serde::de::Error::custom)?;
        Ok(draft)
    }
}

pub trait CompositionDraftRepository: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn create(&self, draft: &CompositionDraft) -> Result<(), Self::Error>;
    fn load(&self, draft_id: &CompositionDraftId) -> Result<CompositionDraft, Self::Error>;
    fn compare_and_set(
        &self,
        expected_revision: u64,
        next: &CompositionDraft,
    ) -> Result<(), Self::Error>;
    fn list(&self) -> Result<Vec<CompositionDraft>, Self::Error>;
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
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ats_kernel::{CompositionId, CompositionParameterId, CompositionProfileId, ItemTypeId};

    use super::*;
    use crate::{ItemCompositionSource, ItemDefinition};

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
        assert_eq!(wire["schemaVersion"], 1);
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
}
