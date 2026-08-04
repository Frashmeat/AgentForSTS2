use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{ContributionId, FeatureId, PrimitiveId, SchemaRef, Sha256Digest};
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::{GamePackId, LoadedGamePack, PackContribution};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ContributionRequirement {
    pub slot_id: ContributionId,
    pub schema: SchemaRef,
}

#[derive(Debug, Error)]
pub enum ContributionResolverError {
    #[error("required contribution slot is missing")]
    MissingSlot,
    #[error("contribution belongs to a different Feature")]
    FeatureMismatch,
    #[error("contribution schema does not match the Feature requirement")]
    SchemaMismatch,
    #[error("required primitive is unavailable")]
    PrimitiveUnavailable,
    #[error("Feature declared a duplicate contribution requirement")]
    DuplicateRequirement,
    #[error("verified contribution payload failed typed decoding")]
    Decode(#[source] serde_json::Error),
}

#[derive(Debug, Default)]
pub struct ContributionResolver {
    available_primitives: BTreeSet<PrimitiveId>,
}

impl ContributionResolver {
    pub fn new(available_primitives: impl IntoIterator<Item = PrimitiveId>) -> Self {
        Self {
            available_primitives: available_primitives.into_iter().collect(),
        }
    }

    pub fn resolve(
        &self,
        pack: &LoadedGamePack,
        feature_id: &FeatureId,
        requirements: &[ContributionRequirement],
    ) -> Result<VerifiedContributionSet, ContributionResolverError> {
        let mut resolved = BTreeMap::new();
        for requirement in requirements {
            if resolved.contains_key(&requirement.slot_id) {
                return Err(ContributionResolverError::DuplicateRequirement);
            }
            let contribution = pack
                .contribution(&requirement.slot_id)
                .ok_or(ContributionResolverError::MissingSlot)?;
            if contribution.feature_id() != feature_id {
                return Err(ContributionResolverError::FeatureMismatch);
            }
            if contribution.schema() != &requirement.schema {
                return Err(ContributionResolverError::SchemaMismatch);
            }
            if contribution
                .required_primitives()
                .iter()
                .any(|primitive| !self.available_primitives.contains(primitive))
            {
                return Err(ContributionResolverError::PrimitiveUnavailable);
            }
            resolved.insert(requirement.slot_id.clone(), contribution.clone());
        }
        Ok(VerifiedContributionSet {
            game_pack_id: pack.id().clone(),
            game_pack_sha256: pack.content_sha256().clone(),
            feature_id: feature_id.clone(),
            contributions: resolved,
        })
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedContributionSet {
    game_pack_id: GamePackId,
    game_pack_sha256: Sha256Digest,
    feature_id: FeatureId,
    contributions: BTreeMap<ContributionId, PackContribution>,
}

impl VerifiedContributionSet {
    #[must_use]
    pub fn game_pack_id(&self) -> &GamePackId {
        &self.game_pack_id
    }

    #[must_use]
    pub fn game_pack_sha256(&self) -> &Sha256Digest {
        &self.game_pack_sha256
    }

    #[must_use]
    pub fn feature_id(&self) -> &FeatureId {
        &self.feature_id
    }

    pub fn decode<T>(&self, slot_id: &ContributionId) -> Result<T, ContributionResolverError>
    where
        T: DeserializeOwned,
    {
        self.contributions
            .get(slot_id)
            .ok_or(ContributionResolverError::MissingSlot)?
            .decode()
            .map_err(ContributionResolverError::Decode)
    }

    pub fn declares_primitive(
        &self,
        slot_id: &ContributionId,
        primitive: &PrimitiveId,
    ) -> Result<bool, ContributionResolverError> {
        Ok(self
            .contributions
            .get(slot_id)
            .ok_or(ContributionResolverError::MissingSlot)?
            .required_primitives()
            .contains(primitive))
    }
}

#[cfg(test)]
mod tests {
    use ats_kernel::{SchemaId, SchemaVersion};
    use serde::Deserialize;
    use sha2::{Digest, Sha256};

    use crate::GamePackLoader;

    use super::*;

    #[derive(Deserialize)]
    struct Rules {
        format: String,
    }

    fn schema(version: u32) -> SchemaRef {
        SchemaRef {
            id: SchemaId::parse("pack.log-rules").unwrap(),
            version: SchemaVersion::new(version).unwrap(),
        }
    }

    fn pack() -> LoadedGamePack {
        let json = r#"{"schemaVersion":3,"id":"fixture-game","displayName":"Fixture","itemTypes":[{"id":"fixture_item","displayNames":{"eng":"Fixture item"},"evidenceQueries":[{"symbols":["Fixture.Symbol"],"terms":[]}]}],"contributions":[{"slotId":"log.analyze.rules","featureId":"log.analyze","schema":{"id":"pack.log-rules","version":1},"requiredPrimitives":["log.parser"],"payload":{"format":"fixture"}}]}"#;
        let hash = Sha256Digest::parse(format!("{:x}", Sha256::digest(json.as_bytes()))).unwrap();
        GamePackLoader::load(json.as_bytes(), &hash).unwrap()
    }

    #[test]
    fn resolves_and_decodes_only_complete_typed_requirements() {
        let slot = ContributionId::parse("log.analyze.rules").unwrap();
        let requirement = ContributionRequirement {
            slot_id: slot.clone(),
            schema: schema(1),
        };
        let resolver = ContributionResolver::new([PrimitiveId::parse("log.parser").unwrap()]);
        let verified = resolver
            .resolve(
                &pack(),
                &FeatureId::parse("log.analyze").unwrap(),
                &[requirement],
            )
            .unwrap();
        assert_eq!(verified.decode::<Rules>(&slot).unwrap().format, "fixture");
    }

    #[test]
    fn rejects_missing_slot_schema_feature_and_primitive() {
        let pack = pack();
        let feature = FeatureId::parse("log.analyze").unwrap();
        let slot = ContributionId::parse("log.analyze.rules").unwrap();
        let requirement = |slot_id, schema| ContributionRequirement { slot_id, schema };

        let resolver = ContributionResolver::default();
        assert!(matches!(
            resolver.resolve(&pack, &feature, &[requirement(slot.clone(), schema(1))]),
            Err(ContributionResolverError::PrimitiveUnavailable)
        ));

        let resolver = ContributionResolver::new([PrimitiveId::parse("log.parser").unwrap()]);
        assert!(matches!(
            resolver.resolve(&pack, &feature, &[requirement(slot.clone(), schema(2))]),
            Err(ContributionResolverError::SchemaMismatch)
        ));
        assert!(matches!(
            resolver.resolve(
                &pack,
                &FeatureId::parse("mod.plan").unwrap(),
                &[requirement(slot, schema(1))]
            ),
            Err(ContributionResolverError::FeatureMismatch)
        ));
        assert!(matches!(
            resolver.resolve(
                &pack,
                &feature,
                &[requirement(
                    ContributionId::parse("log.analyze.missing").unwrap(),
                    schema(1)
                )]
            ),
            Err(ContributionResolverError::MissingSlot)
        ));
    }
}
