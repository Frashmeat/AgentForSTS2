use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{ItemFieldId, ItemId, ItemTypeId, LocaleId, ResourceId, Sha256Digest};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const ITEM_DEFINITION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ItemFieldValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    Choice(String),
    StringList(Vec<String>),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LocalizationStatus {
    Confirmed,
    Outdated,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemLocalization {
    pub name: String,
    pub description: String,
    pub status: LocalizationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translated_from: Option<LocaleId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemResourceBinding {
    pub resource_id: ResourceId,
    pub selected_version: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemDefinition {
    schema_version: u32,
    pub item_id: ItemId,
    pub item_type: ItemTypeId,
    pub canonical_fields: BTreeMap<ItemFieldId, ItemFieldValue>,
    pub behavior_intent: Vec<String>,
    pub localizations: BTreeMap<LocaleId, ItemLocalization>,
    pub resource_bindings: BTreeMap<ResourceId, ItemResourceBinding>,
}

impl ItemDefinition {
    #[must_use]
    pub fn new(item_id: ItemId, item_type: ItemTypeId) -> Self {
        Self {
            schema_version: ITEM_DEFINITION_SCHEMA_VERSION,
            item_id,
            item_type,
            canonical_fields: BTreeMap::new(),
            behavior_intent: Vec::new(),
            localizations: BTreeMap::new(),
            resource_bindings: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn validate(&self) -> Result<(), ItemDefinitionError> {
        if self.schema_version != ITEM_DEFINITION_SCHEMA_VERSION {
            return Err(ItemDefinitionError::UnsupportedSchema);
        }
        if self.canonical_fields.len() > 128
            || self
                .canonical_fields
                .values()
                .any(|value| !valid_field_value(value))
        {
            return Err(ItemDefinitionError::InvalidCanonicalFields);
        }
        if self.behavior_intent.len() > 64
            || self
                .behavior_intent
                .iter()
                .any(|value| !valid_text(value, 4_000))
        {
            return Err(ItemDefinitionError::InvalidBehaviorIntent);
        }
        if self.localizations.len() > 32
            || self.localizations.iter().any(|(locale, value)| {
                !valid_text(&value.name, 256)
                    || !valid_text(&value.description, 8_000)
                    || value.translated_from.as_ref() == Some(locale)
                    || (value.status == LocalizationStatus::Outdated
                        && value.translated_from.is_none())
                    || value
                        .translated_from
                        .as_ref()
                        .is_some_and(|source| !self.localizations.contains_key(source))
            })
        {
            return Err(ItemDefinitionError::InvalidLocalization);
        }
        if self.resource_bindings.len() > 128 {
            return Err(ItemDefinitionError::InvalidResourceBindings);
        }
        Ok(())
    }

    pub fn definition_hash(&self) -> Result<Sha256Digest, ItemDefinitionError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(ItemDefinitionError::Serialize)?;
        Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
            .map_err(|_| ItemDefinitionError::InvalidHash)
    }
}

impl<'de> Deserialize<'de> for ItemDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            item_id: ItemId,
            item_type: ItemTypeId,
            canonical_fields: BTreeMap<ItemFieldId, ItemFieldValue>,
            behavior_intent: Vec<String>,
            localizations: BTreeMap<LocaleId, ItemLocalization>,
            resource_bindings: BTreeMap<ResourceId, ItemResourceBinding>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let definition = Self {
            schema_version: wire.schema_version,
            item_id: wire.item_id,
            item_type: wire.item_type,
            canonical_fields: wire.canonical_fields,
            behavior_intent: wire.behavior_intent,
            localizations: wire.localizations,
            resource_bindings: wire.resource_bindings,
        };
        definition.validate().map_err(serde::de::Error::custom)?;
        Ok(definition)
    }
}

#[derive(Debug, Error)]
pub enum ItemDefinitionError {
    #[error("item definition schema version is unsupported")]
    UnsupportedSchema,
    #[error("item definition canonical fields are invalid")]
    InvalidCanonicalFields,
    #[error("item definition behavior intent is invalid")]
    InvalidBehaviorIntent,
    #[error("item definition localization is invalid")]
    InvalidLocalization,
    #[error("item definition resource bindings are invalid")]
    InvalidResourceBindings,
    #[error("item definition cannot be serialized")]
    Serialize(#[source] serde_json::Error),
    #[error("item definition identity cannot be represented")]
    InvalidHash,
}

fn valid_text(value: &str, max_len: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_len
        && !value.chars().any(|character| character.is_control())
}

fn valid_field_value(value: &ItemFieldValue) -> bool {
    match value {
        ItemFieldValue::Text(value) | ItemFieldValue::Choice(value) => valid_text(value, 4_000),
        ItemFieldValue::Integer(_) | ItemFieldValue::Boolean(_) => true,
        ItemFieldValue::StringList(values) => {
            values.len() <= 128
                && values.iter().all(|value| valid_text(value, 1_000))
                && values.iter().collect::<BTreeSet<_>>().len() == values.len()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition() -> ItemDefinition {
        let mut definition = ItemDefinition::new(
            ItemId::parse("burning-blood").unwrap(),
            ItemTypeId::parse("relic").unwrap(),
        );
        definition.canonical_fields.insert(
            ItemFieldId::parse("rarity").unwrap(),
            ItemFieldValue::Choice("starter".into()),
        );
        definition.behavior_intent = vec!["Heal the player after combat.".into()];
        definition.localizations.insert(
            LocaleId::parse("eng").unwrap(),
            ItemLocalization {
                name: "Burning Blood".into(),
                description: "At the end of combat, heal 6 HP.".into(),
                status: LocalizationStatus::Confirmed,
                translated_from: None,
            },
        );
        definition
    }

    #[test]
    fn definition_hash_is_stable_and_covers_canonical_content() {
        let first = definition();
        let encoded = serde_json::to_string(&first).unwrap();
        let decoded: ItemDefinition = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            decoded.definition_hash().unwrap(),
            first.definition_hash().unwrap()
        );

        let mut changed = first.clone();
        changed.behavior_intent[0].push_str(" Only once.");
        assert_ne!(
            changed.definition_hash().unwrap(),
            first.definition_hash().unwrap()
        );
    }

    #[test]
    fn invalid_definition_content_cannot_receive_an_identity() {
        let mut invalid = definition();
        invalid.behavior_intent.push("\n".into());
        assert!(matches!(
            invalid.definition_hash(),
            Err(ItemDefinitionError::InvalidBehaviorIntent)
        ));

        let mut value = serde_json::to_value(definition()).unwrap();
        value["schemaVersion"] = serde_json::json!(99);
        assert!(serde_json::from_value::<ItemDefinition>(value).is_err());
    }
}
