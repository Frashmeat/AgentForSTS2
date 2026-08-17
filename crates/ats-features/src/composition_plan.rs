use std::collections::{BTreeMap, BTreeSet};

use ats_game_context::{
    CompositionProfileSet, ContributionResolverError, EvidenceQueryError, ItemFieldValueSpec,
    ItemReferenceKind, ItemTypeDescriptor, LoadedGamePack, TruthEvidenceRecord,
    VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{
    CompositionDraftId, CompositionId, ContributionId, ExecutionGraphId, FailureCode, FeatureId,
    ItemFieldId, ItemId, ItemReferenceSlotId, ItemTypeId, LocaleId, LocalizationFieldId, RecipeId,
    ResourceId, SchemaId, SchemaRef, SchemaVersion, Sha256Digest,
};
use ats_runtime::{
    CancellationToken, FinishReason, ModelClient, ModelError, ModelGamePackRef,
    ModelOutputContract, ModelRequestError, ModelRequestSnapshot, RunFailure, TokenUsage,
    VersionedPayload,
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

mod staged;
pub use staged::*;

const RECIPE_BYTES: &[u8] = include_bytes!("../recipes/composition-plan.json");
const RECIPE_SHA256: &str = "ebf895d74318d5798c9cd6c97e5c8d43b71d9723e26fd5e7f2e6cbc174e687c6";
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
            schema: schema_version("pack.composition-plan-guidance", 2),
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<CompositionExecutionRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CompositionExecutionRequest {
    Start {
        execution_graph_id: ExecutionGraphId,
    },
    Resume {
        execution_graph_id: ExecutionGraphId,
        expected_revision: u64,
        previous_run_id: ats_runtime::RunId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionPlanResult {
    pub draft_id: CompositionDraftId,
    pub revision: u64,
    pub root_item_id: ItemId,
    pub node_count: u32,
    pub model_request_sha256: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_graph_id: Option<ExecutionGraphId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validated_content_digest: Option<Sha256Digest>,
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
    node_groups: Vec<CompositionNodeGroup>,
    #[serde(default)]
    coordination: CompositionCoordination,
    #[serde(default)]
    binding_rules: Vec<CompositionBindingRule>,
    guidance: Vec<String>,
}

impl CompositionPlanGuidance {
    fn allowed_item_types(&self) -> Vec<ItemTypeId> {
        self.node_groups
            .iter()
            .map(|group| group.item_type.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
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
struct CompositionNodeGroup {
    id: String,
    item_type: ItemTypeId,
    count: CompositionCountRule,
    #[serde(default)]
    depends_on_group_ids: Vec<String>,
    generation_kind: CompositionGenerationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionCountRule {
    base_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parameter_id: Option<ats_kernel::CompositionParameterId>,
    multiplier: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CompositionGenerationKind {
    ModelItem,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionCoordination {
    #[serde(default)]
    suite_brief: CompositionSuiteBrief,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionSuiteBrief {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    guidance: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum ReferenceBindingMeasure {
    Bindings,
    TotalQuantity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionBindingRule {
    source_group_id: String,
    slot_id: ItemReferenceSlotId,
    target_group_ids: Vec<String>,
    measure: ReferenceBindingMeasure,
    quantity_policy: CompositionQuantityPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum CompositionQuantityPolicy {
    Constant {
        value: u32,
    },
    BriefDistribution {
        total_parameter_id: ats_kernel::CompositionParameterId,
        min_per_target: u32,
        max_per_target: u32,
    },
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedCompositionProfile<'a> {
    composition_id: &'a CompositionId,
    root_item_type: &'a ItemTypeId,
    max_nodes: u32,
    selected_profile: &'a ItemCompositionProfile,
    expected_node_count: u32,
    expected_node_type_counts: Vec<ResolvedNodeTypeCount>,
    expected_reference_bindings: Vec<ResolvedReferenceBindingCount>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedNodeTypeCount {
    item_type: ItemTypeId,
    count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolvedReferenceBindingCount {
    source_item_type: ItemTypeId,
    slot_id: ItemReferenceSlotId,
    measure: ReferenceBindingMeasure,
    count: u32,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompositionPlanFailureDetails {
    reason_code: CompositionPlanFailureReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actual_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_id: Option<ItemId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    item_type: Option<ItemTypeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot_id: Option<ItemReferenceSlotId>,
}

#[derive(Debug, Clone, Copy, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum CompositionPlanFailureReason {
    JsonDecode,
    RootProfileMissing,
    DraftGraphInvalid,
    NodeCountOverflow,
    NodeTotal,
    ItemTypeUnsupported,
    ItemIdDuplicate,
    RootMissingOrWrongType,
    ItemTypeCount,
    ReferenceTargetDuplicate,
    ReferenceCountOverflow,
    ReferenceQuantityInvalid,
    ReferenceBindingCount,
    ReferenceTotalQuantity,
    PinnedTargetMissing,
    RootPinnedClosure,
    ReferenceTargetMissing,
    IdentityTargetType,
    ResolvedNodeMissing,
    ItemDefinitionInvalid,
    ItemDefinitionHashInvalid,
}

impl CompositionPlanFailureDetails {
    fn reason(reason_code: CompositionPlanFailureReason) -> Self {
        Self {
            reason_code,
            expected_count: None,
            actual_count: None,
            item_id: None,
            item_type: None,
            slot_id: None,
        }
    }

    fn counts(
        reason_code: CompositionPlanFailureReason,
        expected_count: u32,
        actual_count: u32,
    ) -> Self {
        Self {
            expected_count: Some(expected_count),
            actual_count: Some(actual_count),
            ..Self::reason(reason_code)
        }
    }

    fn with_item(mut self, item_id: &ItemId, item_type: &ItemTypeId) -> Self {
        self.item_id = Some(item_id.clone());
        self.item_type = Some(item_type.clone());
        self
    }

    fn with_slot(mut self, slot_id: &ItemReferenceSlotId) -> Self {
        self.slot_id = Some(slot_id.clone());
        self
    }
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
    pub model_request_limits: ats_runtime::ModelRequestLimits,
}

pub struct CompositionRetryNodeContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub plan_contributions: &'a VerifiedContributionSet,
    pub retry_contributions: &'a VerifiedContributionSet,
    pub truth: &'a VerifiedTruthSnapshot,
    pub project_context: Option<&'a str>,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
    pub model_request_limits: ats_runtime::ModelRequestLimits,
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
    staged_recipes: StagedRecipes,
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
        Ok(Self {
            recipe,
            staged_recipes: StagedRecipes::built_in()?,
        })
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
        let evidence = query_evidence(context.truth, context.pack, &guidance.allowed_item_types())?;
        check_cancelled(cancellation)?;

        let profile = ItemCompositionProfile {
            composition_id: request.composition_id.clone(),
            source: request.source.clone(),
            parameters: request.parameters.clone(),
        };
        let resolved_profile = resolve_composition_profile(profile_set, guidance, &profile)?;
        let output_contract =
            composition_plan_output_contract(context.pack, guidance, &resolved_profile)?;
        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&output_contract.json_schema)?,
            ),
            ("pack.contribution".into(), serialize(guidance)?),
            ("truth.evidence".into(), serialize(&evidence)?),
            ("composition.profile".into(), serialize(&resolved_profile)?),
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
        let model_request = self.recipe.render_with_output_contract(
            &slots,
            context.model,
            output_contract,
            &context.model_request_limits,
        )?;
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
        let planned: ModelCompositionPlan =
            serde_json::from_str(&response.content).map_err(|_| {
                CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
                    CompositionPlanFailureReason::JsonDecode,
                ))
            })?;
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
                .ok_or_else(|| {
                    CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
                        CompositionPlanFailureReason::RootProfileMissing,
                    ))
                })?,
            profile,
            nodes,
            Utc::now(),
        )
        .map_err(|_| {
            CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
                CompositionPlanFailureReason::DraftGraphInvalid,
            ))
        })?;
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
            node_count: u32::try_from(draft.nodes.len()).map_err(|_| {
                CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
                    CompositionPlanFailureReason::NodeCountOverflow,
                ))
            })?,
            model_request_sha256: snapshot.request_sha256().clone(),
            execution_graph_id: None,
            validated_content_digest: None,
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
        let model_request =
            self.recipe
                .render(&slots, context.model, &context.model_request_limits)?;
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
                || entry.guidance.is_empty()
                || entry.guidance.len() > 64
                || entry.guidance.iter().any(|value| !valid_text(value, 2_000))
                || entry.coordination.suite_brief.guidance.len() > 32
                || entry
                    .coordination
                    .suite_brief
                    .guidance
                    .iter()
                    .any(|value| !valid_text(value, 2_000))
                || (!entry.coordination.suite_brief.enabled
                    && !entry.coordination.suite_brief.guidance.is_empty())
                || !valid_node_groups(pack, profile, entry)
                || !valid_binding_rules(pack, profile, entry)
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

fn valid_binding_rules(
    pack: &LoadedGamePack,
    profile: &CompositionProfileSet,
    guidance: &CompositionPlanGuidance,
) -> bool {
    if guidance.binding_rules.len() > 128 {
        return false;
    }
    let groups = guidance
        .node_groups
        .iter()
        .map(|group| (group.id.as_str(), group))
        .collect::<BTreeMap<_, _>>();
    let mut identities = BTreeSet::new();
    guidance.binding_rules.iter().all(|rule| {
        let Some(source_group) = groups.get(rule.source_group_id.as_str()) else {
            return false;
        };
        let Some(descriptor) = pack.item_type(&source_group.item_type) else {
            return false;
        };
        let Some(slot) = descriptor
            .reference_slots()
            .iter()
            .find(|slot| slot.id() == &rule.slot_id)
        else {
            return false;
        };
        let targets = rule
            .target_group_ids
            .iter()
            .filter_map(|id| groups.get(id.as_str()))
            .collect::<Vec<_>>();
        !rule.target_group_ids.is_empty()
            && targets.len() == rule.target_group_ids.len()
            && rule.target_group_ids.iter().collect::<BTreeSet<_>>().len()
                == rule.target_group_ids.len()
            && targets
                .iter()
                .all(|group| slot.allowed_item_types().contains(&group.item_type))
            && identities.insert((rule.source_group_id.as_str(), &rule.slot_id, rule.measure))
            && (rule.measure != ReferenceBindingMeasure::TotalQuantity
                || slot.kind() == ItemReferenceKind::Pinned)
            && match &rule.quantity_policy {
                CompositionQuantityPolicy::Constant { value } => {
                    (1..=slot.max_quantity()).contains(value)
                }
                CompositionQuantityPolicy::BriefDistribution {
                    total_parameter_id,
                    min_per_target,
                    max_per_target,
                } => {
                    rule.measure == ReferenceBindingMeasure::TotalQuantity
                        && slot.kind() == ItemReferenceKind::Pinned
                        && guidance.coordination.suite_brief.enabled
                        && min_per_target <= max_per_target
                        && *min_per_target >= slot.min_quantity()
                        && *max_per_target <= slot.max_quantity()
                        && profile
                            .parameters()
                            .iter()
                            .any(|parameter| parameter.id() == total_parameter_id)
                }
            }
    })
}

fn valid_node_groups(
    pack: &LoadedGamePack,
    profile: &CompositionProfileSet,
    guidance: &CompositionPlanGuidance,
) -> bool {
    if guidance.node_groups.is_empty() || guidance.node_groups.len() > 64 {
        return false;
    }
    let ids = guidance
        .node_groups
        .iter()
        .map(|group| group.id.as_str())
        .collect::<BTreeSet<_>>();
    if ids.len() != guidance.node_groups.len()
        || guidance.node_groups.iter().any(|group| {
            !valid_group_id(&group.id)
                || pack.item_type(&group.item_type).is_none()
                || group.count.base_count > 128
                || group.count.multiplier > 128
                || (group.count.parameter_id.is_none() && group.count.multiplier != 0)
                || group.count.parameter_id.as_ref().is_some_and(|id| {
                    !profile
                        .parameters()
                        .iter()
                        .any(|parameter| parameter.id() == id)
                        || group.count.multiplier == 0
                })
                || group
                    .depends_on_group_ids
                    .iter()
                    .any(|dependency| dependency == &group.id || !ids.contains(dependency.as_str()))
                || group
                    .depends_on_group_ids
                    .iter()
                    .collect::<BTreeSet<_>>()
                    .len()
                    != group.depends_on_group_ids.len()
        })
        || group_dependencies_are_cyclic(&guidance.node_groups)
    {
        return false;
    }
    let base_count = guidance
        .node_groups
        .iter()
        .try_fold(0_u32, |sum, group| sum.checked_add(group.count.base_count));
    if base_count != Some(profile.base_node_count())
        || guidance
            .node_groups
            .iter()
            .filter(|group| &group.item_type == profile.root_item_type())
            .map(|group| group.count.base_count)
            .sum::<u32>()
            == 0
    {
        return false;
    }
    profile.parameters().iter().all(|parameter| {
        guidance
            .node_groups
            .iter()
            .filter(|group| group.count.parameter_id.as_ref() == Some(parameter.id()))
            .map(|group| group.count.multiplier)
            .sum::<u32>()
            == parameter.node_weight()
    })
}

fn group_dependencies_are_cyclic(groups: &[CompositionNodeGroup]) -> bool {
    let mut pending = groups
        .iter()
        .map(|group| (group.id.as_str(), group.depends_on_group_ids.len()))
        .collect::<BTreeMap<_, _>>();
    let mut ready = pending
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect::<BTreeSet<_>>();
    let mut visited = 0_usize;
    while let Some(id) = ready.pop_first() {
        visited += 1;
        for group in groups
            .iter()
            .filter(|group| group.depends_on_group_ids.iter().any(|value| value == id))
        {
            let Some(count) = pending.get_mut(group.id.as_str()) else {
                return true;
            };
            *count -= 1;
            if *count == 0 {
                ready.insert(group.id.as_str());
            }
        }
    }
    visited != groups.len()
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

fn resolve_composition_profile<'a>(
    profile_set: &'a CompositionProfileSet,
    guidance: &CompositionPlanGuidance,
    profile: &'a ItemCompositionProfile,
) -> Result<ResolvedCompositionProfile<'a>, CompositionPlanError> {
    let expected_node_count = profile_set
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
    let node_counts = expected_node_type_counts(guidance, profile)?;
    if node_counts.values().copied().sum::<u32>() != expected_node_count {
        return Err(CompositionPlanError::InvalidPackGuidance);
    }
    let reference_counts = expected_reference_binding_counts(guidance, profile)?;
    Ok(ResolvedCompositionProfile {
        composition_id: profile_set.id(),
        root_item_type: profile_set.root_item_type(),
        max_nodes: profile_set.max_nodes(),
        selected_profile: profile,
        expected_node_count,
        expected_node_type_counts: node_counts
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(item_type, count)| ResolvedNodeTypeCount { item_type, count })
            .collect(),
        expected_reference_bindings: reference_counts
            .into_iter()
            .map(
                |((source_item_type, slot_id, measure), count)| ResolvedReferenceBindingCount {
                    source_item_type,
                    slot_id,
                    measure,
                    count,
                },
            )
            .collect(),
    })
}

fn expected_node_type_counts(
    guidance: &CompositionPlanGuidance,
    profile: &ItemCompositionProfile,
) -> Result<BTreeMap<ItemTypeId, u32>, CompositionPlanError> {
    let mut expected = BTreeMap::new();
    for group in &guidance.node_groups {
        let parameter_count = group
            .count
            .parameter_id
            .as_ref()
            .and_then(|id| profile.parameters.get(id))
            .copied()
            .unwrap_or_default();
        let count = group
            .count
            .base_count
            .checked_add(parameter_count.saturating_mul(group.count.multiplier))
            .ok_or(CompositionPlanError::InvalidProfile)?;
        let current = expected.entry(group.item_type.clone()).or_insert(0_u32);
        *current = current
            .checked_add(count)
            .ok_or(CompositionPlanError::InvalidProfile)?;
    }
    Ok(expected)
}

fn expected_reference_binding_counts(
    guidance: &CompositionPlanGuidance,
    profile: &ItemCompositionProfile,
) -> Result<
    BTreeMap<(ItemTypeId, ItemReferenceSlotId, ReferenceBindingMeasure), u32>,
    CompositionPlanError,
> {
    let mut expected = BTreeMap::new();
    for rule in &guidance.binding_rules {
        let source_group = guidance
            .node_groups
            .iter()
            .find(|group| group.id == rule.source_group_id)
            .ok_or(CompositionPlanError::InvalidPackGuidance)?;
        let target_count = rule.target_group_ids.iter().try_fold(0_u32, |total, id| {
            let group = guidance
                .node_groups
                .iter()
                .find(|group| &group.id == id)
                .ok_or(CompositionPlanError::InvalidPackGuidance)?;
            total
                .checked_add(resolve_group_count(group, profile)?)
                .ok_or(CompositionPlanError::InvalidProfile)
        })?;
        let count = match (&rule.measure, &rule.quantity_policy) {
            (ReferenceBindingMeasure::Bindings, _) => target_count,
            (
                ReferenceBindingMeasure::TotalQuantity,
                CompositionQuantityPolicy::Constant { value },
            ) => target_count
                .checked_mul(*value)
                .ok_or(CompositionPlanError::InvalidProfile)?,
            (
                ReferenceBindingMeasure::TotalQuantity,
                CompositionQuantityPolicy::BriefDistribution {
                    total_parameter_id, ..
                },
            ) => profile
                .parameters
                .get(total_parameter_id)
                .copied()
                .ok_or(CompositionPlanError::InvalidProfile)?,
        };
        let key = (
            source_group.item_type.clone(),
            rule.slot_id.clone(),
            rule.measure,
        );
        match expected.get(&key) {
            Some(existing) if *existing != count => {
                return Err(CompositionPlanError::InvalidPackGuidance);
            }
            Some(_) => {}
            None => {
                expected.insert(key, count);
            }
        }
    }
    Ok(expected)
}

fn resolve_group_count(
    group: &CompositionNodeGroup,
    profile: &ItemCompositionProfile,
) -> Result<u32, CompositionPlanError> {
    let parameter_count = group
        .count
        .parameter_id
        .as_ref()
        .and_then(|id| profile.parameters.get(id))
        .copied()
        .unwrap_or_default();
    group
        .count
        .base_count
        .checked_add(parameter_count.saturating_mul(group.count.multiplier))
        .ok_or(CompositionPlanError::InvalidProfile)
}

fn composition_plan_output_contract(
    pack: &LoadedGamePack,
    guidance: &CompositionPlanGuidance,
    resolved: &ResolvedCompositionProfile<'_>,
) -> Result<ModelOutputContract, CompositionPlanError> {
    let binding_counts = resolved
        .expected_reference_bindings
        .iter()
        .filter(|rule| rule.measure == ReferenceBindingMeasure::Bindings)
        .map(|rule| {
            (
                (rule.source_item_type.clone(), rule.slot_id.clone()),
                rule.count,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let node_variants = resolved
        .expected_node_type_counts
        .iter()
        .map(|expected| {
            let descriptor = pack
                .item_type(&expected.item_type)
                .ok_or(CompositionPlanError::InvalidPackGuidance)?;
            composition_node_schema(descriptor, &binding_counts)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if node_variants.is_empty()
        || resolved.expected_node_count == 0
        || resolved.expected_node_count > resolved.max_nodes
        || resolved
            .expected_node_type_counts
            .iter()
            .any(|count| !guidance.allowed_item_types().contains(&count.item_type))
    {
        return Err(CompositionPlanError::InvalidPackGuidance);
    }
    Ok(ModelOutputContract {
        schema: model_output_schema(),
        json_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["rootItemId", "nodes"],
            "properties": {
                "rootItemId": item_id_schema(),
                "nodes": {
                    "type": "array",
                    "minItems": resolved.expected_node_count,
                    "maxItems": resolved.expected_node_count,
                    "items": { "oneOf": node_variants }
                }
            }
        }),
    })
}

fn composition_node_schema(
    descriptor: &ItemTypeDescriptor,
    binding_counts: &BTreeMap<(ItemTypeId, ItemReferenceSlotId), u32>,
) -> Result<serde_json::Value, CompositionPlanError> {
    let field_properties = descriptor
        .fields()
        .iter()
        .map(|field| {
            (
                field.id().as_str().into(),
                item_field_value_schema(field.value()),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let required_fields = descriptor
        .fields()
        .iter()
        .filter(|field| field.required())
        .map(|field| field.id().as_str())
        .collect::<Vec<_>>();

    let localization_fields = descriptor.localization_fields().iter().collect::<Vec<_>>();
    let required_localization_fields = descriptor
        .localization_fields()
        .iter()
        .filter(|field| field.required())
        .collect::<Vec<_>>();
    let localization_properties = descriptor
        .required_locales()
        .iter()
        .map(|locale| {
            let properties = localization_fields
                .iter()
                .map(|field| {
                    (
                        field.id().as_str().into(),
                        serde_json::json!({
                            "type": "string",
                            "minLength": field.min_length(),
                            "maxLength": field.max_length()
                        }),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            let required = required_localization_fields
                .iter()
                .map(|field| field.id().as_str())
                .collect::<Vec<_>>();
            (
                locale.as_str().into(),
                serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "required": required,
                    "properties": properties
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let required_locales = descriptor
        .required_locales()
        .iter()
        .map(|locale| locale.as_str())
        .collect::<Vec<_>>();

    let mut reference_properties = serde_json::Map::new();
    let mut required_reference_slots = Vec::new();
    for slot in descriptor.reference_slots() {
        let exact = binding_counts.get(&(descriptor.id().clone(), slot.id().clone()));
        let (min_items, max_items) = exact
            .copied()
            .map(|count| (count, count))
            .unwrap_or((slot.min_items(), slot.max_items()));
        if max_items == 0 {
            continue;
        }
        let item_schema = match slot.kind() {
            ItemReferenceKind::Identity => serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "itemId", "expectedItemType"],
                "properties": {
                    "kind": { "const": "identity" },
                    "itemId": item_id_schema(),
                    "expectedItemType": {
                        "type": "string",
                        "enum": slot.allowed_item_types().iter().map(|value| value.as_str()).collect::<Vec<_>>()
                    }
                }
            }),
            ItemReferenceKind::Pinned => serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["kind", "itemId", "quantity"],
                "properties": {
                    "kind": { "const": "pinned" },
                    "itemId": item_id_schema(),
                    "quantity": {
                        "type": "integer",
                        "minimum": slot.min_quantity(),
                        "maximum": slot.max_quantity()
                    }
                }
            }),
        };
        reference_properties.insert(
            slot.id().as_str().into(),
            serde_json::json!({
                "type": "array",
                "minItems": min_items,
                "maxItems": max_items,
                "items": item_schema
            }),
        );
        if min_items > 0 {
            required_reference_slots.push(slot.id().as_str());
        }
    }

    Ok(serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "itemId",
            "itemType",
            "canonicalFields",
            "behaviorIntent",
            "localizations",
            "referenceBindings"
        ],
        "properties": {
            "itemId": item_id_schema(),
            "itemType": { "const": descriptor.id().as_str() },
            "canonicalFields": {
                "type": "object",
                "additionalProperties": false,
                "required": required_fields,
                "properties": field_properties
            },
            "behaviorIntent": {
                "type": "array",
                "minItems": 1,
                "maxItems": 32,
                "items": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 1_000,
                    "pattern": "^[^\\u0000]*\\S[^\\u0000]*$"
                }
            },
            "localizations": {
                "type": "object",
                "additionalProperties": false,
                "required": required_locales,
                "properties": localization_properties
            },
            "referenceBindings": {
                "type": "object",
                "additionalProperties": false,
                "required": required_reference_slots,
                "properties": reference_properties
            }
        }
    }))
}

fn item_field_value_schema(spec: &ItemFieldValueSpec) -> serde_json::Value {
    match spec {
        ItemFieldValueSpec::Text {
            min_length,
            max_length,
            ..
        } => serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "value"],
            "properties": {
                "kind": { "const": "text" },
                "value": { "type": "string", "minLength": min_length, "maxLength": max_length }
            }
        }),
        ItemFieldValueSpec::Integer { min, max } => serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "value"],
            "properties": {
                "kind": { "const": "integer" },
                "value": { "type": "integer", "minimum": min, "maximum": max }
            }
        }),
        ItemFieldValueSpec::Boolean => serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "value"],
            "properties": {
                "kind": { "const": "boolean" },
                "value": { "type": "boolean" }
            }
        }),
        ItemFieldValueSpec::Choice { options } => serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "value"],
            "properties": {
                "kind": { "const": "choice" },
                "value": {
                    "type": "string",
                    "enum": options.iter().map(|option| option.value()).collect::<Vec<_>>()
                }
            }
        }),
        ItemFieldValueSpec::StringList {
            min_items,
            max_items,
            item_max_length,
        } => serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "value"],
            "properties": {
                "kind": { "const": "string_list" },
                "value": {
                    "type": "array",
                    "minItems": min_items,
                    "maxItems": max_items,
                    "items": { "type": "string", "maxLength": item_max_length }
                }
            }
        }),
    }
}

