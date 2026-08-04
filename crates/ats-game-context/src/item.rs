use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{GamePackId, ItemFieldId, ItemTypeId, LocaleId, ResourceId, Sha256Digest};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{EvidenceQuery, EvidenceQueryError, LoadedGamePack, VerifiedTruthSnapshot};

const MAX_EVIDENCE_RECORDS: u16 = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemTypeDescriptor {
    id: ItemTypeId,
    display_names: BTreeMap<LocaleId, String>,
    #[serde(default)]
    required_locales: Vec<LocaleId>,
    #[serde(default)]
    fields: Vec<ItemFieldSpec>,
    evidence_queries: Vec<ItemEvidenceQuery>,
    #[serde(default)]
    required_resource_roles: Vec<ResourceId>,
}

impl ItemTypeDescriptor {
    #[must_use]
    pub fn id(&self) -> &ItemTypeId {
        &self.id
    }

    #[must_use]
    pub fn display_names(&self) -> &BTreeMap<LocaleId, String> {
        &self.display_names
    }

    #[must_use]
    pub fn required_locales(&self) -> &[LocaleId] {
        &self.required_locales
    }

    #[must_use]
    pub fn fields(&self) -> &[ItemFieldSpec] {
        &self.fields
    }

    #[must_use]
    pub fn evidence_queries(&self) -> &[ItemEvidenceQuery] {
        &self.evidence_queries
    }

    #[must_use]
    pub fn required_resource_roles(&self) -> &[ResourceId] {
        &self.required_resource_roles
    }

