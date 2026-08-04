use std::collections::BTreeSet;

use ats_game_context::{ItemFieldSpec, ItemFieldValueSpec, LoadedGamePack};
use ats_workspace::{ItemDefinition, ItemFieldValue, LocalizationStatus};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ItemDefinitionValidationMode {
    Draft,
    Ready,
}

pub struct ItemDefinitionValidator;

impl ItemDefinitionValidator {
    pub fn validate(
        pack: &LoadedGamePack,
        definition: &ItemDefinition,
        mode: ItemDefinitionValidationMode,
    ) -> Result<(), ItemDefinitionValidationError> {
        definition
            .validate()
            .map_err(|_| ItemDefinitionValidationError::InvalidDefinition)?;
        let descriptor = pack
            .item_type(&definition.item_type)
            .ok_or(ItemDefinitionValidationError::UnsupportedItemType)?;

        for (field_id, value) in &definition.canonical_fields {
            let spec = descriptor
                .fields()
                .iter()
                .find(|candidate| candidate.id() == field_id)
                .ok_or(ItemDefinitionValidationError::UnknownField)?;
            validate_field(spec, value)?;
        }
        if mode == ItemDefinitionValidationMode::Ready
            && descriptor.fields().iter().any(|field| {
                field.required() && !definition.canonical_fields.contains_key(field.id())
            })
        {
            return Err(ItemDefinitionValidationError::MissingRequiredField);
        }

        let allowed_locales = descriptor
            .required_locales()
            .iter()
            .collect::<BTreeSet<_>>();
        if definition
            .localizations
            .keys()
            .any(|locale| !allowed_locales.contains(locale))
        {
            return Err(ItemDefinitionValidationError::UnknownLocale);
        }
        if mode == ItemDefinitionValidationMode::Ready
            && descriptor.required_locales().iter().any(|locale| {
                definition
                    .localizations
                    .get(locale)
                    .is_none_or(|value| value.status != LocalizationStatus::Confirmed)
            })
        {
            return Err(ItemDefinitionValidationError::LocalizationIncomplete);
        }

        let allowed_roles = descriptor
            .required_resource_roles()
            .iter()
            .collect::<BTreeSet<_>>();
        if definition
            .resource_bindings
            .keys()
            .any(|role| !allowed_roles.contains(role))
        {
            return Err(ItemDefinitionValidationError::UnknownResourceRole);
        }
        if mode == ItemDefinitionValidationMode::Ready
            && (definition.resource_bindings.len() != allowed_roles.len()
                || definition.behavior_intent.is_empty())
        {
            return Err(ItemDefinitionValidationError::DefinitionIncomplete);
        }
        Ok(())
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ItemDefinitionValidationError {
    #[error("item definition contract is invalid")]
    InvalidDefinition,
    #[error("item type is not declared by the Game Pack")]
    UnsupportedItemType,
    #[error("item definition contains a field not declared by the Game Pack")]
    UnknownField,
    #[error("item definition field value does not match the Pack descriptor")]
    InvalidFieldValue,
    #[error("item definition is missing a required Pack field")]
    MissingRequiredField,
    #[error("item definition contains an undeclared locale")]
    UnknownLocale,
    #[error("item definition required localizations are missing, outdated, or unconfirmed")]
    LocalizationIncomplete,
    #[error("item definition contains an undeclared Resource role")]
    UnknownResourceRole,
    #[error("item definition is not ready for generation")]
    DefinitionIncomplete,
}

fn validate_field(
    spec: &ItemFieldSpec,
    value: &ItemFieldValue,
) -> Result<(), ItemDefinitionValidationError> {
    let valid = match (spec.value(), value) {
        (
            ItemFieldValueSpec::Text {
                min_length,
                max_length,
                ..
            },
            ItemFieldValue::Text(value),
        ) => char_count_in_range(value, *min_length, *max_length),
        (ItemFieldValueSpec::Integer { min, max }, ItemFieldValue::Integer(value)) => {
            (min..=max).contains(&value)
        }
        (ItemFieldValueSpec::Boolean, ItemFieldValue::Boolean(_)) => true,
        (ItemFieldValueSpec::Choice { options }, ItemFieldValue::Choice(value)) => {
            options.iter().any(|option| option.value() == value)
        }
        (
            ItemFieldValueSpec::StringList {
                min_items,
                max_items,
                item_max_length,
            },
            ItemFieldValue::StringList(values),
        ) => {
            u32::try_from(values.len())
                .is_ok_and(|length| (*min_items..=*max_items).contains(&length))
                && values
                    .iter()
                    .all(|value| char_count_in_range(value, 1, *item_max_length))
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(ItemDefinitionValidationError::InvalidFieldValue)
    }
}

fn char_count_in_range(value: &str, min: u32, max: u32) -> bool {
    u32::try_from(value.chars().count()).is_ok_and(|count| (min..=max).contains(&count))
}

#[cfg(test)]
mod tests {
    use ats_game_context::GamePackLoader;
    use ats_kernel::{ItemFieldId, ItemId, ItemTypeId, LocaleId, ResourceId, Sha256Digest};
    use ats_workspace::{ItemLocalization, ItemResourceBinding, LocalizationStatus};

    use super::*;

    fn relic() -> ItemDefinition {
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture-relic").unwrap(),
            ItemTypeId::parse("relic").unwrap(),
        );
        definition.canonical_fields.insert(
            ItemFieldId::parse("rarity").unwrap(),
            ItemFieldValue::Choice("common".into()),
        );
        definition
    }

    #[test]
    fn draft_accepts_incomplete_content_but_rejects_pack_drift() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let definition = relic();
        ItemDefinitionValidator::validate(&pack, &definition, ItemDefinitionValidationMode::Draft)
            .unwrap();
        assert!(matches!(
            ItemDefinitionValidator::validate(
                &pack,
                &definition,
                ItemDefinitionValidationMode::Ready
            ),
            Err(ItemDefinitionValidationError::LocalizationIncomplete)
        ));

        let mut drift = definition;
        drift.canonical_fields.insert(
            ItemFieldId::parse("unknown_field").unwrap(),
            ItemFieldValue::Boolean(true),
        );
        assert!(matches!(
            ItemDefinitionValidator::validate(&pack, &drift, ItemDefinitionValidationMode::Draft),
            Err(ItemDefinitionValidationError::UnknownField)
        ));
    }

    #[test]
    fn ready_requires_confirmed_locales_resources_and_behavior() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let mut definition = relic();
        for (locale, name) in [("eng", "Fixture Relic"), ("zhs", "测试遗物")] {
            definition.localizations.insert(
                LocaleId::parse(locale).unwrap(),
                ItemLocalization {
                    name: name.into(),
                    description: "A confirmed description.".into(),
                    status: LocalizationStatus::Confirmed,
                    translated_from: None,
                },
            );
        }
        definition.behavior_intent = vec!["Gain one observable effect.".into()];
        let selected_version = Sha256Digest::parse("a".repeat(64)).unwrap();
        for role in ["relic.normal", "relic.outline", "relic.big"] {
            definition.resource_bindings.insert(
                ResourceId::parse(role).unwrap(),
                ItemResourceBinding {
                    resource_id: ResourceId::parse(format!("resource.{}", role.replace('.', "-")))
                        .unwrap(),
                    selected_version: selected_version.clone(),
                },
            );
        }
        ItemDefinitionValidator::validate(&pack, &definition, ItemDefinitionValidationMode::Ready)
            .unwrap();
    }
}
