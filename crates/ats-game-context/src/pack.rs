use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{ContributionId, FeatureId, GamePackId, PrimitiveId, SchemaRef, Sha256Digest};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{CompositionProfileError, CompositionProfileSet, ItemCatalogError, ItemTypeDescriptor};

pub const GAME_PACK_SCHEMA_VERSION: u32 = 4;
pub(crate) const BUILT_IN_STS2_SHA256: &str =
    "96f6dfa86b993812a394189477c0b9a6608427f79268aec8d722f76cbde0b3f8";
const BUILT_IN_STS2: &[u8] = include_bytes!("../../../game_packs/sts2/stage2-game-pack.json");

#[derive(Debug, Clone)]
pub struct PackContribution {
    slot_id: ContributionId,
    feature_id: FeatureId,
    schema: SchemaRef,
    required_primitives: Vec<PrimitiveId>,
    payload: serde_json::Value,
}

impl PackContribution {
    #[must_use]
    pub fn slot_id(&self) -> &ContributionId {
        &self.slot_id
    }

    #[must_use]
    pub fn feature_id(&self) -> &FeatureId {
        &self.feature_id
    }

    #[must_use]
    pub fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    #[must_use]
    pub fn required_primitives(&self) -> &[PrimitiveId] {
        &self.required_primitives
    }

    pub(crate) fn decode<T>(&self) -> Result<T, serde_json::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(self.payload.clone())
    }
}

#[derive(Debug, Clone)]
pub struct LoadedGamePack {
    id: GamePackId,
    display_name: String,
    content_sha256: Sha256Digest,
    item_types: BTreeMap<ats_kernel::ItemTypeId, ItemTypeDescriptor>,
    composition_profiles: BTreeMap<ats_kernel::CompositionId, CompositionProfileSet>,
    contributions: BTreeMap<ContributionId, PackContribution>,
}

impl LoadedGamePack {
    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        GAME_PACK_SCHEMA_VERSION
    }

    #[must_use]
    pub fn id(&self) -> &GamePackId {
        &self.id
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn content_sha256(&self) -> &Sha256Digest {
        &self.content_sha256
    }

    #[must_use]
    pub fn item_types(&self) -> &BTreeMap<ats_kernel::ItemTypeId, ItemTypeDescriptor> {
        &self.item_types
    }

    #[must_use]
    pub fn item_type(&self, id: &ats_kernel::ItemTypeId) -> Option<&ItemTypeDescriptor> {
        self.item_types.get(id)
    }

    #[must_use]
    pub fn composition_profiles(
        &self,
    ) -> &BTreeMap<ats_kernel::CompositionId, CompositionProfileSet> {
        &self.composition_profiles
    }

    #[must_use]
    pub fn composition_profile(
        &self,
        id: &ats_kernel::CompositionId,
    ) -> Option<&CompositionProfileSet> {
        self.composition_profiles.get(id)
    }

    pub(crate) fn contribution(&self, slot_id: &ContributionId) -> Option<&PackContribution> {
        self.contributions.get(slot_id)
    }
}