fn item_id_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "string",
        "pattern": "^[a-z][a-z0-9_-]*$",
        "maxLength": 128
    })
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
        return Err(CompositionPlanError::ProfileCountMismatch(
            CompositionPlanFailureDetails::counts(
                CompositionPlanFailureReason::NodeTotal,
                expected_count,
                u32::try_from(planned.nodes.len()).unwrap_or(u32::MAX),
            ),
        ));
    }
    let root_id = planned.root_item_id;
    let mut source = BTreeMap::new();
    for node in planned.nodes {
        if !guidance.allowed_item_types().contains(&node.item_type) {
            return Err(CompositionPlanError::InvalidModelOutput(
                CompositionPlanFailureDetails::reason(
                    CompositionPlanFailureReason::ItemTypeUnsupported,
                )
                .with_item(&node.item_id, &node.item_type),
            ));
        }
        let item_id = node.item_id.clone();
        let item_type = node.item_type.clone();
        if source.insert(item_id.clone(), node).is_some() {
            return Err(CompositionPlanError::InvalidModelOutput(
                CompositionPlanFailureDetails::reason(
                    CompositionPlanFailureReason::ItemIdDuplicate,
                )
                .with_item(&item_id, &item_type),
            ));
        }
    }
    if source
        .get(&root_id)
        .is_none_or(|node| &node.item_type != profile_set.root_item_type())
    {
        return Err(CompositionPlanError::InvalidModelOutput(
            CompositionPlanFailureDetails::reason(
                CompositionPlanFailureReason::RootMissingOrWrongType,
            ),
        ));
    }
    let actual_counts = source.values().fold(BTreeMap::new(), |mut counts, node| {
        *counts.entry(node.item_type.clone()).or_insert(0_u32) += 1;
        counts
    });
    let mut expected_counts = expected_node_type_counts(guidance, profile)?;
    expected_counts.retain(|_, count| *count > 0);
    if actual_counts != expected_counts {
        let mismatched_type = actual_counts
            .keys()
            .chain(expected_counts.keys())
            .find(|item_type| actual_counts.get(*item_type) != expected_counts.get(*item_type))
            .cloned();
        let expected = mismatched_type
            .as_ref()
            .and_then(|item_type| expected_counts.get(item_type))
            .copied()
            .unwrap_or_default();
        let actual = mismatched_type
            .as_ref()
            .and_then(|item_type| actual_counts.get(item_type))
            .copied()
            .unwrap_or_default();
        let mut details = CompositionPlanFailureDetails::counts(
            CompositionPlanFailureReason::ItemTypeCount,
            expected,
            actual,
        );
        details.item_type = mismatched_type;
        return Err(CompositionPlanError::ProfileCountMismatch(details));
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
    let expected = expected_reference_binding_counts(guidance, profile)?;

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
                return Err(CompositionPlanError::InvalidModelOutput(
                    CompositionPlanFailureDetails::reason(
                        CompositionPlanFailureReason::ReferenceTargetDuplicate,
                    )
                    .with_item(&node.item_id, &node.item_type)
                    .with_slot(&slot_id),
                ));
            }
            let actual = match measure {
                ReferenceBindingMeasure::Bindings => {
                    u32::try_from(bindings.len()).map_err(|_| {
                        CompositionPlanError::InvalidModelOutput(
                            CompositionPlanFailureDetails::reason(
                                CompositionPlanFailureReason::ReferenceCountOverflow,
                            )
                            .with_item(&node.item_id, &node.item_type)
                            .with_slot(&slot_id),
                        )
                    })?
                }
                ReferenceBindingMeasure::TotalQuantity => bindings
                    .iter()
                    .try_fold(0_u32, |sum, binding| match binding {
                        PlannedReference::Pinned { quantity, .. } => sum.checked_add(*quantity),
                        PlannedReference::Identity { .. } => None,
                    })
                    .ok_or_else(|| {
                        CompositionPlanError::InvalidModelOutput(
                            CompositionPlanFailureDetails::reason(
                                CompositionPlanFailureReason::ReferenceQuantityInvalid,
                            )
                            .with_item(&node.item_id, &node.item_type)
                            .with_slot(&slot_id),
                        )
                    })?,
            };
            if actual != expected_count {
                let reason = match measure {
                    ReferenceBindingMeasure::Bindings => {
                        CompositionPlanFailureReason::ReferenceBindingCount
                    }
                    ReferenceBindingMeasure::TotalQuantity => {
                        CompositionPlanFailureReason::ReferenceTotalQuantity
                    }
                };
                return Err(CompositionPlanError::ProfileCountMismatch(
                    CompositionPlanFailureDetails::counts(reason, expected_count, actual)
                        .with_item(&node.item_id, &node.item_type)
                        .with_slot(&slot_id),
                ));
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
        let node = nodes.get(item_id).ok_or_else(|| {
            CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
                CompositionPlanFailureReason::PinnedTargetMissing,
            ))
        })?;
        for binding in node.reference_bindings.values().flatten() {
            if let PlannedReference::Pinned { item_id, .. } = binding {
                pending.push(item_id);
            }
        }
    }
    if visited.len() != nodes.len() {
        return Err(CompositionPlanError::InvalidModelOutput(
            CompositionPlanFailureDetails::counts(
                CompositionPlanFailureReason::RootPinnedClosure,
                u32::try_from(nodes.len()).unwrap_or(u32::MAX),
                u32::try_from(visited.len()).unwrap_or(u32::MAX),
            ),
        ));
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
                let target = nodes.get(target_id).ok_or_else(|| {
                    CompositionPlanError::InvalidModelOutput(
                        CompositionPlanFailureDetails::reason(
                            CompositionPlanFailureReason::ReferenceTargetMissing,
                        )
                        .with_item(&node.item_id, &node.item_type),
                    )
                })?;
                if expected_type.is_some_and(|value| value != &target.item_type) {
                    return Err(CompositionPlanError::InvalidModelOutput(
                        CompositionPlanFailureDetails::reason(
                            CompositionPlanFailureReason::IdentityTargetType,
                        )
                        .with_item(&node.item_id, &node.item_type),
                    ));
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
    let node = source.get(item_id).ok_or_else(|| {
        CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
            CompositionPlanFailureReason::ResolvedNodeMissing,
        ))
    })?;
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
        .map_err(|_| {
            CompositionPlanError::InvalidModelOutput(
                CompositionPlanFailureDetails::reason(
                    CompositionPlanFailureReason::ItemDefinitionInvalid,
                )
                .with_item(&node.item_id, &node.item_type),
            )
        })?;
    let stored = StoredItemDefinition {
        definition_hash: definition.definition_hash().map_err(|_| {
            CompositionPlanError::InvalidModelOutput(
                CompositionPlanFailureDetails::reason(
                    CompositionPlanFailureReason::ItemDefinitionHashInvalid,
                )
                .with_item(&node.item_id, &node.item_type),
            )
        })?,
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
    ProfileCountMismatch(CompositionPlanFailureDetails),
    #[error("composition planning Recipe is invalid")]
    InvalidRecipeContract,
    #[error("composition model output was truncated")]
    TruncatedModelOutput,
    #[error("composition model output is invalid")]
    InvalidModelOutput(CompositionPlanFailureDetails),
    #[error("composition pinned references contain a cycle")]
    PinnedCycle,
    #[error("composition planning Truth evidence is missing")]
    MissingEvidence,
    #[error("composition Draft identity already exists")]
    DraftConflict,
    #[error("composition Draft storage failed")]
    DraftStorage,
    #[error("composition execution graph revision changed")]
    ExecutionGraphConflict,
    #[error("composition execution graph storage failed")]
    ExecutionGraphStorage,
    #[error("composition commit conflicts with an existing Draft")]
    CommitConflict,
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
        let details = match self {
            Self::ProfileCountMismatch(details) | Self::InvalidModelOutput(details) => Some(
                VersionedPayload::from_typed(composition_plan_failure_details_schema(), details)
                    .expect("built-in composition failure details are valid"),
            ),
            _ => None,
        };
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
            Self::ProfileCountMismatch(_) => (
                "composition.profile.count_mismatch",
                "composition.plan.model",
            ),
            Self::InvalidRecipeContract | Self::Recipe(_) => {
                ("feature.recipe_invalid", "composition.plan.recipe")
            }
            Self::TruncatedModelOutput => ("model.output_truncated", "composition.plan.model"),
            Self::InvalidModelOutput(_) => ("model.output_invalid", "composition.plan.model"),
            Self::PinnedCycle => ("composition.graph.cycle", "composition.plan.graph"),
            Self::MissingEvidence => ("truth.evidence_missing", "composition.plan.truth"),
            Self::Evidence(_) => ("truth.query_invalid", "composition.plan.truth"),
            Self::DraftConflict => ("composition.draft.conflict", "composition.plan.persist"),
            Self::DraftStorage => (
                "composition.draft.storage_failed",
                "composition.plan.persist",
            ),
            Self::ExecutionGraphConflict => (
                "composition.execution.conflict",
                "composition.plan.execution",
            ),
            Self::ExecutionGraphStorage => (
                "composition.execution.storage_failed",
                "composition.plan.execution",
            ),
            Self::CommitConflict => ("composition.commit.conflict", "composition.plan.commit"),
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
            details,
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
        CompositionPlanError::ProfileCountMismatch(_) => {
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

fn composition_plan_failure_details_schema() -> SchemaRef {
    schema("feature.composition-plan-failure-details")
}

fn retry_model_output_schema() -> SchemaRef {
    schema("feature.composition-retry-node-model-output")
}

fn schema(id: &str) -> SchemaRef {
    schema_version(id, 1)
}

fn schema_version(id: &str, version: u32) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(version).expect("built-in schema version is valid"),
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

fn valid_group_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ats_game_context::{
        ContributionResolver, GamePackLoader, TruthSnapshotIndex, TruthSnapshotManifest,
        TruthSnapshotSource,
    };
    use ats_kernel::{CompositionParameterId, CompositionProfileId, PrimitiveId};
    use ats_runtime::{
        ExecutionGraphRecord, ExecutionGraphRecovery, ExecutionGraphRepository,
        ExecutionGraphRepositoryError, ExecutionGraphStatus, ModelResponse, ModelStream,
        RunRepository,
    };
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

        fn create_or_match(
            &self,
            draft: &CompositionDraft,
            expected_payload_sha256: &Sha256Digest,
        ) -> Result<ats_workspace::CompositionDraftCreateOrMatch, Self::Error> {
            if &draft.payload_sha256().map_err(|_| MemoryError)? != expected_payload_sha256 {
                return Err(MemoryError);
            }
            let mut stored = self.0.lock().unwrap();
            match stored.as_ref() {
                Some(existing) if existing == draft => {
                    Ok(ats_workspace::CompositionDraftCreateOrMatch::Matched)
                }
                Some(_) => Err(MemoryError),
                None => {
                    *stored = Some(draft.clone());
                    Ok(ats_workspace::CompositionDraftCreateOrMatch::Created)
                }
            }
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

    struct StagedModel {
        responses: Mutex<VecDeque<String>>,
        snapshots: Mutex<Vec<ModelRequestSnapshot>>,
    }

    #[async_trait]
    impl ModelClient for StagedModel {
        async fn complete(
            &self,
            request: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelResponse, ModelError> {
            self.snapshots.lock().unwrap().push(request);
            let content = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(ModelError::Transport)?;
            Ok(ModelResponse {
                model: "fixture-staged".into(),
                content,
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

    #[derive(Default)]
    struct MemoryGraphs(Mutex<Option<ExecutionGraphRecord>>);

    impl ExecutionGraphRepository for MemoryGraphs {
        fn create_claimed(
            &self,
            graph: &ExecutionGraphRecord,
            run_id: &ats_runtime::RunId,
        ) -> Result<(), ExecutionGraphRepositoryError> {
            if graph.active_run_id() != Some(run_id) {
                return Err(ExecutionGraphRepositoryError::InvalidRecord);
            }
            let mut stored = self.0.lock().unwrap();
            if stored.is_some() {
                return Err(ExecutionGraphRepositoryError::AlreadyExists);
            }
            *stored = Some(graph.clone());
            Ok(())
        }

        fn get(
            &self,
            id: &ExecutionGraphId,
        ) -> Result<ExecutionGraphRecord, ExecutionGraphRepositoryError> {
            self.0
                .lock()
                .unwrap()
                .clone()
                .filter(|graph| graph.id() == id)
                .ok_or(ExecutionGraphRepositoryError::NotFound)
        }

        fn compare_and_set(
            &self,
            expected_revision: u64,
            next: &ExecutionGraphRecord,
        ) -> Result<(), ExecutionGraphRepositoryError> {
            let mut stored = self.0.lock().unwrap();
            if stored.as_ref().is_none_or(|graph| {
                graph.id() != next.id()
                    || graph.revision() != expected_revision
                    || next.revision() != expected_revision + 1
            }) {
                return Err(ExecutionGraphRepositoryError::Conflict);
            }
            *stored = Some(next.clone());
            Ok(())
        }

        fn list(&self) -> Result<Vec<ExecutionGraphRecord>, ExecutionGraphRepositoryError> {
            Ok(self.0.lock().unwrap().clone().into_iter().collect())
        }

        fn recover_structure(
            &self,
            _: &dyn RunRepository,
        ) -> Result<ExecutionGraphRecovery, ExecutionGraphRepositoryError> {
            Ok(ExecutionGraphRecovery::default())
        }
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
                "schema":{"id":"pack.composition-plan-guidance","version":2},
                "payload":{"compositions":[{
                    "compositionId":"fixture_suite",
                    "nodeGroups":[
                        {"id":"root","itemType":"root","count":{"baseCount":1,"multiplier":0},"dependsOnGroupIds":["children"],"generationKind":"model_item"},
                        {"id":"children","itemType":"child","count":{"baseCount":0,"parameterId":"child_count","multiplier":1},"generationKind":"model_item"}
                    ],
                    "coordination":{"suiteBrief":{"enabled":true,"guidance":["Coordinate the fixture nodes."]}},
                    "bindingRules":[
                        {"sourceGroupId":"root","slotId":"children","targetGroupIds":["children"],"measure":"bindings","quantityPolicy":{"kind":"constant","value":1}},
                        {"sourceGroupId":"children","slotId":"owner","targetGroupIds":["root"],"measure":"bindings","quantityPolicy":{"kind":"constant","value":1}}
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
            Err(CompositionPlanError::InvalidModelOutput(_))
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
            node_groups: vec![
                CompositionNodeGroup {
                    id: "root".into(),
                    item_type: ItemTypeId::parse("root").unwrap(),
                    count: CompositionCountRule {
                        base_count: 1,
                        parameter_id: None,
                        multiplier: 0,
                    },
                    depends_on_group_ids: Vec::new(),
                    generation_kind: CompositionGenerationKind::ModelItem,
                },
                CompositionNodeGroup {
                    id: "children".into(),
                    item_type: ItemTypeId::parse("child").unwrap(),
                    count: CompositionCountRule {
                        base_count: 2,
                        parameter_id: None,
                        multiplier: 0,
                    },
                    depends_on_group_ids: Vec::new(),
                    generation_kind: CompositionGenerationKind::ModelItem,
                },
            ],
            coordination: CompositionCoordination::default(),
            binding_rules: vec![CompositionBindingRule {
                source_group_id: "root".into(),
                slot_id: ItemReferenceSlotId::parse("children").unwrap(),
                target_group_ids: vec!["children".into()],
                measure: ReferenceBindingMeasure::TotalQuantity,
                quantity_policy: CompositionQuantityPolicy::Constant { value: 5 },
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
            Err(CompositionPlanError::InvalidModelOutput(_))
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
                    execution: None,
                },
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: Some("Fixture project"),
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ats_runtime::ModelRequestLimits::default(),
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
        let snapshots = model.snapshots.lock().unwrap();
        let request = snapshots[0].request();
        assert_eq!(
            request.output_contract.json_schema["properties"]["nodes"]["minItems"],
            serde_json::json!(2)
        );
        assert_eq!(
            request.output_contract.json_schema["properties"]["nodes"]["maxItems"],
            serde_json::json!(2)
        );
        assert_eq!(
            request.output_contract.json_schema["properties"]["nodes"]["items"]["oneOf"]
                .as_array()
                .map(Vec::len),
            Some(2)
        );
        let prompt = request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(prompt.contains("\"expectedNodeCount\": 2"));
        assert!(prompt.contains("\"expectedNodeTypeCounts\""));
    }

    #[tokio::test]
    async fn staged_plan_resumes_only_the_failed_node_and_commits_exact_draft() {
        let pack = pack();
        let truth = truth(&pack);
        let contributions = ContributionResolver::new(Vec::<PrimitiveId>::new())
            .resolve(
                &pack,
                &CompositionPlanFeature::id(),
                &[CompositionPlanFeature::contribution_requirement()],
            )
            .unwrap();
        let service = CompositionPlanService::built_in().unwrap();
        let request = CompositionPlanRequest {
            draft_id: CompositionDraftId::parse("staged-fixture-draft").unwrap(),
            composition_id: CompositionId::parse("fixture_suite").unwrap(),
            concept: "Create one recoverable fixture suite.".into(),
            source: ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            },
            parameters: BTreeMap::from([(
                CompositionParameterId::parse("child_count").unwrap(),
                1,
            )]),
            execution: None,
        };
        let first_run = ats_runtime::RunId::parse("run-staged-first").unwrap();
        let start = service
            .prepare_staged_start(
                request,
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ats_runtime::ModelRequestLimits::new(Some(4_096))
                        .unwrap(),
                },
                first_run.clone(),
            )
            .unwrap();
        let graph_id = start.graph.id().clone();
        let graphs = MemoryGraphs::default();
        graphs.create_claimed(&start.graph, &first_run).unwrap();
        let drafts = MemoryDrafts::default();
        let first_model = StagedModel {
            responses: Mutex::new(VecDeque::from([
                serde_json::json!({
                    "theme":"A bounded fixture theme.",
                    "nodeResponsibilities":{
                        "item.generate:children:000":"Provide the child behavior.",
                        "item.generate:root:000":"Coordinate the root behavior."
                    },
                    "quantityDistributions":{}
                })
                .to_string(),
                serde_json::json!({
                    "canonicalFields":{},
                    "behaviorIntent":["Provide the child behavior."],
                    "localizations":{}
                })
                .to_string(),
            ])),
            snapshots: Mutex::new(Vec::new()),
        };
        let first_error = service
            .execute_staged(
                &first_model,
                &MemoryItems,
                &drafts,
                &graphs,
                &first_run,
                start.request,
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ats_runtime::ModelRequestLimits::default(),
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                first_error,
                CompositionPlanError::Model(ModelError::Transport)
            ),
            "unexpected staged failure after {} requests: {first_error:?}",
            first_model.snapshots.lock().unwrap().len()
        );
        assert_eq!(first_model.snapshots.lock().unwrap().len(), 3);
        assert!(
            first_model
                .snapshots
                .lock()
                .unwrap()
                .iter()
                .all(|snapshot| snapshot.request().max_output_tokens == 4_096)
        );
        let paused = graphs.get(&graph_id).unwrap();
        assert_eq!(paused.status(), ExecutionGraphStatus::Paused);
        assert!(drafts.0.lock().unwrap().is_none());

        let expected_revision = paused.revision();
        let second_run = ats_runtime::RunId::parse("run-staged-second").unwrap();
        let resumed = service
            .prepare_staged_resume(
                paused,
                expected_revision,
                second_run.clone(),
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ats_runtime::ModelRequestLimits::new(Some(1_024))
                        .unwrap(),
                },
            )
            .unwrap();
        graphs
            .compare_and_set(expected_revision, &resumed.graph)
            .unwrap();
        let second_model = StagedModel {
            responses: Mutex::new(VecDeque::from([serde_json::json!({
                "canonicalFields":{},
                "behaviorIntent":["Coordinate the root behavior."],
                "localizations":{}
            })
            .to_string()])),
            snapshots: Mutex::new(Vec::new()),
        };
        let outcome = service
            .execute_staged(
                &second_model,
                &MemoryItems,
                &drafts,
                &graphs,
                &second_run,
                resumed.request,
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ats_runtime::ModelRequestLimits::new(Some(1_024))
                        .unwrap(),
                },
                &CancellationToken::new(),
            )
            .await;
        assert!(
            outcome.is_ok(),
            "resume failed with {:?}; graph: {:?}",
            outcome.as_ref().err(),
            graphs.get(&graph_id).unwrap()
        );
        let execution = outcome.unwrap();
        assert_eq!(
            second_model.snapshots.lock().unwrap()[0]
                .request()
                .max_output_tokens,
            4_096
        );
        assert_eq!(second_model.snapshots.lock().unwrap().len(), 1);
        assert_eq!(execution.request_snapshots.len(), 1);
        assert_eq!(
            execution.draft.source_execution_graph_id,
            Some(graph_id.clone())
        );
        assert!(execution.draft.validated_content_digest.is_some());
        assert_eq!(
            graphs.get(&graph_id).unwrap().status(),
            ExecutionGraphStatus::Succeeded
        );
        assert_eq!(drafts.0.lock().unwrap().as_ref(), Some(&execution.draft));

        let committed_graph = graphs.get(&graph_id).unwrap();
        let committed_revision = committed_graph.revision();
        let committed_result = execution.result.clone();
        let committed_draft = execution.draft.clone();
        let reconciliation_run = ats_runtime::RunId::parse("run-staged-reconcile").unwrap();
        let reconciliation = service
            .prepare_staged_resume(
                committed_graph,
                committed_revision,
                reconciliation_run.clone(),
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ats_runtime::ModelRequestLimits::default(),
                },
            )
            .unwrap();
        assert_eq!(reconciliation.graph.revision(), committed_revision);
        let reconciliation_model = StagedModel {
            responses: Mutex::new(VecDeque::new()),
            snapshots: Mutex::new(Vec::new()),
        };
        let reconciled = service
            .execute_staged(
                &reconciliation_model,
                &MemoryItems,
                &drafts,
                &graphs,
                &reconciliation_run,
                reconciliation.request,
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ats_runtime::ModelRequestLimits::default(),
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(reconciliation_model.snapshots.lock().unwrap().is_empty());
        assert!(reconciled.request_snapshots.is_empty());
        assert_eq!(reconciled.result, committed_result);
        assert_eq!(reconciled.draft, committed_draft);
        assert_eq!(
            graphs.get(&graph_id).unwrap().revision(),
            committed_revision
        );
    }

    #[tokio::test]
    async fn staged_plan_preflight_cancellation_preserves_pause_and_cancel_semantics() {
        let pack = pack();
        let truth = truth(&pack);
        let contributions = ContributionResolver::new(Vec::<PrimitiveId>::new())
            .resolve(
                &pack,
                &CompositionPlanFeature::id(),
                &[CompositionPlanFeature::contribution_requirement()],
            )
            .unwrap();
        let service = CompositionPlanService::built_in().unwrap();

        for (suffix, reason, expected_status) in [
            (
                "pause",
                ats_runtime::CancellationReason::Pause,
                ExecutionGraphStatus::Paused,
            ),
            (
                "cancel",
                ats_runtime::CancellationReason::User,
                ExecutionGraphStatus::Cancelled,
            ),
        ] {
            let run_id = ats_runtime::RunId::parse(format!("run-staged-{suffix}")).unwrap();
            let start = service
                .prepare_staged_start(
                    CompositionPlanRequest {
                        draft_id: CompositionDraftId::parse(format!(
                            "staged-{suffix}-fixture-draft"
                        ))
                        .unwrap(),
                        composition_id: CompositionId::parse("fixture_suite").unwrap(),
                        concept: "Exercise staged cancellation semantics.".into(),
                        source: ItemCompositionSource::Preset {
                            profile_id: CompositionProfileId::parse("standard").unwrap(),
                        },
                        parameters: BTreeMap::from([(
                            CompositionParameterId::parse("child_count").unwrap(),
                            1,
                        )]),
                        execution: None,
                    },
                    CompositionPlanContext {
                        pack: &pack,
                        contributions: &contributions,
                        truth: &truth,
                        project_context: None,
                        custom_instructions: None,
                        model: None,
                        model_request_limits: ats_runtime::ModelRequestLimits::default(),
                    },
                    run_id.clone(),
                )
                .unwrap();
            let graph_id = start.graph.id().clone();
            let graphs = MemoryGraphs::default();
            graphs.create_claimed(&start.graph, &run_id).unwrap();
            let model = StagedModel {
                responses: Mutex::new(VecDeque::new()),
                snapshots: Mutex::new(Vec::new()),
            };
            let cancellation = CancellationToken::new();
            assert!(cancellation.cancel(reason));

            let result = service
                .execute_staged(
                    &model,
                    &MemoryItems,
                    &MemoryDrafts::default(),
                    &graphs,
                    &run_id,
                    start.request,
                    CompositionPlanContext {
                        pack: &pack,
                        contributions: &contributions,
                        truth: &truth,
                        project_context: None,
                        custom_instructions: None,
                        model: None,
                        model_request_limits: ats_runtime::ModelRequestLimits::default(),
                    },
                    &cancellation,
                )
                .await;
            assert!(matches!(result, Err(CompositionPlanError::Cancelled)));
            assert!(model.snapshots.lock().unwrap().is_empty());
            assert_eq!(graphs.get(&graph_id).unwrap().status(), expected_status);
        }
    }

    #[test]
    fn composition_model_failures_persist_only_safe_versioned_details() {
        let failure = CompositionPlanError::ProfileCountMismatch(
            CompositionPlanFailureDetails::counts(
                CompositionPlanFailureReason::ReferenceTotalQuantity,
                10,
                9,
            )
            .with_item(
                &ItemId::parse("fixture-root").unwrap(),
                &ItemTypeId::parse("root").unwrap(),
            )
            .with_slot(&ItemReferenceSlotId::parse("children").unwrap()),
        )
        .run_failure();

        assert_eq!(failure.code.as_str(), "composition.profile.count_mismatch");
        assert_eq!(
            failure.details.as_ref().map(VersionedPayload::schema),
            Some(&composition_plan_failure_details_schema())
        );
        assert_eq!(
            failure.details.as_ref().map(VersionedPayload::payload),
            Some(&serde_json::json!({
                "reasonCode": "reference_total_quantity",
                "expectedCount": 10,
                "actualCount": 9,
                "itemId": "fixture-root",
                "itemType": "root",
                "slotId": "children"
            }))
        );
    }

    #[test]
    fn sts2_prototype_compiles_to_v2_stable_groups_and_eleven_business_nodes() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let contributions = ContributionResolver::new(Vec::<PrimitiveId>::new())
            .resolve(
                &pack,
                &CompositionPlanFeature::id(),
                &[CompositionPlanFeature::contribution_requirement()],
            )
            .unwrap();
        let contribution: CompositionPlanContribution =
            contributions.decode(&contribution_slot()).unwrap();
        let guidance = contribution
            .compositions
            .iter()
            .find(|value| value.composition_id.as_str() == "character_suite")
            .unwrap();
        let profile_set = pack
            .composition_profile(&CompositionId::parse("character_suite").unwrap())
            .unwrap();
        let prototype = profile_set
            .profiles()
            .iter()
            .find(|profile| profile.id().as_str() == "prototype")
            .unwrap();
        let profile = ItemCompositionProfile {
            composition_id: profile_set.id().clone(),
            source: ItemCompositionSource::Preset {
                profile_id: prototype.id().clone(),
            },
            parameters: prototype.values().clone(),
        };
        let resolved = resolve_composition_profile(profile_set, guidance, &profile).unwrap();
        assert_eq!(resolved.expected_node_count, 11);
        assert_eq!(guidance.node_groups.len(), 9);
        assert!(guidance.coordination.suite_brief.enabled);
        assert!(
            guidance
                .node_groups
                .iter()
                .any(|group| { group.id == "starter_cards" && group.item_type.as_str() == "card" })
        );
        assert!(guidance.binding_rules.iter().any(|rule| {
            rule.source_group_id == "character"
                && rule.slot_id.as_str() == "starting_deck"
                && rule.target_group_ids == ["starter_cards"]
                && matches!(
                    rule.quantity_policy,
                    CompositionQuantityPolicy::BriefDistribution { .. }
                )
        }));
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
                    execution: None,
                },
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &plan_contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ats_runtime::ModelRequestLimits::default(),
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
                    model_request_limits: ats_runtime::ModelRequestLimits::default(),
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
                    model_request_limits: ats_runtime::ModelRequestLimits::default(),
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
