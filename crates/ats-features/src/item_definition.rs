use std::collections::BTreeSet;

use ats_game_context::{
    ItemFieldSpec, ItemFieldValueSpec, ItemReferenceKind, ItemResourceProfileSpec,
    ItemTypeDescriptor, LoadedGamePack, LocalizationFieldSpec,
};
use ats_kernel::ResourceId;
use ats_workspace::{
    ItemCompositionSource, ItemDefinition, ItemFieldValue, ItemReferenceBinding, LocalizationStatus,
};
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
        for localization in definition.localizations.values() {
            for (field_id, value) in &localization.fields {
                let spec = descriptor
                    .localization_fields()
                    .iter()
                    .find(|candidate| candidate.id() == field_id)
                    .ok_or(ItemDefinitionValidationError::UnknownLocalizationField)?;
                validate_localization_field(spec, value)?;
            }
        }
        if mode == ItemDefinitionValidationMode::Ready
            && descriptor.required_locales().iter().any(|locale| {
                definition.localizations.get(locale).is_none_or(|value| {
                    value.status != LocalizationStatus::Confirmed
                        || descriptor.localization_fields().iter().any(|field| {
                            field.required()
                                && value.fields.get(field.id()).is_none_or(|candidate| {
                                    validate_localization_field(field, candidate).is_err()
                                })
                        })
                })
            })
        {
            return Err(ItemDefinitionValidationError::LocalizationIncomplete);
        }

        let required_roles = Self::required_resource_roles(descriptor, definition)?;
        let allowed_roles = required_roles.iter().collect::<BTreeSet<_>>();
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

        for (slot_id, bindings) in &definition.reference_bindings {
            let slot = descriptor
                .reference_slots()
                .iter()
                .find(|candidate| candidate.id() == slot_id)
                .ok_or(ItemDefinitionValidationError::UnknownReferenceSlot)?;
            if !u32::try_from(bindings.len()).is_ok_and(|count| {
                count <= slot.max_items()
                    && (mode != ItemDefinitionValidationMode::Ready || count >= slot.min_items())
            }) {
                return Err(ItemDefinitionValidationError::InvalidReferenceBinding);
            }
            for binding in bindings {
                match (slot.kind(), binding) {
                    (
                        ItemReferenceKind::Identity,
                        ItemReferenceBinding::Identity {
                            expected_item_type, ..
                        },
                    ) if slot.allowed_item_types().contains(expected_item_type) => {}
                    (ItemReferenceKind::Pinned, ItemReferenceBinding::Pinned { quantity, .. })
                        if (slot.min_quantity()..=slot.max_quantity()).contains(quantity) => {}
                    _ => return Err(ItemDefinitionValidationError::InvalidReferenceBinding),
                }
            }
        }
        if mode == ItemDefinitionValidationMode::Ready
            && descriptor.reference_slots().iter().any(|slot| {
                match u32::try_from(
                    definition
                        .reference_bindings
                        .get(slot.id())
                        .map_or(0, Vec::len),
                ) {
                    Ok(count) => count < slot.min_items() || count > slot.max_items(),
                    Err(_) => true,
                }
            })
        {
            return Err(ItemDefinitionValidationError::MissingRequiredReference);
        }

        if let Some(composition) = &definition.composition_profile {
            let profile_set = pack
                .composition_profile(&composition.composition_id)
                .ok_or(ItemDefinitionValidationError::InvalidCompositionProfile)?;
            if profile_set.root_item_type() != &definition.item_type {
                return Err(ItemDefinitionValidationError::InvalidCompositionProfile);
            }
            let source_profile = match &composition.source {
                ItemCompositionSource::Preset { profile_id } => profile_set
                    .profiles()
                    .iter()
                    .find(|profile| profile.id() == profile_id)
                    .filter(|profile| profile.values() == &composition.parameters),
                ItemCompositionSource::Custom { base_profile_id } => profile_set
                    .profiles()
                    .iter()
                    .find(|profile| profile.id() == base_profile_id),
            };
            if source_profile.is_none()
                || profile_set
                    .validate_parameters(&composition.parameters)
                    .is_err()
            {
                return Err(ItemDefinitionValidationError::InvalidCompositionProfile);
            }
        }
        Ok(())
    }

    pub fn required_resource_roles(
        descriptor: &ItemTypeDescriptor,
        definition: &ItemDefinition,
    ) -> Result<Vec<ResourceId>, ItemDefinitionValidationError> {
        let profile = selected_resource_profile(descriptor, definition)?;
        Ok(profile
            .map(ItemResourceProfileSpec::required_resource_roles)
            .unwrap_or_default()
            .to_vec())
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
    #[error("item definition contains an undeclared localization field")]
    UnknownLocalizationField,
    #[error("item definition localization field does not match the Pack descriptor")]
    InvalidLocalizationField,
    #[error("item definition required localizations are missing, outdated, or unconfirmed")]
    LocalizationIncomplete,
    #[error("item definition contains an undeclared Resource role")]
    UnknownResourceRole,
    #[error("item definition Resource profile is missing or invalid")]
    InvalidResourceProfile,
    #[error("item definition contains an undeclared reference slot")]
    UnknownReferenceSlot,
    #[error("item definition reference does not match the Pack slot")]
    InvalidReferenceBinding,
    #[error("item definition is missing a required reference")]
    MissingRequiredReference,
    #[error("item definition composition profile is invalid")]
    InvalidCompositionProfile,
    #[error("item definition is not ready for generation")]
    DefinitionIncomplete,
}

