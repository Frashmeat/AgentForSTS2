use std::collections::{BTreeMap, BTreeSet};

use ats_game_context::{
    CompositionProfileSet, ContributionResolverError, EvidenceQueryError, ItemReferenceKind,
    LoadedGamePack, TruthEvidenceRecord, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{
    CompositionDraftId, CompositionId, ContributionId, FailureCode, FeatureId, ItemFieldId, ItemId,
    ItemReferenceSlotId, ItemTypeId, LocaleId, LocalizationFieldId, RecipeId, ResourceId, SchemaId,
    SchemaRef, SchemaVersion, Sha256Digest,
};
use ats_runtime::{
    CancellationToken, FinishReason, ModelClient, ModelError, ModelGamePackRef, ModelRequestError,
    ModelRequestSnapshot, RunFailure, TokenUsage,
};
use ats_workspace::{
    CompositionDraft, CompositionDraftNode, CompositionDraftRepository,
    CompositionDraftRepositoryErrorKind, ItemCompositionProfile, ItemCompositionSource,
    ItemDefinition, ItemFieldValue, ItemLocalization, ItemReferenceBinding, ItemRepository,
    ItemRepositoryErrorKind, ItemResourceBinding, LocalizationStatus, StoredItemDefinition,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;
use crate::item_definition::{ItemDefinitionValidationMode, ItemDefinitionValidator};
use crate::prompt::{FeatureRecipe, FeatureRecipeError, FeatureRecipeLoader};

const RECIPE_BYTES: &[u8] = include_bytes!("../recipes/composition-plan.json");
const RECIPE_SHA256: &str = "c05cb1527f125f51cdb9a9c106df1d233ba5fb496934012cdc35752d591862b0";
const RETRY_RECIPE_BYTES: &[u8] = include_bytes!("../recipes/composition-retry-node.json");
const RETRY_RECIPE_SHA256: &str =
    "900cff35c5565c6b709d247b53b00601198ead84dbad05ba963677ef3011d667";

pub struct CompositionPlanFeature;

pub struct CompositionRetryNodeFeature;

impl FeatureSpec for CompositionPlanFeature {
    type Request = CompositionPlanRequest;
    type Result = CompositionPlanResult;
    type ArtifactExtension = CompositionPlanArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("composition.plan").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema("feature.composition-plan-request")
    }

    fn result_schema() -> SchemaRef {
        schema("feature.composition-plan-result")
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.composition-plan-artifact-extension")
    }
}

impl CompositionPlanFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: contribution_slot(),
            schema: schema("pack.composition-plan-guidance"),
        }
    }
}

impl FeatureSpec for CompositionRetryNodeFeature {
    type Request = CompositionRetryNodeRequest;
    type Result = CompositionRetryNodeResult;
    type ArtifactExtension = CompositionRetryNodeArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("composition.retry-node").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema("feature.composition-retry-node-request")
    }

    fn result_schema() -> SchemaRef {
        schema("feature.composition-retry-node-result")
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.composition-retry-node-artifact-extension")
    }
}

