use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{ContributionId, FeatureId, GamePackId, PrimitiveId, SchemaRef, Sha256Digest};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const GAME_PACK_SCHEMA_VERSION: u32 = 2;
const BUILT_IN_STS2_SHA256: &str =
    "e12b9fa3aab96efeabcb2b52953a9e39b705ae6115012d0272f52952f21b3fc9";
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawManifest {
    schema_version: u32,
    id: GamePackId,
    display_name: String,
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
    use super::*;

    fn load_fixture(json: &str) -> Result<LoadedGamePack, GamePackLoadError> {
        GamePackLoader::load(json.as_bytes(), &sha256_bytes(json.as_bytes()))
    }

    #[test]
    fn pinned_loader_accepts_synthetic_and_rejects_hash_schema_and_duplicates() {
        let json = r#"{
          "schemaVersion":2,
          "id":"fixture-game",
          "displayName":"Fixture Game",
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

        let bad_schema = json.replace("\"schemaVersion\":2", "\"schemaVersion\":1");
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
    }
}
