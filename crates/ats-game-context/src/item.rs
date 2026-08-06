use std::collections::{BTreeMap, BTreeSet};

use ats_kernel::{
    CompositionId, CompositionParameterId, CompositionProfileId, GamePackId, ItemFieldId,
    ItemReferenceSlotId, ItemTypeId, LocaleId, LocalizationFieldId, ResourceId, ResourceProfileId,
    Sha256Digest,
};
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
    #[serde(default)]
    localization_fields: Vec<LocalizationFieldSpec>,
    #[serde(default)]
    reference_slots: Vec<ItemReferenceSlotSpec>,
    #[serde(default)]
    resource_profile_field: Option<ItemFieldId>,
    #[serde(default)]
    resource_profiles: Vec<ItemResourceProfileSpec>,
    evidence_queries: Vec<ItemEvidenceQuery>,
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
    pub fn localization_fields(&self) -> &[LocalizationFieldSpec] {
        &self.localization_fields
    }

    #[must_use]
    pub fn reference_slots(&self) -> &[ItemReferenceSlotSpec] {
        &self.reference_slots
    }

    #[must_use]
    pub fn resource_profile_field(&self) -> Option<&ItemFieldId> {
        self.resource_profile_field.as_ref()
    }

    #[must_use]
    pub fn resource_profiles(&self) -> &[ItemResourceProfileSpec] {
        &self.resource_profiles
    }

    #[must_use]
    pub fn evidence_queries(&self) -> &[ItemEvidenceQuery] {
        &self.evidence_queries
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
        if self.localization_fields.len() > 64
            || has_duplicates_by(&self.localization_fields, LocalizationFieldSpec::id)
            || self
                .localization_fields
                .iter()
                .any(|field| field.validate().is_err())
            || (!self.required_locales.is_empty() && self.localization_fields.is_empty())
        {
            return Err(ItemCatalogError::InvalidLocalizationFields);
        }
        if self.reference_slots.len() > 64
            || has_duplicates_by(&self.reference_slots, ItemReferenceSlotSpec::id)
            || self
                .reference_slots
                .iter()
                .any(|slot| slot.validate().is_err())
        {
            return Err(ItemCatalogError::InvalidReferenceSlots);
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
        self.validate_resource_profiles()?;
        Ok(())
    }

    fn validate_resource_profiles(&self) -> Result<(), ItemCatalogError> {
        if self.resource_profiles.len() > 32
            || has_duplicates_by(&self.resource_profiles, ItemResourceProfileSpec::id)
            || self
                .resource_profiles
                .iter()
                .any(|profile| profile.validate().is_err())
        {
            return Err(ItemCatalogError::InvalidResourceProfiles);
        }
        let Some(selector) = &self.resource_profile_field else {
            return if self.resource_profiles.len() <= 1 {
                Ok(())
            } else {
                Err(ItemCatalogError::InvalidResourceProfiles)
            };
        };
        let Some(ItemFieldSpec {
            required: true,
            value: ItemFieldValueSpec::Choice { options },
            ..
        }) = self.fields.iter().find(|field| field.id() == selector)
        else {
            return Err(ItemCatalogError::InvalidResourceProfiles);
        };
        let option_ids = options
            .iter()
            .map(|option| option.value.as_str())
            .collect::<BTreeSet<_>>();
        let profile_ids = self
            .resource_profiles
            .iter()
            .map(|profile| profile.id.as_str())
            .collect::<BTreeSet<_>>();
        if option_ids == profile_ids {
            Ok(())
        } else {
            Err(ItemCatalogError::InvalidResourceProfiles)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalizationFieldSpec {
    id: LocalizationFieldId,
    display_names: BTreeMap<LocaleId, String>,
    required: bool,
    multiline: bool,
    min_length: u32,
    max_length: u32,
}

impl LocalizationFieldSpec {
    #[must_use]
    pub fn id(&self) -> &LocalizationFieldId {
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
    pub const fn multiline(&self) -> bool {
        self.multiline
    }

    #[must_use]
    pub const fn min_length(&self) -> u32 {
        self.min_length
    }

    #[must_use]
    pub const fn max_length(&self) -> u32 {
        self.max_length
    }

    fn validate(&self) -> Result<(), ItemCatalogError> {
        validate_display_names(&self.display_names)?;
        if self.min_length <= self.max_length && self.max_length <= 8_000 {
            Ok(())
        } else {
            Err(ItemCatalogError::InvalidLocalizationFields)
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ItemReferenceKind {
    Identity,
    Pinned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemReferenceSlotSpec {
    id: ItemReferenceSlotId,
    display_names: BTreeMap<LocaleId, String>,
    kind: ItemReferenceKind,
    allowed_item_types: Vec<ItemTypeId>,
    min_items: u32,
    max_items: u32,
    min_quantity: u32,
    max_quantity: u32,
}

impl ItemReferenceSlotSpec {
    #[must_use]
    pub fn id(&self) -> &ItemReferenceSlotId {
        &self.id
    }

    #[must_use]
    pub fn display_names(&self) -> &BTreeMap<LocaleId, String> {
        &self.display_names
    }

    #[must_use]
    pub const fn kind(&self) -> ItemReferenceKind {
        self.kind
    }

    #[must_use]
    pub fn allowed_item_types(&self) -> &[ItemTypeId] {
        &self.allowed_item_types
    }

    #[must_use]
    pub const fn min_items(&self) -> u32 {
        self.min_items
    }

    #[must_use]
    pub const fn max_items(&self) -> u32 {
        self.max_items
    }

    #[must_use]
    pub const fn min_quantity(&self) -> u32 {
        self.min_quantity
    }

    #[must_use]
    pub const fn max_quantity(&self) -> u32 {
        self.max_quantity
    }

    fn validate(&self) -> Result<(), ItemCatalogError> {
        validate_display_names(&self.display_names)?;
        let quantity_valid = self.min_quantity >= 1
            && self.min_quantity <= self.max_quantity
            && self.max_quantity <= 999
            && (self.kind == ItemReferenceKind::Pinned
                || (self.min_quantity == 1 && self.max_quantity == 1));
        if !self.allowed_item_types.is_empty()
            && self.allowed_item_types.len() <= 32
            && !has_duplicates(&self.allowed_item_types)
            && self.min_items <= self.max_items
            && self.max_items <= 128
            && quantity_valid
        {
            Ok(())
        } else {
            Err(ItemCatalogError::InvalidReferenceSlots)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemResourceProfileSpec {
    id: ResourceProfileId,
    display_names: BTreeMap<LocaleId, String>,
    #[serde(default)]
    required_resource_roles: Vec<ResourceId>,
}

impl ItemResourceProfileSpec {
    #[must_use]
    pub fn id(&self) -> &ResourceProfileId {
        &self.id
    }

    #[must_use]
    pub fn display_names(&self) -> &BTreeMap<LocaleId, String> {
        &self.display_names
    }

    #[must_use]
    pub fn required_resource_roles(&self) -> &[ResourceId] {
        &self.required_resource_roles
    }

    fn validate(&self) -> Result<(), ItemCatalogError> {
        validate_display_names(&self.display_names)?;
        if self.required_resource_roles.len() <= 128
            && !has_duplicates(&self.required_resource_roles)
        {
            Ok(())
        } else {
            Err(ItemCatalogError::InvalidResourceProfiles)
        }
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
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionProfileSet {
    id: CompositionId,
    display_names: BTreeMap<LocaleId, String>,
    root_item_type: ItemTypeId,
    default_profile: CompositionProfileId,
    custom_base_profile: CompositionProfileId,
    max_nodes: u32,
    base_node_count: u32,
    parameters: Vec<CompositionParameterSpec>,
    profiles: Vec<CompositionProfileSpec>,
    #[serde(default)]
    constraints: Vec<CompositionConstraintSpec>,
}

impl CompositionProfileSet {
    #[must_use]
    pub fn id(&self) -> &CompositionId {
        &self.id
    }

    #[must_use]
    pub fn display_names(&self) -> &BTreeMap<LocaleId, String> {
        &self.display_names
    }

    #[must_use]
    pub fn root_item_type(&self) -> &ItemTypeId {
        &self.root_item_type
    }

    #[must_use]
    pub fn default_profile(&self) -> &CompositionProfileId {
        &self.default_profile
    }

    #[must_use]
    pub fn custom_base_profile(&self) -> &CompositionProfileId {
        &self.custom_base_profile
    }

    #[must_use]
    pub const fn max_nodes(&self) -> u32 {
        self.max_nodes
    }

    #[must_use]
    pub const fn base_node_count(&self) -> u32 {
        self.base_node_count
    }

    #[must_use]
    pub fn parameters(&self) -> &[CompositionParameterSpec] {
        &self.parameters
    }

    #[must_use]
    pub fn profiles(&self) -> &[CompositionProfileSpec] {
        &self.profiles
    }

    #[must_use]
    pub fn constraints(&self) -> &[CompositionConstraintSpec] {
        &self.constraints
    }

    pub fn validate_parameters(
        &self,
        values: &BTreeMap<CompositionParameterId, u32>,
    ) -> Result<(), CompositionProfileError> {
        if values.len() != self.parameters.len() {
            return Err(CompositionProfileError::InvalidParameters);
        }
        let mut node_count = self.base_node_count;
        for parameter in &self.parameters {
            let Some(value) = values.get(parameter.id()) else {
                return Err(CompositionProfileError::InvalidParameters);
            };
            if !(parameter.min..=parameter.max).contains(value) {
                return Err(CompositionProfileError::InvalidParameters);
            }
            node_count = node_count
                .checked_add(value.saturating_mul(parameter.node_weight))
                .ok_or(CompositionProfileError::InvalidNodeLimit)?;
        }
        if node_count > self.max_nodes {
            return Err(CompositionProfileError::InvalidNodeLimit);
        }
        for constraint in &self.constraints {
            match constraint {
                CompositionConstraintSpec::LessOrEqual { left, right }
                    if values.get(left).is_none_or(|left_value| {
                        values
                            .get(right)
                            .is_none_or(|right_value| left_value > right_value)
                    }) =>
                {
                    return Err(CompositionProfileError::ConstraintViolation);
                }
                CompositionConstraintSpec::LessOrEqual { .. } => {}
            }
        }
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), CompositionProfileError> {
        validate_display_names(&self.display_names)
            .map_err(|_| CompositionProfileError::InvalidDisplayNames)?;
        if self.max_nodes == 0
            || self.max_nodes > 128
            || self.base_node_count == 0
            || self.base_node_count > self.max_nodes
            || self.parameters.is_empty()
            || self.parameters.len() > 64
            || has_duplicates_by(&self.parameters, CompositionParameterSpec::id)
            || self
                .parameters
                .iter()
                .any(|parameter| !parameter.is_valid())
            || self.profiles.is_empty()
            || self.profiles.len() > 16
            || has_duplicates_by(&self.profiles, CompositionProfileSpec::id)
            || self.profiles.iter().any(|profile| !profile.is_valid())
            || self.constraints.len() > 32
        {
            return Err(CompositionProfileError::InvalidContract);
        }
        let parameter_ids = self
            .parameters
            .iter()
            .map(CompositionParameterSpec::id)
            .collect::<BTreeSet<_>>();
        if self.constraints.iter().any(|constraint| match constraint {
            CompositionConstraintSpec::LessOrEqual { left, right } => {
                left == right || !parameter_ids.contains(left) || !parameter_ids.contains(right)
            }
        }) {
            return Err(CompositionProfileError::InvalidContract);
        }
        let profile_ids = self
            .profiles
            .iter()
            .map(CompositionProfileSpec::id)
            .collect::<BTreeSet<_>>();
        if !profile_ids.contains(&self.default_profile)
            || !profile_ids.contains(&self.custom_base_profile)
            || self
                .profiles
                .iter()
                .any(|profile| self.validate_parameters(&profile.values).is_err())
        {
            return Err(CompositionProfileError::InvalidContract);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionParameterSpec {
    id: CompositionParameterId,
    display_names: BTreeMap<LocaleId, String>,
    min: u32,
    max: u32,
    node_weight: u32,
}

impl CompositionParameterSpec {
    #[must_use]
    pub fn id(&self) -> &CompositionParameterId {
        &self.id
    }

    #[must_use]
    pub fn display_names(&self) -> &BTreeMap<LocaleId, String> {
        &self.display_names
    }

    #[must_use]
    pub const fn min(&self) -> u32 {
        self.min
    }

    #[must_use]
    pub const fn max(&self) -> u32 {
        self.max
    }

    #[must_use]
    pub const fn node_weight(&self) -> u32 {
        self.node_weight
    }

    fn is_valid(&self) -> bool {
        self.min <= self.max
            && self.max <= 128
            && self.node_weight <= 128
            && validate_display_names(&self.display_names).is_ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionProfileSpec {
    id: CompositionProfileId,
    display_names: BTreeMap<LocaleId, String>,
    values: BTreeMap<CompositionParameterId, u32>,
}

impl CompositionProfileSpec {
    #[must_use]
    pub fn id(&self) -> &CompositionProfileId {
        &self.id
    }

    #[must_use]
    pub fn display_names(&self) -> &BTreeMap<LocaleId, String> {
        &self.display_names
    }

    #[must_use]
    pub fn values(&self) -> &BTreeMap<CompositionParameterId, u32> {
        &self.values
    }

    fn is_valid(&self) -> bool {
        !self.values.is_empty()
            && self.values.len() <= 64
            && validate_display_names(&self.display_names).is_ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CompositionConstraintSpec {
    LessOrEqual {
        left: CompositionParameterId,
        right: CompositionParameterId,
    },
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CompositionProfileError {
    #[error("composition profile display names are invalid")]
    InvalidDisplayNames,
    #[error("composition profile contract is invalid")]
    InvalidContract,
    #[error("composition profile parameters are invalid")]
    InvalidParameters,
    #[error("composition profile exceeds the node limit")]
    InvalidNodeLimit,
    #[error("composition profile constraint is violated")]
    ConstraintViolation,
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
    #[error("item type localization field descriptors are invalid")]
    InvalidLocalizationFields,
    #[error("item type reference slots are invalid")]
    InvalidReferenceSlots,
    #[error("item type evidence queries are invalid")]
    InvalidEvidenceQueries,
    #[error("item type Resource profiles are invalid")]
    InvalidResourceProfiles,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemCapabilityCatalog {
    pub game_pack_id: GamePackId,
    pub game_pack_sha256: Sha256Digest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truth_snapshot_id: Option<Sha256Digest>,
    pub item_types: Vec<ItemTypeCapability>,
    pub composition_profiles: Vec<CompositionProfileSet>,
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
            composition_profiles: pack.composition_profiles().values().cloned().collect(),
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
            "schemaVersion": 4,
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
                    }, {
                        "id": "name_color",
                        "displayNames": {"eng":"Name color", "zhs":"名称颜色"},
                        "required": true,
                        "value": {
                            "kind": "text",
                            "multiline": false,
                            "minLength": 6,
                            "maxLength": 8
                        }
                    }, {
                        "id": "tags",
                        "displayNames": {"eng":"Tags", "zhs":"标签"},
                        "required": false,
                        "value": {
                            "kind": "string_list",
                            "minItems": 0,
                            "maxItems": 4,
                            "itemMaxLength": 64
                        }
                    }],
                    "localizationFields": [
                        {
                            "id":"name",
                            "displayNames":{"eng":"Name", "zhs":"名称"},
                            "required":true,
                            "multiline":false,
                            "minLength":1,
                            "maxLength":256
                        }
                    ],
                    "evidenceQueries": [{"symbols":["Fixture.Symbol"],"terms":[]}],
                    "resourceProfiles": [{
                        "id":"default",
                        "displayNames":{"eng":"Default"},
                        "requiredResourceRoles":["fixture.icon"]
                    }]
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

        let catalog_wire = serde_json::to_value(&catalog).unwrap();
        let ready_wire = catalog_wire["itemTypes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["descriptor"]["id"] == "ready_item")
            .unwrap();
        let fields = ready_wire["descriptor"]["fields"].as_array().unwrap();
        let text = fields
            .iter()
            .find(|field| field["id"] == "name_color")
            .unwrap();
        assert_eq!(text["value"]["minLength"], 6);
        assert_eq!(text["value"]["maxLength"], 8);
        assert!(text["value"].get("min_length").is_none());
        assert!(text["value"].get("max_length").is_none());
        let string_list = fields.iter().find(|field| field["id"] == "tags").unwrap();
        assert_eq!(string_list["value"]["minItems"], 0);
        assert_eq!(string_list["value"]["maxItems"], 4);
        assert_eq!(string_list["value"]["itemMaxLength"], 64);
        assert!(string_list["value"].get("min_items").is_none());
        assert!(string_list["value"].get("max_items").is_none());
        assert!(string_list["value"].get("item_max_length").is_none());
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
            descriptor.resource_profiles()[0].required_resource_roles()[0].as_str(),
            "fixture.icon"
        );
    }
}