    pub(crate) fn validate(&self) -> Result<(), ItemCatalogError> {
        validate_display_names(&self.display_names)?;
        if self.required_locales.len() > 32
            || has_duplicates(&self.required_locales)
            || self
                .required_locales
                .iter()
                .any(|locale| !self.display_names.contains_key(locale))
        {
            return Err(ItemCatalogError::InvalidLocales);
        }
        if self.fields.len() > 128
            || has_duplicates_by(&self.fields, ItemFieldSpec::id)
            || self.fields.iter().any(|field| field.validate().is_err())
        {
            return Err(ItemCatalogError::InvalidFields);
        }
        if self.evidence_queries.is_empty()
            || self.evidence_queries.len() > 64
            || self
                .evidence_queries
                .iter()
                .any(|query| query.validate().is_err())
        {
            return Err(ItemCatalogError::InvalidEvidenceQueries);
        }
        if self.required_resource_roles.len() > 128 || has_duplicates(&self.required_resource_roles)
        {
            return Err(ItemCatalogError::InvalidResourceRoles);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemFieldSpec {
    id: ItemFieldId,
    display_names: BTreeMap<LocaleId, String>,
    required: bool,
    value: ItemFieldValueSpec,
}

impl ItemFieldSpec {
    #[must_use]
    pub fn id(&self) -> &ItemFieldId {
        &self.id
    }

    #[must_use]
    pub fn display_names(&self) -> &BTreeMap<LocaleId, String> {
        &self.display_names
    }

    #[must_use]
    pub const fn required(&self) -> bool {
        self.required
    }

    #[must_use]
    pub const fn value(&self) -> &ItemFieldValueSpec {
        &self.value
    }

    fn validate(&self) -> Result<(), ItemCatalogError> {
        validate_display_names(&self.display_names)?;
        self.value.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemFieldValueSpec {
    Text {
        multiline: bool,
        min_length: u32,
        max_length: u32,
    },
    Integer {
        min: i64,
        max: i64,
    },
    Boolean,
    Choice {
        options: Vec<ItemChoiceOption>,
    },
    StringList {
        min_items: u32,
        max_items: u32,
        item_max_length: u32,
    },
}

impl ItemFieldValueSpec {
    fn validate(&self) -> Result<(), ItemCatalogError> {
        let valid = match self {
            Self::Text {
                min_length,
                max_length,
                ..
            } => min_length <= max_length && *max_length <= 8_000,
            Self::Integer { min, max } => min <= max,
            Self::Boolean => true,
            Self::Choice { options } => {
                !options.is_empty()
                    && options.len() <= 128
                    && !has_duplicates_by(options, |option| &option.value)
                    && options.iter().all(ItemChoiceOption::is_valid)
            }
            Self::StringList {
                min_items,
                max_items,
                item_max_length,
            } => {
                min_items <= max_items && *max_items <= 128 && (1..=4_000).contains(item_max_length)
            }
        };
        if valid {
            Ok(())
        } else {
            Err(ItemCatalogError::InvalidFields)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemChoiceOption {
    value: String,
    display_names: BTreeMap<LocaleId, String>,
}

impl ItemChoiceOption {
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }

    #[must_use]
    pub fn display_names(&self) -> &BTreeMap<LocaleId, String> {
        &self.display_names
    }

    fn is_valid(&self) -> bool {
        valid_local_value(&self.value) && validate_display_names(&self.display_names).is_ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemEvidenceQuery {
    symbols: Vec<String>,
    terms: Vec<String>,
}

impl ItemEvidenceQuery {
    #[must_use]
    pub fn symbols(&self) -> &[String] {
        &self.symbols
    }

    #[must_use]
    pub fn terms(&self) -> &[String] {
        &self.terms
    }

    #[must_use]
    pub fn as_query(&self) -> EvidenceQuery {
        EvidenceQuery {
            symbols: self.symbols.clone(),
            terms: self.terms.clone(),
            limit: MAX_EVIDENCE_RECORDS,
        }
    }

    fn validate(&self) -> Result<(), ItemCatalogError> {
        if (self.symbols.is_empty() && self.terms.is_empty())
            || self.symbols.len() > 32
            || self.terms.len() > 32
            || self
                .symbols
                .iter()
                .chain(&self.terms)
                .any(|value| !valid_text(value, 128))
            || has_duplicates(&self.symbols)
            || has_duplicates(&self.terms)
        {
            Err(ItemCatalogError::InvalidEvidenceQueries)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ItemCatalogError {
    #[error("item type display names or required locales are invalid")]
    InvalidLocales,
    #[error("item type field descriptors are invalid")]
    InvalidFields,
    #[error("item type evidence queries are invalid")]
    InvalidEvidenceQueries,
    #[error("item type resource roles are invalid")]
    InvalidResourceRoles,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemCapabilityCatalog {
    pub game_pack_id: GamePackId,
    pub game_pack_sha256: Sha256Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truth_snapshot_id: Option<Sha256Digest>,
    pub item_types: Vec<ItemTypeCapability>,
}

impl ItemCapabilityCatalog {
    pub fn evaluate(
        pack: &LoadedGamePack,
        truth: Option<&VerifiedTruthSnapshot>,
    ) -> Result<Self, CapabilityEvaluationError> {
        if truth.is_some_and(|snapshot| {
            snapshot.manifest().game_pack_id() != pack.id()
                || snapshot.manifest().game_pack_sha256() != pack.content_sha256()
        }) {
            return Err(CapabilityEvaluationError::TruthPackMismatch);
        }

        let mut item_types = Vec::with_capacity(pack.item_types().len());
        for descriptor in pack.item_types().values() {
            let blockers = if let Some(snapshot) = truth {
                descriptor
                    .evidence_queries()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, query)| match snapshot.query(&query.as_query()) {
                        Ok(records) if records.is_empty() => {
                            Some(Ok(ItemCapabilityBlocker::MissingEvidence {
                                query_index: u32::try_from(index)
                                    .expect("Pack query count is bounded by validation"),
                            }))
                        }
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                vec![ItemCapabilityBlocker::TruthSnapshotUnavailable]
            };
            item_types.push(ItemTypeCapability {
                descriptor: descriptor.clone(),
                ready: blockers.is_empty(),
                blockers,
            });
        }

        Ok(Self {
            game_pack_id: pack.id().clone(),
            game_pack_sha256: pack.content_sha256().clone(),
            truth_snapshot_id: truth.map(|snapshot| snapshot.manifest().snapshot_id().clone()),
            item_types,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemTypeCapability {
    pub descriptor: ItemTypeDescriptor,
    pub ready: bool,
    pub blockers: Vec<ItemCapabilityBlocker>,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(tag = "code", rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemCapabilityBlocker {
    #[serde(rename = "truth.snapshot_unavailable")]
    TruthSnapshotUnavailable,
    #[serde(rename = "truth.evidence_missing")]
    MissingEvidence {
        #[serde(rename = "queryIndex")]
        query_index: u32,
    },
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CapabilityEvaluationError {
    #[error("verified Truth Snapshot does not belong to the selected Game Pack")]
    TruthPackMismatch,
    #[error("Pack evidence query is invalid")]
    EvidenceQuery(#[from] EvidenceQueryError),
}

fn validate_display_names(names: &BTreeMap<LocaleId, String>) -> Result<(), ItemCatalogError> {
    if names.is_empty() || names.len() > 32 || names.values().any(|value| !valid_text(value, 128)) {
        Err(ItemCatalogError::InvalidLocales)
    } else {
        Ok(())
    }
}

fn valid_text(value: &str, max_len: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max_len && !value.chars().any(char::is_control)
}

fn valid_local_value(value: &str) -> bool {
    value.len() <= 64
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn has_duplicates<T: Ord>(values: &[T]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() != values.len()
}

fn has_duplicates_by<T, K: Ord + ?Sized>(values: &[T], key: impl Fn(&T) -> &K) -> bool {
    values.iter().map(key).collect::<BTreeSet<_>>().len() != values.len()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use sha2::{Digest, Sha256};

    use crate::{
        GamePackLoader, TruthEvidenceRecord, TruthSnapshotIndex, TruthSnapshotManifest,
        TruthSnapshotSource,
    };

    use super::*;

    fn digest(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
    }

    fn pack() -> LoadedGamePack {
        let value = serde_json::json!({
            "schemaVersion": 3,
            "id": "fixture-game",
            "displayName": "Fixture Game",
            "itemTypes": [
                {
                    "id": "ready_item",
                    "displayNames": {"eng":"Ready item", "zhs":"就绪项目"},
                    "requiredLocales": ["eng", "zhs"],
                    "fields": [{
                        "id": "rarity",
                        "displayNames": {"eng":"Rarity", "zhs":"稀有度"},
                        "required": true,
                        "value": {
                            "kind": "choice",
                            "options": [{
                                "value":"common",
                                "displayNames":{"eng":"Common", "zhs":"普通"}
                            }]
                        }
                    }],
                    "evidenceQueries": [{"symbols":["Fixture.Symbol"],"terms":[]}],
                    "requiredResourceRoles": ["fixture.icon"]
                },
                {
                    "id": "blocked_item",
                    "displayNames": {"eng":"Blocked item"},
                    "evidenceQueries": [{"symbols":["Missing.Symbol"],"terms":[]}]
                }
            ],
            "contributions": []
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        GamePackLoader::load(&bytes, &digest(&bytes)).unwrap()
    }

    fn snapshot(pack: &LoadedGamePack) -> VerifiedTruthSnapshot {
        let records = vec![TruthEvidenceRecord {
            source_id: "game".into(),
            symbol: "Fixture.Symbol".into(),
            purpose: "fixture purpose".into(),
            bounded_excerpt: "Fixture.Symbol is available.".into(),
            relative_path: "indexes/game/Fixture.cs".into(),
        }];
        let index_bytes = serde_json::to_vec(&records).unwrap();
        let manifest = TruthSnapshotManifest::new(
            pack,
            vec![TruthSnapshotSource {
                id: "game".into(),
                kind: "local_file".into(),
                version: Some("1".into()),
                relative_path: "sources/game.bin".into(),
                sha256: digest(b"game"),
                byte_length: 4,
            }],
            vec![TruthSnapshotIndex {
                id: "symbols".into(),
                provider: ats_kernel::PrimitiveId::parse("truth.fixture-indexer").unwrap(),
                relative_path: "indexes/symbols.json".into(),
                sha256: digest(&index_bytes),
                record_count: 1,
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

    #[test]
    fn capability_catalog_is_pack_driven_and_truth_scoped() {
        let pack = pack();
        let unavailable = ItemCapabilityCatalog::evaluate(&pack, None).unwrap();
        assert!(unavailable.item_types.iter().all(|item| {
            !item.ready && item.blockers == vec![ItemCapabilityBlocker::TruthSnapshotUnavailable]
        }));

        let truth = snapshot(&pack);
        let catalog = ItemCapabilityCatalog::evaluate(&pack, Some(&truth)).unwrap();
        assert_eq!(
            catalog.truth_snapshot_id,
            Some(truth.manifest().snapshot_id().clone())
        );
        assert!(
            catalog
                .item_types
                .iter()
                .find(|item| item.descriptor.id().as_str() == "ready_item")
                .unwrap()
                .ready
        );
        let blocked = catalog
            .item_types
            .iter()
            .find(|item| item.descriptor.id().as_str() == "blocked_item")
            .unwrap();
        assert_eq!(
            blocked.blockers,
            vec![ItemCapabilityBlocker::MissingEvidence { query_index: 0 }]
        );
        let encoded = serde_json::to_value(blocked).unwrap();
        assert_eq!(encoded["blockers"][0]["code"], "truth.evidence_missing");
        assert_eq!(encoded["blockers"][0]["queryIndex"], 0);
        assert!(encoded["blockers"][0].get("query_index").is_none());
    }

    #[test]
    fn descriptor_exposes_validated_generic_field_contracts() {
        let pack = pack();
        let descriptor = pack
            .item_type(&ItemTypeId::parse("ready_item").unwrap())
            .unwrap();
        assert_eq!(descriptor.fields()[0].id().as_str(), "rarity");
        assert!(matches!(
            descriptor.fields()[0].value(),
            ItemFieldValueSpec::Choice { options } if options[0].value() == "common"
        ));
        assert_eq!(
            descriptor.required_resource_roles()[0].as_str(),
            "fixture.icon"
        );
    }
}