#[derive(Debug, Error)]
pub enum GamePackLoadError {
    #[error("game pack content hash does not match the pinned identity")]
    ContentHashMismatch,
    #[error("game pack JSON is invalid")]
    InvalidJson(#[source] serde_json::Error),
    #[error("game pack schema version is unsupported")]
    UnsupportedSchema,
    #[error("game pack display name is invalid")]
    InvalidDisplayName,
    #[error("game pack contribution payload must be an object")]
    InvalidPayload,
    #[error("game pack contains a duplicate contribution slot")]
    DuplicateContribution,
    #[error("game pack contribution contains duplicate required primitives")]
    DuplicatePrimitive,
    #[error("game pack item type catalog is empty or contains duplicate item types")]
    InvalidItemTypes,
    #[error("game pack item type descriptor is invalid")]
    InvalidItemType(#[source] ItemCatalogError),
    #[error("game pack composition profile contract is invalid")]
    InvalidCompositionProfile(#[source] CompositionProfileError),
    #[error("game pack contains duplicate composition profiles or an unknown root item type")]
    InvalidCompositionProfiles,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawManifest {
    schema_version: u32,
    id: GamePackId,
    display_name: String,
    item_types: Vec<ItemTypeDescriptor>,
    #[serde(default)]
    composition_profiles: Vec<CompositionProfileSet>,
    contributions: Vec<RawContribution>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawContribution {
    slot_id: ContributionId,
    feature_id: FeatureId,
    schema: SchemaRef,
    #[serde(default)]
    required_primitives: Vec<PrimitiveId>,
    payload: serde_json::Value,
}

pub struct GamePackLoader;

impl GamePackLoader {
    pub fn load(
        bytes: &[u8],
        expected_sha256: &Sha256Digest,
    ) -> Result<LoadedGamePack, GamePackLoadError> {
        let actual = sha256_bytes(bytes);
        if &actual != expected_sha256 {
            return Err(GamePackLoadError::ContentHashMismatch);
        }
        let raw: RawManifest =
            serde_json::from_slice(bytes).map_err(GamePackLoadError::InvalidJson)?;
        if raw.schema_version != GAME_PACK_SCHEMA_VERSION {
            return Err(GamePackLoadError::UnsupportedSchema);
        }
        if raw.display_name.trim().is_empty() || raw.display_name.len() > 128 {
            return Err(GamePackLoadError::InvalidDisplayName);
        }

        if raw.item_types.is_empty() || raw.item_types.len() > 64 {
            return Err(GamePackLoadError::InvalidItemTypes);
        }
        let mut item_types = BTreeMap::new();
        for item_type in raw.item_types {
            item_type
                .validate()
                .map_err(GamePackLoadError::InvalidItemType)?;
            if item_types
                .insert(item_type.id().clone(), item_type)
                .is_some()
            {
                return Err(GamePackLoadError::InvalidItemTypes);
            }
        }

        if raw.composition_profiles.len() > 16 {
            return Err(GamePackLoadError::InvalidCompositionProfiles);
        }
        let mut composition_profiles = BTreeMap::new();
        for profile in raw.composition_profiles {
            profile
                .validate()
                .map_err(GamePackLoadError::InvalidCompositionProfile)?;
            if !item_types.contains_key(profile.root_item_type())
                || composition_profiles
                    .insert(profile.id().clone(), profile)
                    .is_some()
            {
                return Err(GamePackLoadError::InvalidCompositionProfiles);
            }
        }

        let mut contributions = BTreeMap::new();
        for raw_contribution in raw.contributions {
            if !raw_contribution.payload.is_object() {
                return Err(GamePackLoadError::InvalidPayload);
            }
            let mut primitives = BTreeSet::new();
            for primitive in &raw_contribution.required_primitives {
                if !primitives.insert(primitive) {
                    return Err(GamePackLoadError::DuplicatePrimitive);
                }
            }
            let contribution = PackContribution {
                slot_id: raw_contribution.slot_id.clone(),
                feature_id: raw_contribution.feature_id,
                schema: raw_contribution.schema,
                required_primitives: raw_contribution.required_primitives,
                payload: raw_contribution.payload,
            };
            if contributions
                .insert(raw_contribution.slot_id, contribution)
                .is_some()
            {
                return Err(GamePackLoadError::DuplicateContribution);
            }
        }

        Ok(LoadedGamePack {
            id: raw.id,
            display_name: raw.display_name,
            content_sha256: actual,
            item_types,
            composition_profiles,
            contributions,
        })
    }

    pub fn load_built_in_sts2() -> Result<LoadedGamePack, GamePackLoadError> {
        let expected = Sha256Digest::parse(BUILT_IN_STS2_SHA256)
            .map_err(|_| GamePackLoadError::ContentHashMismatch)?;
        Self::load(BUILT_IN_STS2, &expected)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum GamePackRegistryError {
    #[error("game pack is already registered")]
    DuplicatePack,
    #[error("game pack is not registered")]
    UnknownPack,
}

#[derive(Debug, Default)]
pub struct GamePackRegistry {
    packs: BTreeMap<GamePackId, LoadedGamePack>,
}

impl GamePackRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn built_in() -> Result<Self, GamePackLoadError> {
        let mut registry = Self::new();
        registry
            .register(GamePackLoader::load_built_in_sts2()?)
            .expect("new built-in registry has no duplicate IDs");
        Ok(registry)
    }

    pub fn register(&mut self, pack: LoadedGamePack) -> Result<(), GamePackRegistryError> {
        if self.packs.contains_key(pack.id()) {
            return Err(GamePackRegistryError::DuplicatePack);
        }
        self.packs.insert(pack.id.clone(), pack);
        Ok(())
    }

    pub fn require(&self, id: &GamePackId) -> Result<&LoadedGamePack, GamePackRegistryError> {
        self.packs.get(id).ok_or(GamePackRegistryError::UnknownPack)
    }
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter always returns a valid digest")
}

#[cfg(test)]
mod tests {
    use ats_kernel::{CompositionId, CompositionParameterId, ItemTypeId, ResourceId};

    use super::*;

    fn load_fixture(json: &str) -> Result<LoadedGamePack, GamePackLoadError> {
        GamePackLoader::load(json.as_bytes(), &sha256_bytes(json.as_bytes()))
    }

    #[test]
    fn pinned_loader_accepts_synthetic_and_rejects_hash_schema_and_duplicates() {
        let json = r#"{
          "schemaVersion":4,
          "id":"fixture-game",
          "displayName":"Fixture Game",
          "itemTypes":[{
            "id":"relic",
            "displayNames":{"eng":"Relic"},
            "requiredLocales":["eng"],
            "fields":[],
            "localizationFields":[{"id":"name","displayNames":{"eng":"Name"},"required":true,"multiline":false,"minLength":1,"maxLength":256}],
            "evidenceQueries":[{"symbols":["CustomRelicModel"],"terms":[]}],
            "resourceProfiles":[{"id":"default","displayNames":{"eng":"Default"},"requiredResourceRoles":[]}]
          }],
          "contributions":[{
            "slotId":"log.analyze.rules",
            "featureId":"log.analyze",
            "schema":{"id":"pack.log-rules","version":1},
            "requiredPrimitives":["log.parser"],
            "payload":{"format":"fixture"}
          }]
        }"#;
        let pack = load_fixture(json).unwrap();
        assert_eq!(pack.id().as_str(), "fixture-game");

        let wrong = Sha256Digest::parse("0".repeat(64)).unwrap();
        assert!(matches!(
            GamePackLoader::load(json.as_bytes(), &wrong),
            Err(GamePackLoadError::ContentHashMismatch)
        ));

        let bad_schema = json.replace("\"schemaVersion\":4", "\"schemaVersion\":1");
        assert!(matches!(
            load_fixture(&bad_schema),
            Err(GamePackLoadError::UnsupportedSchema)
        ));

        let duplicate = json.replace(
            "}]\n        }",
            "},{\"slotId\":\"log.analyze.rules\",\"featureId\":\"log.analyze\",\"schema\":{\"id\":\"pack.log-rules\",\"version\":1},\"payload\":{}}]\n        }",
        );
        assert!(matches!(
            load_fixture(&duplicate),
            Err(GamePackLoadError::DuplicateContribution)
        ));
    }

    #[test]
    fn built_in_sts2_uses_the_same_loader_and_pinned_identity() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        assert_eq!(pack.id().as_str(), "sts2");
        assert!(!pack.contributions.is_empty());
        assert_eq!(pack.item_types().len(), 6);
        let character = pack
            .item_type(&ItemTypeId::parse("character").unwrap())
            .expect("built-in STS2 Pack declares Character");
        assert_eq!(character.fields().len(), 7);
        assert_eq!(character.localization_fields().len(), 14);
        assert_eq!(character.reference_slots().len(), 6);
        assert_eq!(character.evidence_queries().len(), 8);
        assert_eq!(character.resource_profiles().len(), 2);
        assert!(
            character
                .resource_profiles()
                .iter()
                .find(|profile| profile.id().as_str() == "placeholder")
                .unwrap()
                .required_resource_roles()
                .is_empty()
        );
        assert_eq!(
            character
                .resource_profiles()
                .iter()
                .find(|profile| profile.id().as_str() == "branded_placeholder")
                .unwrap()
                .required_resource_roles()
                .iter()
                .map(ResourceId::as_str)
                .collect::<Vec<_>>(),
            [
                "character.top_panel_icon",
                "character.top_panel_icon_outline",
                "character.select_icon",
                "character.select_locked_icon",
                "character.map_marker",
            ]
        );
        let character_suite = pack
            .composition_profile(&CompositionId::parse("character_suite").unwrap())
            .expect("built-in STS2 Pack declares the Character suite");
        assert_eq!(character_suite.default_profile().as_str(), "standard");
        let prototype = character_suite
            .profiles()
            .iter()
            .find(|profile| profile.id().as_str() == "prototype")
            .unwrap();
        assert_eq!(
            character_suite.parameters().iter().fold(
                character_suite.base_node_count(),
                |count, parameter| {
                    count + prototype.values()[parameter.id()] * parameter.node_weight()
                },
            ),
            11
        );
        let card = pack
            .item_type(&ItemTypeId::parse("card").unwrap())
            .expect("built-in STS2 Pack declares Card");
        assert_eq!(card.fields().len(), 5);
        assert_eq!(card.evidence_queries().len(), 6);
        assert_eq!(
            card.resource_profiles()[0]
                .required_resource_roles()
                .iter()
                .map(ResourceId::as_str)
                .collect::<Vec<_>>(),
            ["card.portrait", "card.big"]
        );
        let potion = pack
            .item_type(&ItemTypeId::parse("potion").unwrap())
            .expect("built-in STS2 Pack declares Potion");
        assert_eq!(potion.fields().len(), 3);
        assert_eq!(potion.evidence_queries().len(), 7);
        assert_eq!(
            potion.resource_profiles()[0]
                .required_resource_roles()
                .iter()
                .map(ResourceId::as_str)
                .collect::<Vec<_>>(),
            ["potion.icon"]
        );
        let power = pack
            .item_type(&ItemTypeId::parse("power").unwrap())
            .expect("built-in STS2 Pack declares Power");
        assert_eq!(power.fields().len(), 4);
        assert_eq!(power.evidence_queries().len(), 6);
        assert_eq!(
            power.resource_profiles()[0]
                .required_resource_roles()
                .iter()
                .map(ResourceId::as_str)
                .collect::<Vec<_>>(),
            ["power.icon", "power.big"]
        );
    }

    #[test]
    fn item_catalog_rejects_duplicate_types_and_invalid_field_constraints() {
        let json = r#"{
          "schemaVersion":4,
          "id":"fixture-game",
          "displayName":"Fixture Game",
          "itemTypes":[{
            "id":"relic",
            "displayNames":{"eng":"Relic"},
            "fields":[],
            "evidenceQueries":[{"symbols":["CustomRelicModel"],"terms":[]}]
          }],
          "contributions":[]
        }"#;
        let mut duplicate: serde_json::Value = serde_json::from_str(json).unwrap();
        let item = duplicate["itemTypes"][0].clone();
        duplicate["itemTypes"].as_array_mut().unwrap().push(item);
        assert!(matches!(
            load_fixture(&serde_json::to_string(&duplicate).unwrap()),
            Err(GamePackLoadError::InvalidItemTypes)
        ));

        let mut invalid_field: serde_json::Value = serde_json::from_str(json).unwrap();
        invalid_field["itemTypes"][0]["fields"] = serde_json::json!([{
            "id":"cost",
            "displayNames":{"eng":"Cost"},
            "required":true,
            "value":{"kind":"integer","min":5,"max":1}
        }]);
        assert!(matches!(
            load_fixture(&serde_json::to_string(&invalid_field).unwrap()),
            Err(GamePackLoadError::InvalidItemType(
                ItemCatalogError::InvalidFields
            ))
        ));
    }

    #[test]
    fn pack_v4_validates_reference_resource_localization_and_composition_profiles() {
        let value = serde_json::json!({
            "schemaVersion":4,
            "id":"fixture-game",
            "displayName":"Fixture Game",
            "itemTypes":[
                {
                    "id":"character",
                    "displayNames":{"eng":"Character"},
                    "requiredLocales":["eng"],
                    "fields":[{
                        "id":"visual_profile",
                        "displayNames":{"eng":"Visual profile"},
                        "required":true,
                        "value":{"kind":"choice","options":[
                            {"value":"placeholder","displayNames":{"eng":"Placeholder"}},
                            {"value":"branded_placeholder","displayNames":{"eng":"Branded"}}
                        ]}
                    }],
                    "localizationFields":[{
                        "id":"title",
                        "displayNames":{"eng":"Title"},
                        "required":true,
                        "multiline":false,
                        "minLength":1,
                        "maxLength":256
                    }],
                    "referenceSlots":[{
                        "id":"starting_deck",
                        "displayNames":{"eng":"Starting deck"},
                        "kind":"pinned",
                        "allowedItemTypes":["card"],
                        "minItems":1,
                        "maxItems":8,
                        "minQuantity":1,
                        "maxQuantity":10
                    }],
                    "resourceProfileField":"visual_profile",
                    "resourceProfiles":[
                        {"id":"placeholder","displayNames":{"eng":"Placeholder"},"requiredResourceRoles":[]},
                        {"id":"branded_placeholder","displayNames":{"eng":"Branded"},"requiredResourceRoles":["character.select_icon"]}
                    ],
                    "evidenceQueries":[{"symbols":["CharacterModel"],"terms":[]}]
                },
                {
                    "id":"card",
                    "displayNames":{"eng":"Card"},
                    "evidenceQueries":[{"symbols":["CardModel"],"terms":[]}]
                }
            ],
            "compositionProfiles":[{
                "id":"character_suite",
                "displayNames":{"eng":"Character suite"},
                "rootItemType":"character",
                "defaultProfile":"standard",
                "customBaseProfile":"standard",
                "maxNodes":128,
                "baseNodeCount":1,
                "parameters":[
                    {"id":"starter_card_types","displayNames":{"eng":"Starter card types"},"min":1,"max":16,"nodeWeight":1},
                    {"id":"starting_deck_size","displayNames":{"eng":"Starting deck size"},"min":1,"max":20,"nodeWeight":0}
                ],
                "profiles":[
                    {"id":"prototype","displayNames":{"eng":"Prototype"},"values":{"starter_card_types":3,"starting_deck_size":10}},
                    {"id":"standard","displayNames":{"eng":"Standard"},"values":{"starter_card_types":4,"starting_deck_size":10}}
                ],
                "constraints":[{"kind":"less_or_equal","left":"starter_card_types","right":"starting_deck_size"}]
            }],
            "contributions":[]
        });
        let json = serde_json::to_string(&value).unwrap();
        let pack = load_fixture(&json).unwrap();
        let profiles = pack
            .composition_profile(&CompositionId::parse("character_suite").unwrap())
            .unwrap();
        assert_eq!(profiles.default_profile().as_str(), "standard");
        let custom = BTreeMap::from([
            (
                CompositionParameterId::parse("starter_card_types").unwrap(),
                8,
            ),
            (
                CompositionParameterId::parse("starting_deck_size").unwrap(),
                10,
            ),
        ]);
        profiles.validate_parameters(&custom).unwrap();

        let mut invalid = value;
        invalid["compositionProfiles"][0]["profiles"][1]["values"]["starter_card_types"] =
            serde_json::json!(11);
        invalid["compositionProfiles"][0]["profiles"][1]["values"]["starting_deck_size"] =
            serde_json::json!(10);
        assert!(matches!(
            load_fixture(&serde_json::to_string(&invalid).unwrap()),
            Err(GamePackLoadError::InvalidCompositionProfile(_))
        ));
    }
}