impl CompositionRetryNodeFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: retry_contribution_slot(),
            schema: schema("pack.composition-retry-node-guidance"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionPlanRequest {
    pub draft_id: CompositionDraftId,
    pub composition_id: CompositionId,
    pub concept: String,
    pub source: ItemCompositionSource,
    pub parameters: BTreeMap<ats_kernel::CompositionParameterId, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionPlanResult {
    pub draft_id: CompositionDraftId,
    pub revision: u64,
    pub root_item_id: ItemId,
    pub node_count: u32,
    pub model_request_sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionPlanArtifactExtension {
    pub model_request_sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionRetryNodeRequest {
    pub draft_id: CompositionDraftId,
    pub expected_revision: u64,
    pub item_id: ItemId,
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionRetryNodeResult {
    pub draft_id: CompositionDraftId,
    pub revision: u64,
    pub item_id: ItemId,
    pub definition_hash: Sha256Digest,
    pub model_request_sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionRetryNodeArtifactExtension {
    pub model_request_sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionPlanContribution {
    compositions: Vec<CompositionPlanGuidance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionPlanGuidance {
    composition_id: CompositionId,
    allowed_item_types: Vec<ItemTypeId>,
    node_type_rules: Vec<CompositionNodeTypeRule>,
    #[serde(default)]
    reference_binding_rules: Vec<CompositionReferenceBindingRule>,
    guidance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionRetryContribution {
    compositions: Vec<CompositionRetryGuidance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionRetryGuidance {
    composition_id: CompositionId,
    guidance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionNodeTypeRule {
    item_type: ItemTypeId,
    base_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parameter_id: Option<ats_kernel::CompositionParameterId>,
    parameter_multiplier: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum ReferenceBindingMeasure {
    Bindings,
    TotalQuantity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionReferenceBindingRule {
    source_item_type: ItemTypeId,
    slot_id: ItemReferenceSlotId,
    measure: ReferenceBindingMeasure,
    base_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parameter_id: Option<ats_kernel::CompositionParameterId>,
    parameter_multiplier: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelCompositionPlan {
    root_item_id: ItemId,
    nodes: Vec<ModelCompositionNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelCompositionNode {
    item_id: ItemId,
    item_type: ItemTypeId,
    canonical_fields: BTreeMap<ItemFieldId, ItemFieldValue>,
    behavior_intent: Vec<String>,
    localizations: BTreeMap<LocaleId, BTreeMap<LocalizationFieldId, String>>,
    reference_bindings: BTreeMap<ItemReferenceSlotId, Vec<PlannedReference>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum PlannedReference {
    Identity {
        item_id: ItemId,
        expected_item_type: ItemTypeId,
    },
    Pinned {
        item_id: ItemId,
        quantity: u32,
    },
}

pub struct CompositionPlanContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
    pub truth: &'a VerifiedTruthSnapshot,
    pub project_context: Option<&'a str>,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
}

pub struct CompositionRetryNodeContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub plan_contributions: &'a VerifiedContributionSet,
    pub retry_contributions: &'a VerifiedContributionSet,
    pub truth: &'a VerifiedTruthSnapshot,
    pub project_context: Option<&'a str>,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompositionPlanExecution {
    pub result: CompositionPlanResult,
    pub draft: CompositionDraft,
    pub request_snapshot: ModelRequestSnapshot,
    pub response_model: String,
    pub usage: TokenUsage,
}

pub struct CompositionPlanService {
    recipe: FeatureRecipe,
}

pub struct CompositionRetryNodeService {
    recipe: FeatureRecipe,
}

impl CompositionPlanService {
    pub fn built_in() -> Result<Self, CompositionPlanError> {
        let expected = Sha256Digest::parse(RECIPE_SHA256)
            .map_err(|_| CompositionPlanError::InvalidRecipeContract)?;
        Self::from_recipe(FeatureRecipeLoader::load(RECIPE_BYTES, &expected)?)
    }

    pub fn from_recipe(recipe: FeatureRecipe) -> Result<Self, CompositionPlanError> {
        if recipe.feature_id() != &CompositionPlanFeature::id()
            || recipe.output_contract().schema != model_output_schema()
        {
            return Err(CompositionPlanError::InvalidRecipeContract);
        }
        Ok(Self { recipe })
    }

    pub async fn execute<C, I, D>(
        &self,
        client: &C,
        items: &I,
        drafts: &D,
        request: CompositionPlanRequest,
        context: CompositionPlanContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<CompositionPlanExecution, CompositionPlanError>
    where
        C: ModelClient + ?Sized,
        I: ItemRepository + ?Sized,
        D: CompositionDraftRepository + ?Sized,
    {
        validate_context(&context)?;
        let profile_set = validate_request(context.pack, &request)?;
        let contribution: CompositionPlanContribution =
            context.contributions.decode(&contribution_slot())?;
        contribution.validate(context.pack)?;
        let guidance = contribution
            .compositions
            .iter()
            .find(|value| value.composition_id == request.composition_id)
            .ok_or(CompositionPlanError::UnsupportedComposition)?;
        let evidence = query_evidence(context.truth, context.pack, &guidance.allowed_item_types)?;
        check_cancelled(cancellation)?;

        let profile = ItemCompositionProfile {
            composition_id: request.composition_id.clone(),
            source: request.source.clone(),
            parameters: request.parameters.clone(),
        };
        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&self.recipe.output_contract().json_schema)?,
            ),
            ("pack.contribution".into(), serialize(guidance)?),
            ("truth.evidence".into(), serialize(&evidence)?),
            (
                "composition.profile".into(),
                serialize(&(profile_set, &profile))?,
            ),
            (
                "project.context".into(),
                bounded_optional(context.project_context, 8_000)?,
            ),
            (
                "runtime.custom_instructions".into(),
                bounded_optional(context.custom_instructions, 4_000)?,
            ),
            ("request.concept".into(), request.concept.clone()),
        ]);
        let model_request = self.recipe.render(&slots, context.model)?;
        let snapshot = ModelRequestSnapshot::new(
            CompositionPlanFeature::id(),
            self.recipe.recipe_ref(),
            ModelGamePackRef {
                id: context.pack.id().clone(),
                sha256: context.pack.content_sha256().clone(),
            },
            Some(context.truth.manifest().snapshot_id().clone()),
            Vec::new(),
            model_request,
        )?;
        let response = client.complete(snapshot.clone(), cancellation).await?;
        check_cancelled(cancellation)?;
        if response.finish_reason == FinishReason::MaxTokens {
            return Err(CompositionPlanError::TruncatedModelOutput);
        }
        let planned: ModelCompositionPlan = serde_json::from_str(&response.content)
            .map_err(|_| CompositionPlanError::InvalidModelOutput)?;
        let definitions = build_definitions(
            context.pack,
            profile_set,
            guidance,
            &profile,
            planned,
            &BTreeMap::new(),
        )?;
        let mut nodes = BTreeMap::new();
        for stored in definitions {
            let expected_current_definition_hash =
                match items.load_current(&stored.definition.item_id) {
                    Ok(current) if current.definition.item_type == stored.definition.item_type => {
                        Some(current.definition_hash)
                    }
                    Ok(_) => return Err(CompositionPlanError::ItemTypeConflict),
                    Err(error) => match I::classify_error(&error) {
                        ItemRepositoryErrorKind::NotFound => None,
                        ItemRepositoryErrorKind::Storage => {
                            return Err(CompositionPlanError::ItemStorage);
                        }
                    },
                };
            nodes.insert(
                stored.definition.item_id.clone(),
                CompositionDraftNode {
                    definition: stored.definition,
                    expected_current_definition_hash,
                },
            );
        }
        let draft = CompositionDraft::new(
            request.draft_id.clone(),
            context.pack.id().clone(),
            context.pack.content_sha256().clone(),
            nodes
                .values()
                .find(|node| node.definition.composition_profile.is_some())
                .map(|node| node.definition.item_id.clone())
                .ok_or(CompositionPlanError::InvalidModelOutput)?,
            profile,
            nodes,
            Utc::now(),
        )
        .map_err(|_| CompositionPlanError::InvalidModelOutput)?;
        drafts
            .create(&draft)
            .map_err(|error| match D::classify_error(&error) {
                CompositionDraftRepositoryErrorKind::Conflict => {
                    CompositionPlanError::DraftConflict
                }
                CompositionDraftRepositoryErrorKind::NotFound
                | CompositionDraftRepositoryErrorKind::Storage => {
                    CompositionPlanError::DraftStorage
                }
            })?;
        let result = CompositionPlanResult {
            draft_id: draft.draft_id.clone(),
            revision: draft.revision,
            root_item_id: draft.root_item_id.clone(),
            node_count: u32::try_from(draft.nodes.len())
                .map_err(|_| CompositionPlanError::InvalidModelOutput)?,
            model_request_sha256: snapshot.request_sha256().clone(),
        };
        Ok(CompositionPlanExecution {
            result,
            draft,
            request_snapshot: snapshot,
            response_model: response.model,
            usage: response.usage,
        })
    }

    #[must_use]
    pub fn recipe_id(&self) -> &RecipeId {
        self.recipe.id()
    }
}

impl CompositionRetryNodeService {
    pub fn built_in() -> Result<Self, CompositionRetryNodeError> {
        let expected = Sha256Digest::parse(RETRY_RECIPE_SHA256)
            .map_err(|_| CompositionRetryNodeError::InvalidRecipeContract)?;
        Self::from_recipe(FeatureRecipeLoader::load(RETRY_RECIPE_BYTES, &expected)?)
    }

    pub fn from_recipe(recipe: FeatureRecipe) -> Result<Self, CompositionRetryNodeError> {
        if recipe.feature_id() != &CompositionRetryNodeFeature::id()
            || recipe.output_contract().schema != retry_model_output_schema()
        {
            return Err(CompositionRetryNodeError::InvalidRecipeContract);
        }
        Ok(Self { recipe })
    }

    pub async fn execute<C, D>(
        &self,
        client: &C,
        drafts: &D,
        request: CompositionRetryNodeRequest,
        context: CompositionRetryNodeContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<CompositionRetryNodeExecution, CompositionRetryNodeError>
    where
        C: ModelClient + ?Sized,
        D: CompositionDraftRepository + ?Sized,
    {
        validate_retry_context(&context)?;
        if request.expected_revision == 0 || !valid_text(&request.instructions, 16_000) {
            return Err(CompositionRetryNodeError::InvalidInput);
        }
        let draft =
            drafts
                .load(&request.draft_id)
                .map_err(|error| match D::classify_error(&error) {
                    CompositionDraftRepositoryErrorKind::NotFound => {
                        CompositionRetryNodeError::DraftNotFound
                    }
                    CompositionDraftRepositoryErrorKind::Conflict => {
                        CompositionRetryNodeError::DraftConflict
                    }
                    CompositionDraftRepositoryErrorKind::Storage => {
                        CompositionRetryNodeError::DraftStorage
                    }
                })?;
        if draft.revision != request.expected_revision {
            return Err(CompositionRetryNodeError::DraftConflict);
        }
        if &draft.game_pack_id != context.pack.id()
            || &draft.game_pack_sha256 != context.pack.content_sha256()
        {
            return Err(CompositionRetryNodeError::ContextIdentityMismatch);
        }
        let target = draft
            .nodes
            .get(&request.item_id)
            .ok_or(CompositionRetryNodeError::TargetNotFound)?;
        let profile_set = context
            .pack
            .composition_profile(&draft.profile.composition_id)
            .ok_or(CompositionRetryNodeError::UnsupportedComposition)?;
        profile_set
            .validate_parameters(&draft.profile.parameters)
            .map_err(|_| CompositionRetryNodeError::InvalidProfile)?;
        let source_valid = match &draft.profile.source {
            ItemCompositionSource::Preset { profile_id } => {
                profile_set.profiles().iter().any(|profile| {
                    profile.id() == profile_id && profile.values() == &draft.profile.parameters
                })
            }
            ItemCompositionSource::Custom { base_profile_id } => profile_set
                .profiles()
                .iter()
                .any(|profile| profile.id() == base_profile_id),
        };
        if !source_valid {
            return Err(CompositionRetryNodeError::InvalidProfile);
        }

        let plan_contribution: CompositionPlanContribution =
            context.plan_contributions.decode(&contribution_slot())?;
        plan_contribution
            .validate(context.pack)
            .map_err(map_retry_plan_error)?;
        let structure = plan_contribution
            .compositions
            .iter()
            .find(|value| value.composition_id == draft.profile.composition_id)
            .ok_or(CompositionRetryNodeError::UnsupportedComposition)?;
        let retry_contribution: CompositionRetryContribution = context
            .retry_contributions
            .decode(&retry_contribution_slot())?;
        retry_contribution.validate(context.pack)?;
        let retry_guidance = retry_contribution
            .compositions
            .iter()
            .find(|value| value.composition_id == draft.profile.composition_id)
            .ok_or(CompositionRetryNodeError::UnsupportedComposition)?;
        let evidence = query_evidence(
            context.truth,
            context.pack,
            std::slice::from_ref(&target.definition.item_type),
        )
        .map_err(map_retry_plan_error)?;
        check_cancelled(cancellation).map_err(map_retry_plan_error)?;

        let logical_nodes = draft
            .nodes
            .values()
            .map(|node| definition_to_model_node(&node.definition))
            .collect::<Vec<_>>();
        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&self.recipe.output_contract().json_schema)
                    .map_err(map_retry_plan_error)?,
            ),
            (
                "pack.contribution".into(),
                serialize(&(structure, retry_guidance)).map_err(map_retry_plan_error)?,
            ),
            (
                "truth.evidence".into(),
                serialize(&evidence).map_err(map_retry_plan_error)?,
            ),
            (
                "draft.context".into(),
                serialize(&serde_json::json!({
                    "draftId": draft.draft_id,
                    "revision": draft.revision,
                    "rootItemId": draft.root_item_id,
                    "profile": draft.profile,
                    "targetItemId": request.item_id,
                    "nodes": logical_nodes,
                }))
                .map_err(map_retry_plan_error)?,
            ),
            (
                "project.context".into(),
                bounded_optional(context.project_context, 8_000).map_err(map_retry_plan_error)?,
            ),
            (
                "runtime.custom_instructions".into(),
                bounded_optional(context.custom_instructions, 4_000)
                    .map_err(map_retry_plan_error)?,
            ),
            ("request.instructions".into(), request.instructions.clone()),
        ]);
        let model_request = self.recipe.render(&slots, context.model)?;
        let snapshot = ModelRequestSnapshot::new(
            CompositionRetryNodeFeature::id(),
            self.recipe.recipe_ref(),
            ModelGamePackRef {
                id: context.pack.id().clone(),
                sha256: context.pack.content_sha256().clone(),
            },
            Some(context.truth.manifest().snapshot_id().clone()),
            Vec::new(),
            model_request,
        )?;
        let response = client.complete(snapshot.clone(), cancellation).await?;
        check_cancelled(cancellation).map_err(map_retry_plan_error)?;
        if response.finish_reason == FinishReason::MaxTokens {
            return Err(CompositionRetryNodeError::TruncatedModelOutput);
        }
        let replacement: ModelCompositionNode = serde_json::from_str(&response.content)
            .map_err(|_| CompositionRetryNodeError::InvalidModelOutput)?;
        if replacement.item_id != request.item_id
            || replacement.item_type != target.definition.item_type
        {
            return Err(CompositionRetryNodeError::TargetIdentityMismatch);
        }

        let mut source = draft
            .nodes
            .values()
            .map(|node| {
                let logical = definition_to_model_node(&node.definition);
                (logical.item_id.clone(), logical)
            })
            .collect::<BTreeMap<_, _>>();
        source.insert(request.item_id.clone(), replacement);
        let resource_bindings = draft
            .nodes
            .iter()
            .map(|(item_id, node)| (item_id.clone(), node.definition.resource_bindings.clone()))
            .collect();
        let definitions = build_definitions(
            context.pack,
            profile_set,
            structure,
            &draft.profile,
            ModelCompositionPlan {
                root_item_id: draft.root_item_id.clone(),
                nodes: source.into_values().collect(),
            },
            &resource_bindings,
        )
        .map_err(map_retry_plan_error)?;
        let mut nodes = BTreeMap::new();
        let mut replacement_hash = None;
        for stored in definitions {
            let item_id = stored.definition.item_id.clone();
            let expected_current_definition_hash = draft.nodes[&item_id]
                .expected_current_definition_hash
                .clone();
            if item_id == request.item_id {
                replacement_hash = Some(stored.definition_hash.clone());
            }
            nodes.insert(
                item_id,
                CompositionDraftNode {
                    definition: stored.definition,
                    expected_current_definition_hash,
                },
            );
        }
        let next = draft
            .revised(nodes, Utc::now())
            .map_err(|_| CompositionRetryNodeError::InvalidModelOutput)?;
        drafts
            .compare_and_set(request.expected_revision, &next)
            .map_err(|error| match D::classify_error(&error) {
                CompositionDraftRepositoryErrorKind::Conflict => {
                    CompositionRetryNodeError::DraftConflict
                }
                CompositionDraftRepositoryErrorKind::NotFound => {
                    CompositionRetryNodeError::DraftNotFound
                }
                CompositionDraftRepositoryErrorKind::Storage => {
                    CompositionRetryNodeError::DraftStorage
                }
            })?;
        Ok(CompositionRetryNodeExecution {
            result: CompositionRetryNodeResult {
                draft_id: next.draft_id.clone(),
                revision: next.revision,
                item_id: request.item_id,
                definition_hash: replacement_hash
                    .ok_or(CompositionRetryNodeError::InvalidModelOutput)?,
                model_request_sha256: snapshot.request_sha256().clone(),
            },
            draft: next,
            request_snapshot: snapshot,
            response_model: response.model,
            usage: response.usage,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CompositionRetryNodeExecution {
    pub result: CompositionRetryNodeResult,
    pub draft: CompositionDraft,
    pub request_snapshot: ModelRequestSnapshot,
    pub response_model: String,
    pub usage: TokenUsage,
}

impl CompositionPlanContribution {
    fn validate(&self, pack: &LoadedGamePack) -> Result<(), CompositionPlanError> {
        if self.compositions.len() != pack.composition_profiles().len()
            || self.compositions.len() > 16
        {
            return Err(CompositionPlanError::InvalidPackGuidance);
        }
        let mut ids = BTreeSet::new();
        for entry in &self.compositions {
            let Some(profile) = pack.composition_profile(&entry.composition_id) else {
                return Err(CompositionPlanError::InvalidPackGuidance);
            };
            if !ids.insert(&entry.composition_id)
                || entry.allowed_item_types.is_empty()
                || entry.allowed_item_types.len() > 64
                || entry
                    .allowed_item_types
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    != entry.allowed_item_types.len()
                || !entry.allowed_item_types.contains(profile.root_item_type())
                || entry
                    .allowed_item_types
                    .iter()
                    .any(|id| pack.item_type(id).is_none())
                || entry.guidance.is_empty()
                || entry.guidance.len() > 64
                || entry.guidance.iter().any(|value| !valid_text(value, 2_000))
                || !valid_node_type_rules(profile, entry)
                || !valid_reference_binding_rules(pack, profile, entry)
            {
                return Err(CompositionPlanError::InvalidPackGuidance);
            }
        }
        Ok(())
    }
}

impl CompositionRetryContribution {
    fn validate(&self, pack: &LoadedGamePack) -> Result<(), CompositionRetryNodeError> {
        if self.compositions.len() != pack.composition_profiles().len()
            || self.compositions.len() > 16
        {
            return Err(CompositionRetryNodeError::InvalidPackGuidance);
        }
        let mut ids = BTreeSet::new();
        if self.compositions.iter().any(|entry| {
            pack.composition_profile(&entry.composition_id).is_none()
                || !ids.insert(&entry.composition_id)
                || entry.guidance.is_empty()
                || entry.guidance.len() > 32
                || entry.guidance.iter().any(|value| !valid_text(value, 2_000))
        }) {
            return Err(CompositionRetryNodeError::InvalidPackGuidance);
        }
        Ok(())
    }
}

fn valid_reference_binding_rules(
    pack: &LoadedGamePack,
    profile: &CompositionProfileSet,
    guidance: &CompositionPlanGuidance,
) -> bool {
    if guidance.reference_binding_rules.len() > 128 {
        return false;
    }
    guidance.reference_binding_rules.iter().all(|rule| {
        let Some(descriptor) = pack.item_type(&rule.source_item_type) else {
            return false;
        };
        let Some(slot) = descriptor
            .reference_slots()
            .iter()
            .find(|slot| slot.id() == &rule.slot_id)
        else {
            return false;
        };
        guidance.allowed_item_types.contains(&rule.source_item_type)
            && rule.base_count <= 128
            && rule.parameter_multiplier <= 128
            && (rule.measure != ReferenceBindingMeasure::TotalQuantity
                || slot.kind() == ItemReferenceKind::Pinned)
            && match &rule.parameter_id {
                None => rule.parameter_multiplier == 0,
                Some(parameter_id) => {
                    rule.parameter_multiplier > 0
                        && profile
                            .parameters()
                            .iter()
                            .any(|parameter| parameter.id() == parameter_id)
                }
            }
    })
}

fn valid_node_type_rules(
    profile: &CompositionProfileSet,
    guidance: &CompositionPlanGuidance,
) -> bool {
    if guidance.node_type_rules.is_empty() || guidance.node_type_rules.len() > 64 {
        return false;
    }
    let base_count = guidance
        .node_type_rules
        .iter()
        .try_fold(0_u32, |sum, rule| sum.checked_add(rule.base_count));
    if base_count != Some(profile.base_node_count())
        || guidance
            .node_type_rules
            .iter()
            .filter(|rule| &rule.item_type == profile.root_item_type())
            .map(|rule| rule.base_count)
            .sum::<u32>()
            == 0
        || guidance.node_type_rules.iter().any(|rule| {
            !guidance.allowed_item_types.contains(&rule.item_type)
                || rule.base_count > 128
                || rule.parameter_multiplier > 128
                || (rule.parameter_id.is_none() && rule.parameter_multiplier != 0)
                || rule.parameter_id.as_ref().is_some_and(|id| {
                    !profile
                        .parameters()
                        .iter()
                        .any(|parameter| parameter.id() == id)
                        || rule.parameter_multiplier == 0
                })
        })
    {
        return false;
    }
    profile.parameters().iter().all(|parameter| {
        guidance
            .node_type_rules
            .iter()
            .filter(|rule| rule.parameter_id.as_ref() == Some(parameter.id()))
            .map(|rule| rule.parameter_multiplier)
            .sum::<u32>()
            == parameter.node_weight()
    })
}

fn validate_request<'a>(
    pack: &'a LoadedGamePack,
    request: &CompositionPlanRequest,
) -> Result<&'a CompositionProfileSet, CompositionPlanError> {
    if !valid_text(&request.concept, 16_000) {
        return Err(CompositionPlanError::InvalidInput);
    }
    let profile = pack
        .composition_profile(&request.composition_id)
        .ok_or(CompositionPlanError::UnsupportedComposition)?;
    profile
        .validate_parameters(&request.parameters)
        .map_err(|_| CompositionPlanError::InvalidProfile)?;
    let source_valid = match &request.source {
        ItemCompositionSource::Preset { profile_id } => profile
            .profiles()
            .iter()
            .any(|value| value.id() == profile_id && value.values() == &request.parameters),
        ItemCompositionSource::Custom { base_profile_id } => profile
            .profiles()
            .iter()
            .any(|value| value.id() == base_profile_id),
    };
    if !source_valid {
        return Err(CompositionPlanError::InvalidProfile);
    }
    Ok(profile)
}

fn build_definitions(
    pack: &LoadedGamePack,
    profile_set: &CompositionProfileSet,
    guidance: &CompositionPlanGuidance,
    profile: &ItemCompositionProfile,
    planned: ModelCompositionPlan,
    resource_bindings: &BTreeMap<ItemId, BTreeMap<ResourceId, ItemResourceBinding>>,
) -> Result<Vec<StoredItemDefinition>, CompositionPlanError> {
    let expected_count = profile_set
        .parameters()
        .iter()
        .try_fold(profile_set.base_node_count(), |count, parameter| {
            count.checked_add(
                profile
                    .parameters
                    .get(parameter.id())
                    .copied()
                    .unwrap_or_default()
                    .saturating_mul(parameter.node_weight()),
            )
        })
        .ok_or(CompositionPlanError::InvalidProfile)?;
    if planned.nodes.len() != usize::try_from(expected_count).unwrap_or(usize::MAX) {
        return Err(CompositionPlanError::ProfileCountMismatch);
    }
    let root_id = planned.root_item_id;
    let mut source = BTreeMap::new();
    for node in planned.nodes {
        if !guidance.allowed_item_types.contains(&node.item_type)
            || source.insert(node.item_id.clone(), node).is_some()
        {
            return Err(CompositionPlanError::InvalidModelOutput);
        }
    }
    if source
        .get(&root_id)
        .is_none_or(|node| &node.item_type != profile_set.root_item_type())
    {
        return Err(CompositionPlanError::InvalidModelOutput);
    }
    let actual_counts = source.values().fold(BTreeMap::new(), |mut counts, node| {
        *counts.entry(node.item_type.clone()).or_insert(0_u32) += 1;
        counts
    });
    let mut expected_counts = BTreeMap::new();
    for rule in &guidance.node_type_rules {
        let parameter_count = rule
            .parameter_id
            .as_ref()
            .and_then(|id| profile.parameters.get(id))
            .copied()
            .unwrap_or_default();
        let count = rule
            .base_count
            .checked_add(parameter_count.saturating_mul(rule.parameter_multiplier))
            .ok_or(CompositionPlanError::InvalidProfile)?;
        let current = expected_counts
            .entry(rule.item_type.clone())
            .or_insert(0_u32);
        *current = current
            .checked_add(count)
            .ok_or(CompositionPlanError::InvalidProfile)?;
    }
    expected_counts.retain(|_, count| *count > 0);
    if actual_counts != expected_counts {
        return Err(CompositionPlanError::ProfileCountMismatch);
    }
    validate_reference_targets(&source)?;
    validate_reference_binding_counts(guidance, profile, &source)?;
    validate_root_pinned_closure(&root_id, &source)?;
    let mut resolved = BTreeMap::new();
    let mut active = BTreeSet::new();
    let ids = source.keys().cloned().collect::<Vec<_>>();
    for id in ids {
        resolve_node(
            pack,
            &source,
            &root_id,
            profile,
            &id,
            resource_bindings,
            &mut resolved,
            &mut active,
        )?;
    }
    Ok(resolved.into_values().collect())
}

fn validate_reference_binding_counts(
    guidance: &CompositionPlanGuidance,
    profile: &ItemCompositionProfile,
    nodes: &BTreeMap<ItemId, ModelCompositionNode>,
) -> Result<(), CompositionPlanError> {
    let mut expected = BTreeMap::new();
    for rule in &guidance.reference_binding_rules {
        let parameter_count = rule
            .parameter_id
            .as_ref()
            .and_then(|id| profile.parameters.get(id))
            .copied()
            .unwrap_or_default();
        let count = rule
            .base_count
            .checked_add(parameter_count.saturating_mul(rule.parameter_multiplier))
            .ok_or(CompositionPlanError::InvalidProfile)?;
        let key = (
            rule.source_item_type.clone(),
            rule.slot_id.clone(),
            rule.measure,
        );
        let current = expected.entry(key).or_insert(0_u32);
        *current = current
            .checked_add(count)
            .ok_or(CompositionPlanError::InvalidProfile)?;
    }

    for ((source_type, slot_id, measure), expected_count) in expected {
        for node in nodes.values().filter(|node| node.item_type == source_type) {
            let bindings = node
                .reference_bindings
                .get(&slot_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let unique_targets = bindings
                .iter()
                .map(|binding| match binding {
                    PlannedReference::Identity { item_id, .. }
                    | PlannedReference::Pinned { item_id, .. } => item_id,
                })
                .collect::<BTreeSet<_>>();
            if unique_targets.len() != bindings.len() {
                return Err(CompositionPlanError::InvalidModelOutput);
            }
            let actual = match measure {
                ReferenceBindingMeasure::Bindings => u32::try_from(bindings.len())
                    .map_err(|_| CompositionPlanError::InvalidModelOutput)?,
                ReferenceBindingMeasure::TotalQuantity => bindings
                    .iter()
                    .try_fold(0_u32, |sum, binding| match binding {
                        PlannedReference::Pinned { quantity, .. } => sum.checked_add(*quantity),
                        PlannedReference::Identity { .. } => None,
                    })
                    .ok_or(CompositionPlanError::InvalidModelOutput)?,
            };
            if actual != expected_count {
                return Err(CompositionPlanError::ProfileCountMismatch);
            }
        }
    }
    Ok(())
}

fn validate_root_pinned_closure(
    root_id: &ItemId,
    nodes: &BTreeMap<ItemId, ModelCompositionNode>,
) -> Result<(), CompositionPlanError> {
    let mut visited = BTreeSet::new();
    let mut pending = vec![root_id];
    while let Some(item_id) = pending.pop() {
        if !visited.insert(item_id) {
            continue;
        }
        let node = nodes
            .get(item_id)
            .ok_or(CompositionPlanError::InvalidModelOutput)?;
        for binding in node.reference_bindings.values().flatten() {
            if let PlannedReference::Pinned { item_id, .. } = binding {
                pending.push(item_id);
            }
        }
    }
    if visited.len() != nodes.len() {
        return Err(CompositionPlanError::InvalidModelOutput);
    }
    Ok(())
}

fn validate_reference_targets(
    nodes: &BTreeMap<ItemId, ModelCompositionNode>,
) -> Result<(), CompositionPlanError> {
    for node in nodes.values() {
        for bindings in node.reference_bindings.values() {
            for binding in bindings {
                let (target_id, expected_type) = match binding {
                    PlannedReference::Identity {
                        item_id,
                        expected_item_type,
                    } => (item_id, Some(expected_item_type)),
                    PlannedReference::Pinned { item_id, .. } => (item_id, None),
                };
                let target = nodes
                    .get(target_id)
                    .ok_or(CompositionPlanError::InvalidModelOutput)?;
                if expected_type.is_some_and(|value| value != &target.item_type) {
                    return Err(CompositionPlanError::InvalidModelOutput);
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn resolve_node(
    pack: &LoadedGamePack,
    source: &BTreeMap<ItemId, ModelCompositionNode>,
    root_id: &ItemId,
    profile: &ItemCompositionProfile,
    item_id: &ItemId,
    resource_bindings: &BTreeMap<ItemId, BTreeMap<ResourceId, ItemResourceBinding>>,
    resolved: &mut BTreeMap<ItemId, StoredItemDefinition>,
    active: &mut BTreeSet<ItemId>,
) -> Result<StoredItemDefinition, CompositionPlanError> {
    if let Some(value) = resolved.get(item_id) {
        return Ok(value.clone());
    }
    if !active.insert(item_id.clone()) {
        return Err(CompositionPlanError::PinnedCycle);
    }
    let node = source
        .get(item_id)
        .ok_or(CompositionPlanError::InvalidModelOutput)?;
    let mut definition = ItemDefinition::new(node.item_id.clone(), node.item_type.clone());
    definition.canonical_fields = node.canonical_fields.clone();
    definition.behavior_intent = node.behavior_intent.clone();
    definition.localizations = node
        .localizations
        .iter()
        .map(|(locale, fields)| {
            (
                locale.clone(),
                ItemLocalization {
                    fields: fields.clone(),
                    status: LocalizationStatus::Confirmed,
                    translated_from: None,
                },
            )
        })
        .collect();
    definition.resource_bindings = resource_bindings.get(item_id).cloned().unwrap_or_default();
    if item_id == root_id {
        definition.composition_profile = Some(profile.clone());
    }
    for (slot_id, bindings) in &node.reference_bindings {
        let mut values = Vec::with_capacity(bindings.len());
        for binding in bindings {
            values.push(match binding {
                PlannedReference::Identity {
                    item_id,
                    expected_item_type,
                } => ItemReferenceBinding::Identity {
                    item_id: item_id.clone(),
                    expected_item_type: expected_item_type.clone(),
                },
                PlannedReference::Pinned { item_id, quantity } => {
                    let target = resolve_node(
                        pack,
                        source,
                        root_id,
                        profile,
                        item_id,
                        resource_bindings,
                        resolved,
                        active,
                    )?;
                    ItemReferenceBinding::Pinned {
                        item_id: item_id.clone(),
                        definition_hash: target.definition_hash,
                        quantity: *quantity,
                    }
                }
            });
        }
        definition
            .reference_bindings
            .insert(slot_id.clone(), values);
    }
    active.remove(item_id);
    ItemDefinitionValidator::validate(pack, &definition, ItemDefinitionValidationMode::Draft)
        .map_err(|_| CompositionPlanError::InvalidModelOutput)?;
    let stored = StoredItemDefinition {
        definition_hash: definition
            .definition_hash()
            .map_err(|_| CompositionPlanError::InvalidModelOutput)?,
        definition,
    };
    resolved.insert(item_id.clone(), stored.clone());
    Ok(stored)
}

fn definition_to_model_node(definition: &ItemDefinition) -> ModelCompositionNode {
    ModelCompositionNode {
        item_id: definition.item_id.clone(),
        item_type: definition.item_type.clone(),
        canonical_fields: definition.canonical_fields.clone(),
        behavior_intent: definition.behavior_intent.clone(),
        localizations: definition
            .localizations
            .iter()
            .map(|(locale, localization)| (locale.clone(), localization.fields.clone()))
            .collect(),
        reference_bindings: definition
            .reference_bindings
            .iter()
            .map(|(slot_id, bindings)| {
                (
                    slot_id.clone(),
                    bindings
                        .iter()
                        .map(|binding| match binding {
                            ItemReferenceBinding::Identity {
                                item_id,
                                expected_item_type,
                            } => PlannedReference::Identity {
                                item_id: item_id.clone(),
                                expected_item_type: expected_item_type.clone(),
                            },
                            ItemReferenceBinding::Pinned {
                                item_id, quantity, ..
                            } => PlannedReference::Pinned {
                                item_id: item_id.clone(),
                                quantity: *quantity,
                            },
                        })
                        .collect(),
                )
            })
            .collect(),
    }
}

fn query_evidence(
    truth: &VerifiedTruthSnapshot,
    pack: &LoadedGamePack,
    item_types: &[ItemTypeId],
) -> Result<Vec<TruthEvidenceRecord>, CompositionPlanError> {
    let mut evidence = Vec::new();
    let mut identities = BTreeSet::new();
    for item_type in item_types {
        let descriptor = pack
            .item_type(item_type)
            .ok_or(CompositionPlanError::InvalidPackGuidance)?;
        for query in descriptor.evidence_queries() {
            let records = truth.query(&query.as_query())?;
            if records.is_empty() {
                return Err(CompositionPlanError::MissingEvidence);
            }
            for record in records {
                let identity = (
                    record.source_id.clone(),
                    record.symbol.clone(),
                    record.relative_path.clone(),
                );
                if identities.insert(identity) {
                    evidence.push(record);
                    if evidence.len() == 64 {
                        return Ok(evidence);
                    }
                }
            }
        }
    }
    Ok(evidence)
}

fn validate_context(context: &CompositionPlanContext<'_>) -> Result<(), CompositionPlanError> {
    let manifest = context.truth.manifest();
    if context.contributions.feature_id() != &CompositionPlanFeature::id()
        || context.contributions.game_pack_id() != context.pack.id()
        || context.contributions.game_pack_sha256() != context.pack.content_sha256()
        || manifest.game_pack_id() != context.pack.id()
        || manifest.game_pack_sha256() != context.pack.content_sha256()
    {
        return Err(CompositionPlanError::ContextIdentityMismatch);
    }
    Ok(())
}

fn validate_retry_context(
    context: &CompositionRetryNodeContext<'_>,
) -> Result<(), CompositionRetryNodeError> {
    if context.plan_contributions.game_pack_id() != context.pack.id()
        || context.plan_contributions.game_pack_sha256() != context.pack.content_sha256()
        || context.plan_contributions.feature_id() != &CompositionPlanFeature::id()
        || context.retry_contributions.game_pack_id() != context.pack.id()
        || context.retry_contributions.game_pack_sha256() != context.pack.content_sha256()
        || context.retry_contributions.feature_id() != &CompositionRetryNodeFeature::id()
        || context.truth.manifest().game_pack_id() != context.pack.id()
        || context.truth.manifest().game_pack_sha256() != context.pack.content_sha256()
    {
        return Err(CompositionRetryNodeError::ContextIdentityMismatch);
    }
    Ok(())
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), CompositionPlanError> {
    if cancellation.is_cancelled() {
        Err(CompositionPlanError::Cancelled)
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CompositionPlanError {
    #[error("composition planning input is invalid")]
    InvalidInput,
    #[error("composition planning context identities do not match")]
    ContextIdentityMismatch,
    #[error("composition planning Pack guidance is invalid")]
    InvalidPackGuidance,
    #[error("composition type is unsupported by the Pack")]
    UnsupportedComposition,
    #[error("composition profile parameters are invalid")]
    InvalidProfile,
    #[error("composition model node count does not match the profile")]
    ProfileCountMismatch,
    #[error("composition planning Recipe is invalid")]
    InvalidRecipeContract,
    #[error("composition model output was truncated")]
    TruncatedModelOutput,
    #[error("composition model output is invalid")]
    InvalidModelOutput,
    #[error("composition pinned references contain a cycle")]
    PinnedCycle,
    #[error("composition planning Truth evidence is missing")]
    MissingEvidence,
    #[error("composition Draft identity already exists")]
    DraftConflict,
    #[error("composition Draft storage failed")]
    DraftStorage,
    #[error("composition Item storage failed")]
    ItemStorage,
    #[error("composition Item identity has a different type")]
    ItemTypeConflict,
    #[error("composition planning was cancelled")]
    Cancelled,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Evidence(#[from] EvidenceQueryError),
    #[error(transparent)]
    Recipe(#[from] FeatureRecipeError),
    #[error(transparent)]
    Request(#[from] ModelRequestError),
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl CompositionPlanError {
    #[must_use]
    pub fn run_failure(&self) -> RunFailure {
        let (code, stage) = match self {
            Self::InvalidInput => ("run.input_invalid", "composition.plan.request"),
            Self::ContextIdentityMismatch => ("truth.context_mismatch", "composition.plan.context"),
            Self::InvalidPackGuidance | Self::Contribution(_) => {
                ("pack.contribution_invalid", "composition.plan.pack")
            }
            Self::UnsupportedComposition => (
                "composition.profile.unsupported",
                "composition.plan.profile",
            ),
            Self::InvalidProfile => ("composition.profile.invalid", "composition.plan.profile"),
            Self::ProfileCountMismatch => (
                "composition.profile.count_mismatch",
                "composition.plan.model",
            ),
            Self::InvalidRecipeContract | Self::Recipe(_) => {
                ("feature.recipe_invalid", "composition.plan.recipe")
            }
            Self::TruncatedModelOutput => ("model.output_truncated", "composition.plan.model"),
            Self::InvalidModelOutput => ("model.output_invalid", "composition.plan.model"),
            Self::PinnedCycle => ("composition.graph.cycle", "composition.plan.graph"),
            Self::MissingEvidence => ("truth.evidence_missing", "composition.plan.truth"),
            Self::Evidence(_) => ("truth.query_invalid", "composition.plan.truth"),
            Self::DraftConflict => ("composition.draft.conflict", "composition.plan.persist"),
            Self::DraftStorage => (
                "composition.draft.storage_failed",
                "composition.plan.persist",
            ),
            Self::ItemStorage => ("item.storage_failed", "composition.plan.current"),
            Self::ItemTypeConflict => (
                "composition.draft.item_type_conflict",
                "composition.plan.current",
            ),
            Self::Cancelled | Self::Model(ModelError::Cancelled) => {
                ("run.cancelled", "composition.plan.execute")
            }
            Self::Request(_) => ("model.request_invalid", "composition.plan.model"),
            Self::Model(ModelError::Authentication) => {
                ("model.authentication", "composition.plan.model")
            }
            Self::Model(ModelError::RateLimited { .. }) => {
                ("model.rate_limited", "composition.plan.model")
            }
            Self::Model(ModelError::Configuration) => {
                ("model.configuration", "composition.plan.model")
            }
            Self::Model(ModelError::Transport) => {
                ("model.transport_failed", "composition.plan.model")
            }
            Self::Model(ModelError::Rejected) => {
                ("model.request_rejected", "composition.plan.model")
            }
            Self::Model(ModelError::InvalidResponse) => {
                ("model.response_invalid", "composition.plan.model")
            }
        };
        RunFailure::new(
            FailureCode::parse(code).expect("built-in failure code is valid"),
            stage,
            None,
        )
        .expect("built-in Run failure is valid")
    }
}

#[derive(Debug, Error)]
pub enum CompositionRetryNodeError {
    #[error("composition retry input is invalid")]
    InvalidInput,
    #[error("composition retry context identities do not match")]
    ContextIdentityMismatch,
    #[error("composition retry Pack guidance is invalid")]
    InvalidPackGuidance,
    #[error("composition type is unsupported by the Pack")]
    UnsupportedComposition,
    #[error("composition profile parameters are invalid")]
    InvalidProfile,
    #[error("composition graph no longer matches the profile")]
    ProfileCountMismatch,
    #[error("composition retry Recipe is invalid")]
    InvalidRecipeContract,
    #[error("composition retry model output was truncated")]
    TruncatedModelOutput,
    #[error("composition retry model output is invalid")]
    InvalidModelOutput,
    #[error("composition retry target identity changed")]
    TargetIdentityMismatch,
    #[error("composition retry target does not exist")]
    TargetNotFound,
    #[error("composition pinned references contain a cycle")]
    PinnedCycle,
    #[error("composition retry Truth evidence is missing")]
    MissingEvidence,
    #[error("composition Draft does not exist")]
    DraftNotFound,
    #[error("composition Draft revision changed")]
    DraftConflict,
    #[error("composition Draft storage failed")]
    DraftStorage,
    #[error("composition retry was cancelled")]
    Cancelled,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Evidence(#[from] EvidenceQueryError),
    #[error(transparent)]
    Recipe(#[from] FeatureRecipeError),
    #[error(transparent)]
    Request(#[from] ModelRequestError),
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl CompositionRetryNodeError {
    #[must_use]
    pub fn run_failure(&self) -> RunFailure {
        let (code, stage) = match self {
            Self::InvalidInput => ("run.input_invalid", "composition.retry-node.request"),
            Self::ContextIdentityMismatch => {
                ("truth.context_mismatch", "composition.retry-node.context")
            }
            Self::InvalidPackGuidance | Self::Contribution(_) => {
                ("pack.contribution_invalid", "composition.retry-node.pack")
            }
            Self::UnsupportedComposition => (
                "composition.profile.unsupported",
                "composition.retry-node.profile",
            ),
            Self::InvalidProfile => (
                "composition.profile.invalid",
                "composition.retry-node.profile",
            ),
            Self::ProfileCountMismatch => (
                "composition.profile.count_mismatch",
                "composition.retry-node.graph",
            ),
            Self::InvalidRecipeContract | Self::Recipe(_) => {
                ("feature.recipe_invalid", "composition.retry-node.recipe")
            }
            Self::TruncatedModelOutput => {
                ("model.output_truncated", "composition.retry-node.model")
            }
            Self::InvalidModelOutput | Self::TargetIdentityMismatch => {
                ("model.output_invalid", "composition.retry-node.model")
            }
            Self::TargetNotFound => ("composition.draft.invalid", "composition.retry-node.target"),
            Self::PinnedCycle => ("composition.graph.cycle", "composition.retry-node.graph"),
            Self::MissingEvidence => ("truth.evidence_missing", "composition.retry-node.truth"),
            Self::Evidence(_) => ("truth.query_invalid", "composition.retry-node.truth"),
            Self::DraftNotFound => ("composition.draft.not_found", "composition.retry-node.load"),
            Self::DraftConflict => (
                "composition.draft.conflict",
                "composition.retry-node.persist",
            ),
            Self::DraftStorage => (
                "composition.draft.storage_failed",
                "composition.retry-node.persist",
            ),
            Self::Cancelled | Self::Model(ModelError::Cancelled) => {
                ("run.cancelled", "composition.retry-node.execute")
            }
            Self::Request(_) => ("model.request_invalid", "composition.retry-node.model"),
            Self::Model(ModelError::Authentication) => {
                ("model.authentication", "composition.retry-node.model")
            }
            Self::Model(ModelError::RateLimited { .. }) => {
                ("model.rate_limited", "composition.retry-node.model")
            }
            Self::Model(ModelError::Configuration) => {
                ("model.configuration", "composition.retry-node.model")
            }
            Self::Model(ModelError::Transport) => {
                ("model.transport_failed", "composition.retry-node.model")
            }
            Self::Model(ModelError::Rejected) => {
                ("model.request_rejected", "composition.retry-node.model")
            }
            Self::Model(ModelError::InvalidResponse) => {
                ("model.response_invalid", "composition.retry-node.model")
            }
        };
        RunFailure::new(
            FailureCode::parse(code).expect("built-in failure code is valid"),
            stage,
            None,
        )
        .expect("built-in Run failure is valid")
    }
}

fn map_retry_plan_error(error: CompositionPlanError) -> CompositionRetryNodeError {
    match error {
        CompositionPlanError::InvalidInput => CompositionRetryNodeError::InvalidInput,
        CompositionPlanError::ContextIdentityMismatch => {
            CompositionRetryNodeError::ContextIdentityMismatch
        }
        CompositionPlanError::InvalidPackGuidance | CompositionPlanError::Contribution(_) => {
            CompositionRetryNodeError::InvalidPackGuidance
        }
        CompositionPlanError::UnsupportedComposition => {
            CompositionRetryNodeError::UnsupportedComposition
        }
        CompositionPlanError::InvalidProfile => CompositionRetryNodeError::InvalidProfile,
        CompositionPlanError::ProfileCountMismatch => {
            CompositionRetryNodeError::ProfileCountMismatch
        }
        CompositionPlanError::PinnedCycle => CompositionRetryNodeError::PinnedCycle,
        CompositionPlanError::MissingEvidence => CompositionRetryNodeError::MissingEvidence,
        CompositionPlanError::Evidence(error) => CompositionRetryNodeError::Evidence(error),
        CompositionPlanError::Cancelled => CompositionRetryNodeError::Cancelled,
        _ => CompositionRetryNodeError::InvalidModelOutput,
    }
}

fn contribution_slot() -> ContributionId {
    ContributionId::parse("composition.plan.guidance").expect("built-in contribution ID is valid")
}

fn retry_contribution_slot() -> ContributionId {
    ContributionId::parse("composition.retry-node.guidance")
        .expect("built-in contribution ID is valid")
}

fn model_output_schema() -> SchemaRef {
    schema("feature.composition-plan-model-output")
}

fn retry_model_output_schema() -> SchemaRef {
    schema("feature.composition-retry-node-model-output")
}

fn schema(id: &str) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(1).expect("built-in schema version is valid"),
    }
}

fn serialize<T: Serialize + ?Sized>(value: &T) -> Result<String, CompositionPlanError> {
    serde_json::to_string_pretty(value).map_err(|_| CompositionPlanError::InvalidInput)
}

fn bounded_optional(value: Option<&str>, max: usize) -> Result<String, CompositionPlanError> {
    let value = value.unwrap_or("");
    if value.chars().count() > max || value.contains('\0') {
        return Err(CompositionPlanError::InvalidInput);
    }
    Ok(value.to_owned())
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains('\0')
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ats_game_context::{
        ContributionResolver, GamePackLoader, TruthSnapshotIndex, TruthSnapshotManifest,
        TruthSnapshotSource,
    };
    use ats_kernel::{CompositionParameterId, CompositionProfileId, PrimitiveId};
    use ats_runtime::{ModelResponse, ModelStream};
    use futures_util::stream;
    use sha2::{Digest, Sha256};

    use super::*;

    #[derive(Debug, Error)]
    #[error("memory failure")]
    struct MemoryError;

    #[derive(Default)]
    struct MemoryItems;

    impl ItemRepository for MemoryItems {
        type Error = MemoryError;

        fn classify_error(_: &Self::Error) -> ItemRepositoryErrorKind {
            ItemRepositoryErrorKind::NotFound
        }

        fn save(&self, _: &ItemDefinition) -> Result<StoredItemDefinition, Self::Error> {
            Err(MemoryError)
        }

        fn load_current(&self, _: &ItemId) -> Result<StoredItemDefinition, Self::Error> {
            Err(MemoryError)
        }

        fn load_version(
            &self,
            _: &ItemId,
            _: &Sha256Digest,
        ) -> Result<StoredItemDefinition, Self::Error> {
            Err(MemoryError)
        }

        fn list_current(&self) -> Result<Vec<StoredItemDefinition>, Self::Error> {
            Ok(Vec::new())
        }
    }

    #[derive(Default)]
    struct MemoryDrafts(Mutex<Option<CompositionDraft>>);

    impl CompositionDraftRepository for MemoryDrafts {
        type Error = MemoryError;

        fn create(&self, draft: &CompositionDraft) -> Result<(), Self::Error> {
            let mut stored = self.0.lock().unwrap();
            if stored.is_some() {
                return Err(MemoryError);
            }
            *stored = Some(draft.clone());
            Ok(())
        }

        fn load(&self, _: &CompositionDraftId) -> Result<CompositionDraft, Self::Error> {
            self.0.lock().unwrap().clone().ok_or(MemoryError)
        }

        fn compare_and_set(
            &self,
            expected_revision: u64,
            next: &CompositionDraft,
        ) -> Result<(), Self::Error> {
            let mut stored = self.0.lock().unwrap();
            if stored
                .as_ref()
                .is_none_or(|draft| draft.revision != expected_revision)
            {
                return Err(MemoryError);
            }
            *stored = Some(next.clone());
            Ok(())
        }

        fn list(&self) -> Result<Vec<CompositionDraft>, Self::Error> {
            Ok(self.0.lock().unwrap().clone().into_iter().collect())
        }

        fn delete(&self, _: &CompositionDraftId, _: u64) -> Result<(), Self::Error> {
            Err(MemoryError)
        }
    }

    struct MockModel {
        snapshots: Mutex<Vec<ModelRequestSnapshot>>,
    }

    struct RetryModel {
        snapshots: Mutex<Vec<ModelRequestSnapshot>>,
    }

    #[async_trait]
    impl ModelClient for MockModel {
        async fn complete(
            &self,
            request: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelResponse, ModelError> {
            self.snapshots.lock().unwrap().push(request);
            Ok(ModelResponse {
                model: "fixture".into(),
                content: serde_json::json!({
                    "rootItemId":"fixture-root",
                    "nodes":[
                        {
                            "itemId":"fixture-root",
                            "itemType":"root",
                            "canonicalFields":{},
                            "behaviorIntent":["Provide one coherent root."],
                            "localizations":{},
                            "referenceBindings":{
                                "children":[{"kind":"pinned","itemId":"fixture-child","quantity":1}]
                            }
                        },
                        {
                            "itemId":"fixture-child",
                            "itemType":"child",
                            "canonicalFields":{},
                            "behaviorIntent":["Provide one child behavior."],
                            "localizations":{},
                            "referenceBindings":{
                                "owner":[{"kind":"identity","itemId":"fixture-root","expectedItemType":"root"}]
                            }
                        }
                    ]
                }).to_string(),
                finish_reason: FinishReason::EndTurn,
                usage: TokenUsage::default(),
            })
        }

        async fn stream(
            &self,
            _: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelStream, ModelError> {
            Ok(Box::pin(stream::empty()))
        }
    }

    #[async_trait]
    impl ModelClient for RetryModel {
        async fn complete(
            &self,
            request: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelResponse, ModelError> {
            self.snapshots.lock().unwrap().push(request);
            Ok(ModelResponse {
                model: "fixture-retry".into(),
                content: serde_json::json!({
                    "itemId":"fixture-child",
                    "itemType":"child",
                    "canonicalFields":{},
                    "behaviorIntent":["Provide revised child behavior."],
                    "localizations":{},
                    "referenceBindings":{
                        "owner":[{"kind":"identity","itemId":"fixture-root","expectedItemType":"root"}]
                    }
                })
                .to_string(),
                finish_reason: FinishReason::EndTurn,
                usage: TokenUsage::default(),
            })
        }

        async fn stream(
            &self,
            _: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelStream, ModelError> {
            Ok(Box::pin(stream::empty()))
        }
    }

    fn digest(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
    }

    fn pack() -> LoadedGamePack {
        let value = serde_json::json!({
            "schemaVersion":4,
            "id":"fixture-game",
            "displayName":"Fixture",
            "itemTypes":[
                {
                    "id":"root","displayNames":{"eng":"Root"},
                    "referenceSlots":[{
                        "id":"children","displayNames":{"eng":"Children"},"kind":"pinned",
                        "allowedItemTypes":["child"],"minItems":1,"maxItems":8,
                        "minQuantity":1,"maxQuantity":8
                    }],
                    "evidenceQueries":[{"symbols":["Root.Symbol"],"terms":[]}]
                },
                {
                    "id":"child","displayNames":{"eng":"Child"},
                    "referenceSlots":[{
                        "id":"owner","displayNames":{"eng":"Owner"},"kind":"identity",
                        "allowedItemTypes":["root"],"minItems":1,"maxItems":1,
                        "minQuantity":1,"maxQuantity":1
                    }],
                    "evidenceQueries":[{"symbols":["Child.Symbol"],"terms":[]}]
                }
            ],
            "compositionProfiles":[{
                "id":"fixture_suite","displayNames":{"eng":"Fixture suite"},
                "rootItemType":"root","defaultProfile":"standard","customBaseProfile":"standard",
                "maxNodes":8,"baseNodeCount":1,
                "parameters":[{"id":"child_count","displayNames":{"eng":"Children"},"min":1,"max":4,"nodeWeight":1}],
                "profiles":[{"id":"standard","displayNames":{"eng":"Standard"},"values":{"child_count":1}}],
                "constraints":[]
            }],
            "contributions":[{
                "slotId":"composition.plan.guidance","featureId":"composition.plan",
                "schema":{"id":"pack.composition-plan-guidance","version":1},
                "payload":{"compositions":[{
                    "compositionId":"fixture_suite","allowedItemTypes":["root","child"],
                    "nodeTypeRules":[
                        {"itemType":"root","baseCount":1,"parameterMultiplier":0},
                        {"itemType":"child","baseCount":0,"parameterId":"child_count","parameterMultiplier":1}
                    ],
                    "guidance":["Plan one root and its children."]
                }]}
            },{
                "slotId":"composition.retry-node.guidance","featureId":"composition.retry-node",
                "schema":{"id":"pack.composition-retry-node-guidance","version":1},
                "payload":{"compositions":[{
                    "compositionId":"fixture_suite",
                    "guidance":["Revise only the requested fixture node."]
                }]}
            }]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        GamePackLoader::load(&bytes, &digest(&bytes)).unwrap()
    }

    fn truth(pack: &LoadedGamePack) -> VerifiedTruthSnapshot {
        let records = ["Root.Symbol", "Child.Symbol"]
            .into_iter()
            .map(|symbol| TruthEvidenceRecord {
                source_id: "fixture".into(),
                symbol: symbol.into(),
                purpose: "fixture readiness".into(),
                bounded_excerpt: format!("{symbol} is available."),
                relative_path: format!("indexes/{symbol}.cs"),
            })
            .collect::<Vec<_>>();
        let bytes = serde_json::to_vec(&records).unwrap();
        let manifest = TruthSnapshotManifest::new(
            pack,
            vec![TruthSnapshotSource {
                id: "fixture".into(),
                kind: "local_file".into(),
                version: Some("1".into()),
                relative_path: "sources/fixture.bin".into(),
                sha256: digest(b"fixture"),
                byte_length: 7,
            }],
            vec![TruthSnapshotIndex {
                id: "symbols".into(),
                provider: PrimitiveId::parse("truth.fixture-indexer").unwrap(),
                relative_path: "indexes/symbols.json".into(),
                sha256: digest(&bytes),
                record_count: records.len() as u32,
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

    fn model_nodes(value: serde_json::Value) -> BTreeMap<ItemId, ModelCompositionNode> {
        serde_json::from_value::<Vec<ModelCompositionNode>>(value)
            .unwrap()
            .into_iter()
            .map(|node| (node.item_id.clone(), node))
            .collect()
    }

    #[test]
    fn root_closure_rejects_nodes_reachable_only_by_identity() {
        let nodes = model_nodes(serde_json::json!([
            {
                "itemId":"fixture-root","itemType":"root","canonicalFields":{},
                "behaviorIntent":["Root"],"localizations":{},"referenceBindings":{}
            },
            {
                "itemId":"fixture-child","itemType":"child","canonicalFields":{},
                "behaviorIntent":["Child"],"localizations":{},
                "referenceBindings":{"owner":[{
                    "kind":"identity","itemId":"fixture-root","expectedItemType":"root"
                }]}
            }
        ]));

        assert!(matches!(
            validate_root_pinned_closure(&ItemId::parse("fixture-root").unwrap(), &nodes),
            Err(CompositionPlanError::InvalidModelOutput)
        ));
    }

    #[test]
    fn total_quantity_rule_accepts_exact_sum_and_rejects_duplicate_target() {
        let profile = ItemCompositionProfile {
            composition_id: CompositionId::parse("fixture_suite").unwrap(),
            source: ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            },
            parameters: BTreeMap::from([(
                CompositionParameterId::parse("child_count").unwrap(),
                1,
            )]),
        };
        let guidance = CompositionPlanGuidance {
            composition_id: CompositionId::parse("fixture_suite").unwrap(),
            allowed_item_types: vec![
                ItemTypeId::parse("root").unwrap(),
                ItemTypeId::parse("child").unwrap(),
            ],
            node_type_rules: Vec::new(),
            reference_binding_rules: vec![CompositionReferenceBindingRule {
                source_item_type: ItemTypeId::parse("root").unwrap(),
                slot_id: ItemReferenceSlotId::parse("children").unwrap(),
                measure: ReferenceBindingMeasure::TotalQuantity,
                base_count: 10,
                parameter_id: None,
                parameter_multiplier: 0,
            }],
            guidance: vec!["Fixture".into()],
        };
        let exact = model_nodes(serde_json::json!([{
            "itemId":"fixture-root","itemType":"root","canonicalFields":{},
            "behaviorIntent":["Root"],"localizations":{},
            "referenceBindings":{"children":[
                {"kind":"pinned","itemId":"fixture-child-a","quantity":4},
                {"kind":"pinned","itemId":"fixture-child-b","quantity":6}
            ]}
        }]));
        assert!(validate_reference_binding_counts(&guidance, &profile, &exact).is_ok());

        let duplicate = model_nodes(serde_json::json!([{
            "itemId":"fixture-root","itemType":"root","canonicalFields":{},
            "behaviorIntent":["Root"],"localizations":{},
            "referenceBindings":{"children":[
                {"kind":"pinned","itemId":"fixture-child","quantity":4},
                {"kind":"pinned","itemId":"fixture-child","quantity":6}
            ]}
        }]));
        assert!(matches!(
            validate_reference_binding_counts(&guidance, &profile, &duplicate),
            Err(CompositionPlanError::InvalidModelOutput)
        ));
    }

    #[tokio::test]
    async fn plan_persists_a_profile_bound_draft_and_computes_pinned_hashes() {
        let pack = pack();
        let truth = truth(&pack);
        let contributions = ContributionResolver::new(Vec::<PrimitiveId>::new())
            .resolve(
                &pack,
                &CompositionPlanFeature::id(),
                &[CompositionPlanFeature::contribution_requirement()],
            )
            .unwrap();
        let drafts = MemoryDrafts::default();
        let model = MockModel {
            snapshots: Mutex::new(Vec::new()),
        };
        let execution = CompositionPlanService::built_in()
            .unwrap()
            .execute(
                &model,
                &MemoryItems,
                &drafts,
                CompositionPlanRequest {
                    draft_id: CompositionDraftId::parse("fixture-draft").unwrap(),
                    composition_id: CompositionId::parse("fixture_suite").unwrap(),
                    concept: "Create a coherent fixture suite.".into(),
                    source: ItemCompositionSource::Preset {
                        profile_id: CompositionProfileId::parse("standard").unwrap(),
                    },
                    parameters: BTreeMap::from([(
                        CompositionParameterId::parse("child_count").unwrap(),
                        1,
                    )]),
                },
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: Some("Fixture project"),
                    custom_instructions: None,
                    model: None,
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(execution.result.node_count, 2);
        let draft = drafts.load(&execution.result.draft_id).unwrap();
        assert_eq!(
            draft.profile.source,
            ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            }
        );
        let root = &draft.nodes[&draft.root_item_id].definition;
        let child = &draft.nodes[&ItemId::parse("fixture-child").unwrap()].definition;
        let ItemReferenceBinding::Pinned {
            definition_hash, ..
        } = &root.reference_bindings[&ItemReferenceSlotId::parse("children").unwrap()][0]
        else {
            panic!("root reference must be pinned")
        };
        assert_eq!(definition_hash, &child.definition_hash().unwrap());
        assert!(execution.request_snapshot.truth_snapshot_id().is_some());
    }

    #[tokio::test]
    async fn targeted_retry_revises_one_node_and_recomputes_the_complete_graph() {
        let pack = pack();
        let truth = truth(&pack);
        let resolver = ContributionResolver::new(Vec::<PrimitiveId>::new());
        let plan_contributions = resolver
            .resolve(
                &pack,
                &CompositionPlanFeature::id(),
                &[CompositionPlanFeature::contribution_requirement()],
            )
            .unwrap();
        let retry_contributions = resolver
            .resolve(
                &pack,
                &CompositionRetryNodeFeature::id(),
                &[CompositionRetryNodeFeature::contribution_requirement()],
            )
            .unwrap();
        let drafts = MemoryDrafts::default();
        let plan_model = MockModel {
            snapshots: Mutex::new(Vec::new()),
        };
        let planned = CompositionPlanService::built_in()
            .unwrap()
            .execute(
                &plan_model,
                &MemoryItems,
                &drafts,
                CompositionPlanRequest {
                    draft_id: CompositionDraftId::parse("fixture-retry-draft").unwrap(),
                    composition_id: CompositionId::parse("fixture_suite").unwrap(),
                    concept: "Create a retryable fixture suite.".into(),
                    source: ItemCompositionSource::Preset {
                        profile_id: CompositionProfileId::parse("standard").unwrap(),
                    },
                    parameters: BTreeMap::from([(
                        CompositionParameterId::parse("child_count").unwrap(),
                        1,
                    )]),
                },
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &plan_contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        let old_root_hash = planned.draft.nodes[&planned.draft.root_item_id]
            .definition
            .definition_hash()
            .unwrap();
        let retry_model = RetryModel {
            snapshots: Mutex::new(Vec::new()),
        };
        let request = CompositionRetryNodeRequest {
            draft_id: planned.draft.draft_id.clone(),
            expected_revision: planned.draft.revision,
            item_id: ItemId::parse("fixture-child").unwrap(),
            instructions: "Make the child behavior more explicit.".into(),
        };
        let retried = CompositionRetryNodeService::built_in()
            .unwrap()
            .execute(
                &retry_model,
                &drafts,
                request.clone(),
                CompositionRetryNodeContext {
                    pack: &pack,
                    plan_contributions: &plan_contributions,
                    retry_contributions: &retry_contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(retried.result.revision, 2);
        assert_eq!(retried.draft.nodes.len(), 2);
        let child = &retried.draft.nodes[&request.item_id].definition;
        assert_eq!(
            child.behavior_intent,
            vec!["Provide revised child behavior."]
        );
        assert_eq!(
            retried.result.definition_hash,
            child.definition_hash().unwrap()
        );
        let root = &retried.draft.nodes[&retried.draft.root_item_id].definition;
        assert_ne!(root.definition_hash().unwrap(), old_root_hash);
        let ItemReferenceBinding::Pinned {
            definition_hash, ..
        } = &root.reference_bindings[&ItemReferenceSlotId::parse("children").unwrap()][0]
        else {
            panic!("root reference must remain pinned")
        };
        assert_eq!(definition_hash, &retried.result.definition_hash);
        let prompt = retry_model.snapshots.lock().unwrap()[0]
            .request()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!prompt.contains("definitionHash"));
        assert!(!prompt.contains("resourceBindings"));

        let stale = CompositionRetryNodeService::built_in()
            .unwrap()
            .execute(
                &retry_model,
                &drafts,
                request,
                CompositionRetryNodeContext {
                    pack: &pack,
                    plan_contributions: &plan_contributions,
                    retry_contributions: &retry_contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                },
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(
            stale,
            Err(CompositionRetryNodeError::DraftConflict)
        ));
        assert_eq!(retry_model.snapshots.lock().unwrap().len(), 1);
    }
}