fn selected_resource_profile<'a>(
    descriptor: &'a ItemTypeDescriptor,
    definition: &ItemDefinition,
) -> Result<Option<&'a ItemResourceProfileSpec>, ItemDefinitionValidationError> {
    let Some(selector) = descriptor.resource_profile_field() else {
        return Ok(descriptor.resource_profiles().first());
    };
    let Some(ItemFieldValue::Choice(selected)) = definition.canonical_fields.get(selector) else {
        return if definition.resource_bindings.is_empty() {
            Ok(None)
        } else {
            Err(ItemDefinitionValidationError::InvalidResourceProfile)
        };
    };
    descriptor
        .resource_profiles()
        .iter()
        .find(|profile| profile.id().as_str() == selected)
        .map(Some)
        .ok_or(ItemDefinitionValidationError::InvalidResourceProfile)
}

fn validate_localization_field(
    spec: &LocalizationFieldSpec,
    value: &str,
) -> Result<(), ItemDefinitionValidationError> {
    if char_count_in_range(value, spec.min_length(), spec.max_length()) {
        Ok(())
    } else {
        Err(ItemDefinitionValidationError::InvalidLocalizationField)
    }
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
    use std::collections::BTreeMap;

    use ats_game_context::GamePackLoader;
    use ats_kernel::{
        CompositionId, CompositionParameterId, CompositionProfileId, ItemFieldId, ItemId,
        ItemReferenceSlotId, ItemTypeId, LocaleId, LocalizationFieldId, ResourceId, Sha256Digest,
    };
    use ats_workspace::{
        ItemCompositionProfile, ItemCompositionSource, ItemLocalization, ItemReferenceBinding,
        ItemResourceBinding, LocalizationStatus,
    };
    use sha2::{Digest, Sha256};

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
                    fields: std::collections::BTreeMap::from([
                        (LocalizationFieldId::parse("name").unwrap(), name.into()),
                        (
                            LocalizationFieldId::parse("description").unwrap(),
                            "A confirmed description.".into(),
                        ),
                    ]),
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

    #[test]
    fn ready_validation_resolves_profile_references_and_composition_parameters() {
        let value = serde_json::json!({
            "schemaVersion":5,
            "id":"fixture-game",
            "displayName":"Fixture",
            "behavior": crate::fixture_behavior_json("character"),
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
                        "id":"title","displayNames":{"eng":"Title"},"required":true,
                        "multiline":false,"minLength":1,"maxLength":256
                    }],
                    "referenceSlots":[{
                        "id":"starting_deck","displayNames":{"eng":"Starting deck"},
                        "kind":"pinned","allowedItemTypes":["card"],
                        "minItems":1,"maxItems":4,"minQuantity":1,"maxQuantity":10
                    }],
                    "resourceProfileField":"visual_profile",
                    "resourceProfiles":[
                        {"id":"placeholder","displayNames":{"eng":"Placeholder"},"requiredResourceRoles":[]},
                        {"id":"branded_placeholder","displayNames":{"eng":"Branded"},"requiredResourceRoles":["character.select_icon"]}
                    ],
                    "evidenceQueries":[{"symbols":["CharacterModel"],"terms":[]}]
                },
                {"id":"card","displayNames":{"eng":"Card"},"evidenceQueries":[{"symbols":["CardModel"],"terms":[]}]}
            ],
            "compositionProfiles":[{
                "id":"character_suite","displayNames":{"eng":"Character suite"},
                "rootItemType":"character","defaultProfile":"standard","customBaseProfile":"standard",
                "maxNodes":128,"baseNodeCount":1,
                "parameters":[{"id":"starter_card_types","displayNames":{"eng":"Starter cards"},"min":1,"max":16,"nodeWeight":1}],
                "profiles":[{"id":"standard","displayNames":{"eng":"Standard"},"values":{"starter_card_types":4}}],
                "constraints":[]
            }],
            "contributions":[]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        let hash = Sha256Digest::parse(format!("{:x}", Sha256::digest(&bytes))).unwrap();
        let pack = GamePackLoader::load(&bytes, &hash).unwrap();
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture-character").unwrap(),
            ItemTypeId::parse("character").unwrap(),
        );
        definition.canonical_fields.insert(
            ItemFieldId::parse("visual_profile").unwrap(),
            ItemFieldValue::Choice("branded_placeholder".into()),
        );
        definition.localizations.insert(
            LocaleId::parse("eng").unwrap(),
            ItemLocalization {
                fields: BTreeMap::from([(
                    LocalizationFieldId::parse("title").unwrap(),
                    "Fixture".into(),
                )]),
                status: LocalizationStatus::Confirmed,
                translated_from: None,
            },
        );
        definition.behavior_intent = vec!["Provide a playable character.".into()];
        definition.resource_bindings.insert(
            ResourceId::parse("character.select_icon").unwrap(),
            ItemResourceBinding {
                resource_id: ResourceId::parse("resource.character-select-icon").unwrap(),
                selected_version: Sha256Digest::parse("a".repeat(64)).unwrap(),
            },
        );
        definition.reference_bindings.insert(
            ItemReferenceSlotId::parse("starting_deck").unwrap(),
            vec![ItemReferenceBinding::Pinned {
                item_id: ItemId::parse("fixture-strike").unwrap(),
                definition_hash: Sha256Digest::parse("b".repeat(64)).unwrap(),
                quantity: 4,
            }],
        );
        definition.composition_profile = Some(ItemCompositionProfile {
            composition_id: CompositionId::parse("character_suite").unwrap(),
            source: ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            },
            parameters: BTreeMap::from([(
                CompositionParameterId::parse("starter_card_types").unwrap(),
                4,
            )]),
        });
        ItemDefinitionValidator::validate(&pack, &definition, ItemDefinitionValidationMode::Ready)
            .unwrap();

        if let ItemReferenceBinding::Pinned { quantity, .. } =
            &mut definition.reference_bindings.values_mut().next().unwrap()[0]
        {
            *quantity = 11;
        }
        assert_eq!(
            ItemDefinitionValidator::validate(
                &pack,
                &definition,
                ItemDefinitionValidationMode::Ready,
            ),
            Err(ItemDefinitionValidationError::InvalidReferenceBinding)
        );
    }
}
