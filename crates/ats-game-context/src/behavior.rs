use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};
use std::path::{Component, Path};

use ats_kernel::{
    BehaviorAdapterId, BehaviorCapabilityId, CapabilityCatalogId, CapabilityParameterId,
    GamePackId, ItemFieldId, ItemId, ItemReferenceSlotId, ItemTypeId, LocaleId,
    LocalizationFieldId, ResourceId, SchemaVersion, Sha256Digest,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const CAPABILITY_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const BEHAVIOR_PROPOSAL_SCHEMA_VERSION: u32 = 1;
pub const RENDERED_ITEM_BUNDLE_SCHEMA_VERSION: u32 = 1;

const MAX_CAPABILITIES: usize = 256;
const MAX_PARAMETERS: usize = 32;
const MAX_INVOCATIONS: usize = 128;
const MAX_RENDERED_FILES: usize = 64;
const MAX_RENDERED_FILE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RenderedFileMerge {
    JsonObject,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RenderedFileMergeKeyPolicy {
    UniqueKeys,
    ExclusivePath,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorAdapterIdentity {
    pub id: BehaviorAdapterId,
    pub version: SchemaVersion,
    pub implementation_sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalogIdentity {
    pub id: CapabilityCatalogId,
    pub version: SchemaVersion,
    pub sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CapabilityParameterType {
    Text {
        min_length: u32,
        max_length: u32,
        #[serde(default)]
        multiline: bool,
    },
    Integer {
        min: i64,
        max: i64,
    },
    Boolean,
    Choice {
        options: Vec<String>,
    },
    ItemReference {
        allowed_item_types: Vec<ItemTypeId>,
    },
    ResourceReference,
    TextList {
        min_items: u32,
        max_items: u32,
        item_max_length: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityParameterSpec {
    pub id: CapabilityParameterId,
    pub description: String,
    pub required: bool,
    pub value: CapabilityParameterType,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilitySpec {
    pub id: BehaviorCapabilityId,
    pub description: String,
    pub max_invocations_per_item: u32,
    #[serde(default)]
    pub parameters: Vec<CapabilityParameterSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorCapabilitySet {
    pub item_type: ItemTypeId,
    pub allowed_capabilities: Vec<BehaviorCapabilityId>,
    pub min_invocations: u32,
    pub max_invocations: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityCatalog {
    pub schema_version: u32,
    pub id: CapabilityCatalogId,
    pub version: SchemaVersion,
    pub adapter: BehaviorAdapterIdentity,
    pub capabilities: Vec<CapabilitySpec>,
    pub item_types: Vec<BehaviorCapabilitySet>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackBehaviorContract {
    pub adapter: BehaviorAdapterIdentity,
    pub catalog: CapabilityCatalog,
}

impl PackBehaviorContract {
    pub fn validate(&self) -> Result<(), BehaviorAdapterError> {
        self.catalog.validate()?;
        if self.adapter != self.catalog.adapter {
            return Err(BehaviorAdapterError::InvalidCatalog);
        }
        Ok(())
    }

    pub fn catalog_identity(&self) -> Result<CapabilityCatalogIdentity, BehaviorAdapterError> {
        self.validate()?;
        self.catalog.identity()
    }
}

impl CapabilityCatalog {
    pub fn validate(&self) -> Result<(), BehaviorAdapterError> {
        if self.schema_version != CAPABILITY_CATALOG_SCHEMA_VERSION
            || self.capabilities.len() > MAX_CAPABILITIES
            || self.item_types.is_empty()
            || self.item_types.len() > 64
        {
            return Err(BehaviorAdapterError::InvalidCatalog);
        }

        let mut capability_ids = BTreeSet::new();
        for capability in &self.capabilities {
            if !capability_ids.insert(&capability.id)
                || !valid_text(&capability.description, 2_000)
                || capability.max_invocations_per_item == 0
                || capability.max_invocations_per_item as usize > MAX_INVOCATIONS
                || capability.parameters.len() > MAX_PARAMETERS
            {
                return Err(BehaviorAdapterError::InvalidCatalog);
            }
            let mut parameter_ids = BTreeSet::new();
            for parameter in &capability.parameters {
                if !parameter_ids.insert(&parameter.id)
                    || !valid_text(&parameter.description, 1_000)
                    || !valid_parameter_type(&parameter.value)
                {
                    return Err(BehaviorAdapterError::InvalidCatalog);
                }
            }
        }

        let mut item_types = BTreeSet::new();
        for item in &self.item_types {
            let allowed = item.allowed_capabilities.iter().collect::<BTreeSet<_>>();
            if !item_types.insert(&item.item_type)
                || allowed.len() != item.allowed_capabilities.len()
                || allowed.iter().any(|id| !capability_ids.contains(*id))
                || item.min_invocations > item.max_invocations
                || item.max_invocations as usize > MAX_INVOCATIONS
                || (item.max_invocations == 0 && !item.allowed_capabilities.is_empty())
            {
                return Err(BehaviorAdapterError::InvalidCatalog);
            }
        }
        Ok(())
    }

    pub fn identity(&self) -> Result<CapabilityCatalogIdentity, BehaviorAdapterError> {
        self.validate()?;
        Ok(CapabilityCatalogIdentity {
            id: self.id.clone(),
            version: self.version,
            sha256: hash_serialized(self)?,
        })
    }

    #[must_use]
    pub fn item_capabilities(&self, item_type: &ItemTypeId) -> Option<&BehaviorCapabilitySet> {
        self.item_types
            .iter()
            .find(|entry| &entry.item_type == item_type)
    }

    #[must_use]
    pub fn capability(&self, id: &BehaviorCapabilityId) -> Option<&CapabilitySpec> {
        self.capabilities.iter().find(|entry| &entry.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum CapabilityValue {
    Text(String),
    Integer(i64),
    Boolean(bool),
    Choice(String),
    ItemReference(ItemId),
    ResourceReference(ResourceId),
    TextList(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityInvocation {
    pub capability_id: BehaviorCapabilityId,
    #[serde(default)]
    pub arguments: BTreeMap<CapabilityParameterId, CapabilityValue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BehaviorIssueCode {
    CatalogMismatch,
    AdapterMismatch,
    ItemTypeUnsupported,
    InvocationCount,
    CapabilityUnsupported,
    ArgumentMissing,
    ArgumentUnexpected,
    ArgumentInvalid,
    ReferenceInvalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorIssue {
    pub code: BehaviorIssueCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_id: Option<BehaviorCapabilityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter_id: Option<CapabilityParameterId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorProposal {
    pub schema_version: u32,
    pub item_id: ItemId,
    pub item_type: ItemTypeId,
    pub definition_hash: Sha256Digest,
    pub catalog: CapabilityCatalogIdentity,
    pub adapter: BehaviorAdapterIdentity,
    pub invocations: Vec<CapabilityInvocation>,
}

impl BehaviorProposal {
    pub fn validate(&self, catalog: &CapabilityCatalog) -> Result<(), Vec<BehaviorIssue>> {
        let mut issues = Vec::new();
        let catalog_identity = catalog.identity().map_err(|_| {
            vec![BehaviorIssue {
                code: BehaviorIssueCode::CatalogMismatch,
                capability_id: None,
                parameter_id: None,
            }]
        })?;
        if self.schema_version != BEHAVIOR_PROPOSAL_SCHEMA_VERSION
            || self.catalog != catalog_identity
        {
            issues.push(issue(BehaviorIssueCode::CatalogMismatch, None, None));
        }
        if self.adapter != catalog.adapter {
            issues.push(issue(BehaviorIssueCode::AdapterMismatch, None, None));
        }
        let Some(item_set) = catalog.item_capabilities(&self.item_type) else {
            issues.push(issue(BehaviorIssueCode::ItemTypeUnsupported, None, None));
            return Err(issues);
        };
        if self.invocations.len() < item_set.min_invocations as usize
            || self.invocations.len() > item_set.max_invocations as usize
        {
            issues.push(issue(BehaviorIssueCode::InvocationCount, None, None));
        }
        let allowed = item_set
            .allowed_capabilities
            .iter()
            .collect::<BTreeSet<_>>();
        let mut invocation_counts = BTreeMap::<&BehaviorCapabilityId, u32>::new();
        for invocation in &self.invocations {
            if !allowed.contains(&invocation.capability_id) {
                issues.push(issue(
                    BehaviorIssueCode::CapabilityUnsupported,
                    Some(invocation.capability_id.clone()),
                    None,
                ));
                continue;
            }
            let Some(spec) = catalog.capability(&invocation.capability_id) else {
                issues.push(issue(
                    BehaviorIssueCode::CapabilityUnsupported,
                    Some(invocation.capability_id.clone()),
                    None,
                ));
                continue;
            };
            let invocation_count = invocation_counts
                .entry(&invocation.capability_id)
                .or_default();
            *invocation_count += 1;
            if *invocation_count > spec.max_invocations_per_item {
                issues.push(issue(
                    BehaviorIssueCode::InvocationCount,
                    Some(invocation.capability_id.clone()),
                    None,
                ));
                continue;
            }
            validate_arguments(invocation, spec, &mut issues);
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }

    pub fn sha256(&self) -> Result<Sha256Digest, BehaviorAdapterError> {
        hash_serialized(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorItemReference {
    pub item_id: ItemId,
    pub item_type: ItemTypeId,
    pub definition_hash: Sha256Digest,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorResourceBinding {
    pub resource_id: ResourceId,
    pub selected_version: Sha256Digest,
    #[serde(default)]
    pub published_paths: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorItemContext {
    pub item_id: ItemId,
    pub item_type: ItemTypeId,
    pub definition_hash: Sha256Digest,
    pub canonical_fields: BTreeMap<ItemFieldId, CapabilityValue>,
    pub localizations: BTreeMap<LocaleId, BTreeMap<LocalizationFieldId, String>>,
    pub references: BTreeMap<ItemReferenceSlotId, Vec<BehaviorItemReference>>,
    pub resources: BTreeMap<ResourceId, BehaviorResourceBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BehaviorRenderContext {
    pub mod_id: String,
    pub game_pack_id: GamePackId,
    pub game_pack_sha256: Sha256Digest,
    pub truth_snapshot_id: Sha256Digest,
    pub catalog: CapabilityCatalogIdentity,
    pub adapter: BehaviorAdapterIdentity,
    pub item: BehaviorItemContext,
}

impl BehaviorRenderContext {
    fn validate_for(&self, proposal: &BehaviorProposal) -> Result<(), BehaviorAdapterError> {
        if !valid_path_segment(&self.mod_id)
            || self.catalog != proposal.catalog
            || self.adapter != proposal.adapter
            || self.item.item_id != proposal.item_id
            || self.item.item_type != proposal.item_type
            || self.item.definition_hash != proposal.definition_hash
            || self
                .item
                .references
                .values()
                .flatten()
                .any(|reference| reference.quantity == 0 || reference.item_id == self.item.item_id)
            || self.item.resources.iter().any(|(id, binding)| {
                id != &binding.resource_id
                    || binding
                        .published_paths
                        .iter()
                        .any(|(role, path)| !valid_role(role) || !valid_relative_path(path))
            })
        {
            return Err(BehaviorAdapterError::InvalidContext);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderedFile {
    pub role: String,
    pub relative_path: String,
    pub bytes: Vec<u8>,
    pub sha256: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_merge: Option<RenderedFileMerge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_merge_key_policy: Option<RenderedFileMergeKeyPolicy>,
}

impl RenderedFile {
    pub fn new(
        role: impl Into<String>,
        relative_path: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self, BehaviorAdapterError> {
        let role = role.into();
        let relative_path = relative_path.into();
        if !valid_role(&role)
            || !valid_relative_path(&relative_path)
            || bytes.is_empty()
            || bytes.len() > MAX_RENDERED_FILE_BYTES
        {
            return Err(BehaviorAdapterError::InvalidOutput);
        }
        let sha256 = hash_bytes(&bytes)?;
        Ok(Self {
            role,
            relative_path,
            bytes,
            sha256,
            composition_merge: None,
            composition_merge_key_policy: None,
        })
    }

    #[must_use]
    pub fn with_composition_merge(
        mut self,
        merge: RenderedFileMerge,
        key_policy: RenderedFileMergeKeyPolicy,
    ) -> Self {
        self.composition_merge = Some(merge);
        self.composition_merge_key_policy = Some(key_policy);
        self
    }

    fn validate(&self) -> Result<(), BehaviorAdapterError> {
        if !valid_role(&self.role)
            || !valid_relative_path(&self.relative_path)
            || self.bytes.is_empty()
            || self.bytes.len() > MAX_RENDERED_FILE_BYTES
            || hash_bytes(&self.bytes)? != self.sha256
            || self.composition_merge.is_some() != self.composition_merge_key_policy.is_some()
        {
            return Err(BehaviorAdapterError::InvalidOutput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenderedItemBundle {
    pub schema_version: u32,
    pub item_id: ItemId,
    pub definition_hash: Sha256Digest,
    pub behavior_sha256: Sha256Digest,
    pub adapter: BehaviorAdapterIdentity,
    pub files: Vec<RenderedFile>,
}

impl RenderedItemBundle {
    pub fn new(
        proposal: &BehaviorProposal,
        files: Vec<RenderedFile>,
    ) -> Result<Self, BehaviorAdapterError> {
        let bundle = Self {
            schema_version: RENDERED_ITEM_BUNDLE_SCHEMA_VERSION,
            item_id: proposal.item_id.clone(),
            definition_hash: proposal.definition_hash.clone(),
            behavior_sha256: proposal.sha256()?,
            adapter: proposal.adapter.clone(),
            files,
        };
        bundle.validate_for(proposal)?;
        Ok(bundle)
    }

    pub fn validate_for(&self, proposal: &BehaviorProposal) -> Result<(), BehaviorAdapterError> {
        if self.schema_version != RENDERED_ITEM_BUNDLE_SCHEMA_VERSION
            || self.item_id != proposal.item_id
            || self.definition_hash != proposal.definition_hash
            || self.behavior_sha256 != proposal.sha256()?
            || self.adapter != proposal.adapter
            || self.files.is_empty()
            || self.files.len() > MAX_RENDERED_FILES
        {
            return Err(BehaviorAdapterError::InvalidOutput);
        }
        let mut roles = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for file in &self.files {
            file.validate()?;
            if !roles.insert(&file.role) || !paths.insert(&file.relative_path) {
                return Err(BehaviorAdapterError::InvalidOutput);
            }
        }
        Ok(())
    }
}

pub trait GameBehaviorAdapter: Send + Sync {
    fn identity(&self) -> &BehaviorAdapterIdentity;

    fn validate_ir(
        &self,
        context: &BehaviorRenderContext,
        proposal: &BehaviorProposal,
    ) -> Result<(), BehaviorAdapterError>;

    fn render(
        &self,
        context: &BehaviorRenderContext,
        proposal: &BehaviorProposal,
    ) -> Result<RenderedItemBundle, BehaviorAdapterError>;
}

#[derive(Default)]
pub struct BehaviorAdapterRegistry {
    adapters: BTreeMap<BehaviorAdapterIdentity, Box<dyn GameBehaviorAdapter>>,
}

impl BehaviorAdapterRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        adapter: impl GameBehaviorAdapter + 'static,
    ) -> Result<(), BehaviorAdapterRegistryError> {
        let identity = adapter.identity().clone();
        match self.adapters.entry(identity) {
            Entry::Vacant(entry) => {
                entry.insert(Box::new(adapter));
                Ok(())
            }
            Entry::Occupied(_) => Err(BehaviorAdapterRegistryError::Duplicate),
        }
    }

    pub fn resolve_exact(
        &self,
        expected: &BehaviorAdapterIdentity,
    ) -> Result<&dyn GameBehaviorAdapter, BehaviorAdapterRegistryError> {
        self.adapters
            .get(expected)
            .map(Box::as_ref)
            .ok_or(BehaviorAdapterRegistryError::Unavailable)
    }

    pub fn render(
        &self,
        expected: &BehaviorAdapterIdentity,
        catalog: &CapabilityCatalog,
        context: &BehaviorRenderContext,
        proposal: &BehaviorProposal,
    ) -> Result<RenderedItemBundle, BehaviorAdapterRegistryError> {
        if &catalog.adapter != expected
            || &context.adapter != expected
            || &proposal.adapter != expected
        {
            return Err(BehaviorAdapterRegistryError::IdentityMismatch);
        }
        proposal
            .validate(catalog)
            .map_err(|_| BehaviorAdapterRegistryError::InvalidIr)?;
        context
            .validate_for(proposal)
            .map_err(BehaviorAdapterRegistryError::Adapter)?;
        let adapter = self.resolve_exact(expected)?;
        adapter
            .validate_ir(context, proposal)
            .map_err(BehaviorAdapterRegistryError::Adapter)?;
        let bundle = adapter
            .render(context, proposal)
            .map_err(BehaviorAdapterRegistryError::Adapter)?;
        bundle
            .validate_for(proposal)
            .map_err(BehaviorAdapterRegistryError::Adapter)?;
        Ok(bundle)
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BehaviorAdapterError {
    #[error("capability catalog is invalid")]
    InvalidCatalog,
    #[error("behavior IR is invalid")]
    InvalidIr,
    #[error("behavior render context is invalid")]
    InvalidContext,
    #[error("behavior adapter does not support the requested capability")]
    UnsupportedCapability,
    #[error("behavior adapter output is invalid")]
    InvalidOutput,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum BehaviorAdapterRegistryError {
    #[error("behavior adapter identity is already registered")]
    Duplicate,
    #[error("behavior adapter identity is not registered")]
    Unavailable,
    #[error("behavior adapter identity does not match the pinned context")]
    IdentityMismatch,
    #[error("behavior IR failed local validation")]
    InvalidIr,
    #[error("behavior adapter failed")]
    Adapter(#[source] BehaviorAdapterError),
}

fn validate_arguments(
    invocation: &CapabilityInvocation,
    spec: &CapabilitySpec,
    issues: &mut Vec<BehaviorIssue>,
) {
    let parameters = spec
        .parameters
        .iter()
        .map(|parameter| (&parameter.id, parameter))
        .collect::<BTreeMap<_, _>>();
    for parameter in &spec.parameters {
        match invocation.arguments.get(&parameter.id) {
            None if parameter.required => issues.push(issue(
                BehaviorIssueCode::ArgumentMissing,
                Some(invocation.capability_id.clone()),
                Some(parameter.id.clone()),
            )),
            Some(value) if !value_matches(&parameter.value, value) => issues.push(issue(
                BehaviorIssueCode::ArgumentInvalid,
                Some(invocation.capability_id.clone()),
                Some(parameter.id.clone()),
            )),
            _ => {}
        }
    }
    for parameter_id in invocation.arguments.keys() {
        if !parameters.contains_key(parameter_id) {
            issues.push(issue(
                BehaviorIssueCode::ArgumentUnexpected,
                Some(invocation.capability_id.clone()),
                Some(parameter_id.clone()),
            ));
        }
    }
}

fn issue(
    code: BehaviorIssueCode,
    capability_id: Option<BehaviorCapabilityId>,
    parameter_id: Option<CapabilityParameterId>,
) -> BehaviorIssue {
    BehaviorIssue {
        code,
        capability_id,
        parameter_id,
    }
}

fn valid_parameter_type(value: &CapabilityParameterType) -> bool {
    match value {
        CapabilityParameterType::Text {
            min_length,
            max_length,
            ..
        } => min_length <= max_length && *max_length <= 8_000,
        CapabilityParameterType::Integer { min, max } => min <= max,
        CapabilityParameterType::Boolean | CapabilityParameterType::ResourceReference => true,
        CapabilityParameterType::Choice { options } => {
            !options.is_empty()
                && options.len() <= 128
                && options.iter().all(|option| valid_text(option, 128))
                && options.iter().collect::<BTreeSet<_>>().len() == options.len()
        }
        CapabilityParameterType::ItemReference { allowed_item_types } => {
            !allowed_item_types.is_empty()
                && allowed_item_types.len() <= 64
                && allowed_item_types.iter().collect::<BTreeSet<_>>().len()
                    == allowed_item_types.len()
        }
        CapabilityParameterType::TextList {
            min_items,
            max_items,
            item_max_length,
        } => min_items <= max_items && *max_items <= 128 && (1..=2_000).contains(item_max_length),
    }
}

fn value_matches(spec: &CapabilityParameterType, value: &CapabilityValue) -> bool {
    match (spec, value) {
        (
            CapabilityParameterType::Text {
                min_length,
                max_length,
                multiline,
            },
            CapabilityValue::Text(value),
        ) => {
            let length = value.chars().count();
            length >= *min_length as usize
                && length <= *max_length as usize
                && !value.contains('\0')
                && (*multiline || (!value.contains('\r') && !value.contains('\n')))
        }
        (CapabilityParameterType::Integer { min, max }, CapabilityValue::Integer(value)) => {
            value >= min && value <= max
        }
        (CapabilityParameterType::Boolean, CapabilityValue::Boolean(_))
        | (CapabilityParameterType::ResourceReference, CapabilityValue::ResourceReference(_)) => {
            true
        }
        (CapabilityParameterType::Choice { options }, CapabilityValue::Choice(value)) => {
            options.contains(value)
        }
        (CapabilityParameterType::ItemReference { .. }, CapabilityValue::ItemReference(_)) => true,
        (
            CapabilityParameterType::TextList {
                min_items,
                max_items,
                item_max_length,
            },
            CapabilityValue::TextList(values),
        ) => {
            values.len() >= *min_items as usize
                && values.len() <= *max_items as usize
                && values
                    .iter()
                    .all(|value| valid_text(value, *item_max_length as usize))
        }
        _ => false,
    }
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !value.contains('\0') && value.chars().count() <= max_chars
}

fn valid_role(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn valid_relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value.contains('\\')
        && !value.starts_with(".ats/")
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn valid_path_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn hash_serialized<T: Serialize>(value: &T) -> Result<Sha256Digest, BehaviorAdapterError> {
    let bytes = serde_json::to_vec(value).map_err(|_| BehaviorAdapterError::InvalidCatalog)?;
    hash_bytes(&bytes)
}

fn hash_bytes(bytes: &[u8]) -> Result<Sha256Digest, BehaviorAdapterError> {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .map_err(|_| BehaviorAdapterError::InvalidOutput)
}

#[cfg(test)]
pub(crate) fn fixture_behavior_json(item_type: &str) -> serde_json::Value {
    serde_json::json!({
        "adapter": {
            "id": "game.fixture.behavior",
            "version": 1,
            "implementationSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
        "catalog": {
            "schemaVersion": 1,
            "id": "game.fixture.capabilities",
            "version": 1,
            "adapter": {
                "id": "game.fixture.behavior",
                "version": 1,
                "implementationSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            "capabilities": [{
                "id": "fixture.noop",
                "description": "Fixture no-op capability.",
                "maxInvocationsPerItem": 1,
                "parameters": []
            }],
            "itemTypes": [{
                "itemType": item_type,
                "allowedCapabilities": ["fixture.noop"],
                "minInvocations": 0,
                "maxInvocations": 1
            }]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: char) -> Sha256Digest {
        Sha256Digest::parse(byte.to_string().repeat(64)).unwrap()
    }

    fn adapter_identity() -> BehaviorAdapterIdentity {
        BehaviorAdapterIdentity {
            id: BehaviorAdapterId::parse("game.fixture.behavior").unwrap(),
            version: SchemaVersion::new(1).unwrap(),
            implementation_sha256: digest('a'),
        }
    }

    fn catalog() -> CapabilityCatalog {
        CapabilityCatalog {
            schema_version: CAPABILITY_CATALOG_SCHEMA_VERSION,
            id: CapabilityCatalogId::parse("game.fixture.capabilities").unwrap(),
            version: SchemaVersion::new(1).unwrap(),
            adapter: adapter_identity(),
            capabilities: vec![CapabilitySpec {
                id: BehaviorCapabilityId::parse("card.deal_damage").unwrap(),
                description: "Deal damage to a target.".into(),
                max_invocations_per_item: 1,
                parameters: vec![CapabilityParameterSpec {
                    id: CapabilityParameterId::parse("amount").unwrap(),
                    description: "Damage amount.".into(),
                    required: true,
                    value: CapabilityParameterType::Integer { min: 1, max: 99 },
                }],
            }],
            item_types: vec![BehaviorCapabilitySet {
                item_type: ItemTypeId::parse("card").unwrap(),
                allowed_capabilities: vec![
                    BehaviorCapabilityId::parse("card.deal_damage").unwrap(),
                ],
                min_invocations: 1,
                max_invocations: 4,
            }],
        }
    }

    fn proposal(catalog: &CapabilityCatalog) -> BehaviorProposal {
        BehaviorProposal {
            schema_version: BEHAVIOR_PROPOSAL_SCHEMA_VERSION,
            item_id: ItemId::parse("fixture-card").unwrap(),
            item_type: ItemTypeId::parse("card").unwrap(),
            definition_hash: digest('b'),
            catalog: catalog.identity().unwrap(),
            adapter: catalog.adapter.clone(),
            invocations: vec![CapabilityInvocation {
                capability_id: BehaviorCapabilityId::parse("card.deal_damage").unwrap(),
                arguments: BTreeMap::from([(
                    CapabilityParameterId::parse("amount").unwrap(),
                    CapabilityValue::Integer(7),
                )]),
            }],
        }
    }

    fn context(proposal: &BehaviorProposal) -> BehaviorRenderContext {
        BehaviorRenderContext {
            mod_id: "FixtureMod".into(),
            game_pack_id: GamePackId::parse("fixture-game").unwrap(),
            game_pack_sha256: digest('c'),
            truth_snapshot_id: digest('d'),
            catalog: proposal.catalog.clone(),
            adapter: proposal.adapter.clone(),
            item: BehaviorItemContext {
                item_id: proposal.item_id.clone(),
                item_type: proposal.item_type.clone(),
                definition_hash: proposal.definition_hash.clone(),
                canonical_fields: BTreeMap::new(),
                localizations: BTreeMap::new(),
                references: BTreeMap::new(),
                resources: BTreeMap::new(),
            },
        }
    }

    #[test]
    fn strict_behavior_ir_rejects_native_authoring_fields() {
        let catalog = catalog();
        let proposal = proposal(&catalog);
        let mut value = serde_json::to_value(proposal).unwrap();
        value["source"] = serde_json::json!("public class Guess {}");
        assert!(serde_json::from_value::<BehaviorProposal>(value).is_err());
    }

    #[test]
    fn catalog_and_proposal_enforce_capability_arguments() {
        let catalog = catalog();
        catalog.validate().unwrap();
        let mut proposal = proposal(&catalog);
        proposal.validate(&catalog).unwrap();
        proposal.invocations[0].arguments.insert(
            CapabilityParameterId::parse("amount").unwrap(),
            CapabilityValue::Integer(100),
        );
        assert_eq!(
            proposal.validate(&catalog).unwrap_err()[0].code,
            BehaviorIssueCode::ArgumentInvalid
        );
    }

    struct FixtureAdapter {
        identity: BehaviorAdapterIdentity,
    }

    impl GameBehaviorAdapter for FixtureAdapter {
        fn identity(&self) -> &BehaviorAdapterIdentity {
            &self.identity
        }

        fn validate_ir(
            &self,
            _context: &BehaviorRenderContext,
            _proposal: &BehaviorProposal,
        ) -> Result<(), BehaviorAdapterError> {
            Ok(())
        }

        fn render(
            &self,
            _context: &BehaviorRenderContext,
            proposal: &BehaviorProposal,
        ) -> Result<RenderedItemBundle, BehaviorAdapterError> {
            RenderedItemBundle::new(
                proposal,
                vec![RenderedFile::new(
                    "source",
                    "Generated/FixtureCard.cs",
                    b"sealed class FixtureCard {}".to_vec(),
                )?],
            )
        }
    }

    #[test]
    fn registry_requires_exact_identity_and_revalidates_output() {
        let catalog = catalog();
        let proposal = proposal(&catalog);
        let context = context(&proposal);
        let mut registry = BehaviorAdapterRegistry::new();
        registry
            .register(FixtureAdapter {
                identity: adapter_identity(),
            })
            .unwrap();
        let first = registry
            .render(&catalog.adapter, &catalog, &context, &proposal)
            .unwrap();
        let second = registry
            .render(&catalog.adapter, &catalog, &context, &proposal)
            .unwrap();
        assert_eq!(first, second);

        let mut wrong = catalog.adapter.clone();
        wrong.implementation_sha256 = digest('e');
        assert!(matches!(
            registry.resolve_exact(&wrong),
            Err(BehaviorAdapterRegistryError::Unavailable)
        ));
        assert_eq!(
            registry
                .render(&wrong, &catalog, &context, &proposal)
                .unwrap_err(),
            BehaviorAdapterRegistryError::IdentityMismatch
        );
    }

    #[test]
    fn rendered_bundle_rejects_unsafe_or_tampered_files() {
        assert_eq!(
            RenderedFile::new("source", "../escape.cs", b"x".to_vec()).unwrap_err(),
            BehaviorAdapterError::InvalidOutput
        );
        let catalog = catalog();
        let proposal = proposal(&catalog);
        let mut bundle = RenderedItemBundle::new(
            &proposal,
            vec![RenderedFile::new("source", "Generated/X.cs", b"x".to_vec()).unwrap()],
        )
        .unwrap();
        bundle.files[0].bytes = b"changed".to_vec();
        assert_eq!(
            bundle.validate_for(&proposal).unwrap_err(),
            BehaviorAdapterError::InvalidOutput
        );
    }
}
