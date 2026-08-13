use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ats_game_context::{
    ContributionResolverError, EvidenceQueryError, ItemTypeDescriptor, LoadedGamePack,
    TruthEvidenceRecord, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{
    ContributionId, FailureCode, FeatureId, ItemTypeId, PrimitiveId, ResourceId, SchemaId,
    SchemaRef, SchemaVersion, Sha256Digest,
};
use ats_runtime::{
    ArtifactFileInput, ArtifactPublishRequest, ArtifactPublisher, CancellationToken, FinishReason,
    ModelClient, ModelError, ModelGamePackRef, ModelOutputContract, ModelRequestError,
    ModelRequestSnapshot, ModelResourceRef, PayloadError, ProjectFileWrite, ProjectFileWriter,
    ProjectWriteError, PublishedArtifact, RunFailure, RunLifecycleError, RunRecord, RunStatus,
    RunTransition, TokenUsage, ValidationError, ValidationIssue, ValidationRequest,
    ValidationRunner, VersionedPayload,
};
use ats_workspace::{ItemResourceBinding, StoredItemDefinition};
use ats_workspace::{ResourceAsset, ResourceRepository};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;
use crate::generation_feedback::{
    ExpectedOutputShape, GenerationFeedbackEnvelope, GenerationFeedbackMode,
    GenerationFeedbackPhase, ObservedJsonShape, OutputContractDiagnostic,
    OutputContractDiagnosticCode, candidate_sha256,
};
use crate::item_definition::{ItemDefinitionValidationMode, ItemDefinitionValidator};
use crate::mod_plan::PlanItem;
use crate::prompt::{FeatureRecipe, FeatureRecipeError, FeatureRecipeLoader};
use crate::resource_prepare::{ResourcePrepareFeature, ResourceSpecs, expand_target_template};

const RECIPE_BYTES: &[u8] = include_bytes!("../recipes/mod-generate-single.json");
const RECIPE_SHA256: &str = "f62f6f7f339e3cb30c6e43f321a1fe817a38e47300be3addd6835a87c14e0d3d";
const MAX_EVIDENCE_RECORDS: u16 = 20;

pub struct SingleGenerateFeature;

impl FeatureSpec for SingleGenerateFeature {
    type Request = SingleGenerateRequest;
    type Result = SingleGenerateResult;
    type ArtifactExtension = SingleGenerateArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("mod.generate.single").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema_version("feature.mod-generate-single-request", 3)
    }

    fn result_schema() -> SchemaRef {
        schema_version("feature.mod-generate-single-result", 2)
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema_version("feature.mod-generate-single-artifact-extension", 2)
    }
}

impl SingleGenerateFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: generation_slot(),
            schema: schema_version("pack.mod-generate-single", 5),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SingleGenerateRequest {
    pub artifact_id: String,
    pub mod_id: String,
    pub plan: PlanItem,
    pub definition: StoredItemDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SingleGenerateResult {
    pub publication: SingleGeneratePublication,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_manifest_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<Sha256Digest>,
    pub generated_file_count: u32,
    pub validation_primitive: PrimitiveId,
    pub acceptance_notes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SingleGeneratePublication {
    Published,
    CompositionStaged,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SingleGenerateArtifactExtension {
    pub model_request_sha256: Sha256Digest,
    pub definition_hash: Sha256Digest,
    pub generated_file_count: u32,
    pub validation_primitive: PrimitiveId,
    pub acceptance_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerateContribution {
    validation_primitive: PrimitiveId,
    guidance: Vec<String>,
    item_types: Vec<GenerateItemType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GenerateItemType {
    id: String,
    guidance: Vec<String>,
    generated_files: Vec<GeneratedFileSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedFileSpec {
    role: String,
    target_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    composition_merge: Option<CompositionFileMerge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    composition_merge_key_policy: Option<CompositionMergeKeyPolicy>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CompositionFileMerge {
    JsonObject,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CompositionMergeKeyPolicy {
    UniqueKeys,
    ExclusivePath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratedModBundle {
    files: BTreeMap<String, serde_json::Value>,
    acceptance_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SingleGenerateFailureDetails {
    reason_code: SingleGenerateFailureReason,
    #[serde(skip)]
    output_feedback: Option<SingleOutputFeedbackEvidence>,
}

#[derive(Debug, Clone, Copy, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum SingleGenerateFailureReason {
    OutputTruncated,
    JsonDecode,
    FileCount,
    AcceptanceNotes,
    FileRole,
    FileContent,
    MergeContent,
    MergeKeyConflict,
    GeneratedFileCountOverflow,
    CheckpointProvenance,
    CheckpointResult,
}

impl SingleGenerateFailureDetails {
    fn reason(reason_code: SingleGenerateFailureReason) -> Self {
        Self {
            reason_code,
            output_feedback: None,
        }
    }

    fn with_output_feedback(
        mut self,
        diagnostic: OutputContractDiagnostic,
        candidate_sha256: Sha256Digest,
    ) -> Self {
        self.output_feedback = Some(SingleOutputFeedbackEvidence {
            envelope: GenerationFeedbackEnvelope::output_contract(diagnostic),
            candidate_sha256,
        });
        self
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SingleOutputFeedbackEvidence {
    pub envelope: GenerationFeedbackEnvelope,
    pub candidate_sha256: Sha256Digest,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratePromptContribution<'a> {
    item_type: &'a str,
    common_guidance: &'a [String],
    item_guidance: &'a [String],
    generated_files: Vec<GeneratePromptFileSpec<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeneratePromptFileSpec<'a> {
    role: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    composition_merge: Option<CompositionFileMerge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    composition_merge_key_policy: Option<CompositionMergeKeyPolicy>,
}

struct SingleRequestAssemblyContext<'a> {
    request: &'a SingleGenerateRequest,
    context: &'a SingleGenerateContext<'a>,
    contribution: &'a GenerateContribution,
    item_spec: &'a GenerateItemType,
    evidence: &'a [TruthEvidenceRecord],
    resources: &'a [ModelResourceRef],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactGameContext {
    game_pack_id: ats_kernel::GamePackId,
    game_pack_sha256: Sha256Digest,
    truth_snapshot_id: Sha256Digest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactModelProvenance {
    request_sha256: Sha256Digest,
    model: String,
    usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactResourceProvenance {
    selected_resources: Vec<ModelResourceRef>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactItemDefinitionProvenance {
    definition_hash: Sha256Digest,
}

pub struct SingleGenerateContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
    pub resource_contributions: &'a VerifiedContributionSet,
    pub truth: &'a VerifiedTruthSnapshot,
    pub project_root: &'a Path,
    pub project_context: &'a str,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SingleGenerateExecution {
    pub result: SingleGenerateResult,
    pub request_snapshot: ModelRequestSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SingleGenerationProvenance {
    pub definition_hash: Sha256Digest,
    pub model_request_sha256: Sha256Digest,
    pub model: String,
    pub usage: TokenUsage,
    pub selected_resources: Vec<ModelResourceRef>,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedArtifactFile {
    pub role: String,
    pub relative_path: String,
    pub composition_merge: Option<CompositionFileMerge>,
    pub composition_merge_key_policy: Option<CompositionMergeKeyPolicy>,
}

pub struct SingleGenerateProposal {
    pub result: SingleGenerateResult,
    pub request_snapshot: ModelRequestSnapshot,
    pub writes: Vec<ProjectFileWrite>,
    pub artifact_files: Vec<ProposedArtifactFile>,
    pub provenance: SingleGenerationProvenance,
    generated: ValidatedBundle,
    selected: Vec<LoadedResource>,
    extension: SingleGenerateArtifactExtension,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SingleGenerateProposalCheckpoint {
    pub result: SingleGenerateResult,
    pub provenance: SingleGenerationProvenance,
    pub(crate) files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SingleRevisionRequest {
    GeneratedContent {
        current_files: BTreeMap<String, String>,
        issues: Vec<ValidationIssue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_feedback: Option<GenerationFeedbackEnvelope>,
    },
    OperatorAdjustment {
        current_files: BTreeMap<String, String>,
        instruction: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_feedback: Option<GenerationFeedbackEnvelope>,
    },
}

enum SingleFeedbackContext<'a> {
    Revision(&'a SingleRevisionRequest),
    OutputContract(&'a GenerationFeedbackEnvelope),
}

pub struct SingleGenerateCompositionProposal {
    pub result: SingleGenerateResult,
    pub writes: Vec<ProjectFileWrite>,
    pub artifact_files: Vec<ProposedArtifactFile>,
    pub provenance: SingleGenerationProvenance,
}

impl SingleGenerateProposal {
    #[must_use]
    pub fn checkpoint(&self) -> SingleGenerateProposalCheckpoint {
        SingleGenerateProposalCheckpoint {
            result: self.result.clone(),
            provenance: self.provenance.clone(),
            files: self
                .generated
                .files
                .iter()
                .map(|(role, _, content)| (role.clone(), content.clone()))
                .collect(),
        }
    }

    #[must_use]
    pub fn into_composition(self) -> SingleGenerateCompositionProposal {
        SingleGenerateCompositionProposal {
            result: self.result,
            writes: self.writes,
            artifact_files: self.artifact_files,
            provenance: self.provenance,
        }
    }

    pub(crate) fn composition_proposal(&self) -> SingleGenerateCompositionProposal {
        SingleGenerateCompositionProposal {
            result: self.result.clone(),
            writes: self.writes.clone(),
            artifact_files: self.artifact_files.clone(),
            provenance: self.provenance.clone(),
        }
    }

    pub(crate) fn merge_key_conflict_error(&self, role: String) -> SingleGenerateError {
        merge_key_conflict_error(&self.checkpoint(), role)
    }
}

fn merge_key_conflict_error(
    checkpoint: &SingleGenerateProposalCheckpoint,
    role: String,
) -> SingleGenerateError {
    let candidate = serde_json::to_vec(&checkpoint.files)
        .expect("validated Single checkpoint files are serializable");
    SingleGenerateError::InvalidModelOutput(
        SingleGenerateFailureDetails::reason(SingleGenerateFailureReason::MergeKeyConflict)
            .with_output_feedback(
                OutputContractDiagnostic {
                    code: OutputContractDiagnosticCode::MergeKeyConflict,
                    role_id: Some(role),
                    expected_shape: ExpectedOutputShape::UniqueObjectKeys,
                    observed_shape: ObservedJsonShape::Object,
                },
                candidate_sha256(&candidate),
            ),
    )
}

pub struct SingleProposalDependencies<'a, C, R>
where
    C: ModelClient + ?Sized,
    R: ResourceRepository + ?Sized,
{
    pub model: &'a C,
    pub resources: &'a R,
}

pub struct SingleGenerateDependencies<'a, C, R, W, V, A>
where
    C: ModelClient + ?Sized,
    R: ResourceRepository + ?Sized,
    W: ProjectFileWriter + ?Sized,
    V: ValidationRunner + ?Sized,
    A: ArtifactPublisher + ?Sized,
{
    pub model: &'a C,
    pub resources: &'a R,
    pub writer: &'a W,
    pub validator: &'a V,
    pub artifacts: &'a A,
}

pub struct SingleGenerateService {
    recipe: FeatureRecipe,
}

impl SingleGenerateService {
    pub fn built_in() -> Result<Self, SingleGenerateError> {
        let expected = Sha256Digest::parse(RECIPE_SHA256)
            .map_err(|_| SingleGenerateError::InvalidRecipeContract)?;
        Self::from_recipe(FeatureRecipeLoader::load(RECIPE_BYTES, &expected)?)
    }

    pub fn from_recipe(recipe: FeatureRecipe) -> Result<Self, SingleGenerateError> {
        if recipe.feature_id() != &SingleGenerateFeature::id()
            || recipe.output_contract().schema != bundle_schema()
        {
            return Err(SingleGenerateError::InvalidRecipeContract);
        }
        Ok(Self { recipe })
    }

    pub async fn execute<C, R, W, V, A>(
        &self,
        dependencies: SingleGenerateDependencies<'_, C, R, W, V, A>,
        run: &mut RunRecord,
        request: SingleGenerateRequest,
        context: SingleGenerateContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<SingleGenerateExecution, SingleGenerateError>
    where
        C: ModelClient + ?Sized,
        R: ResourceRepository + ?Sized,
        W: ProjectFileWriter + ?Sized,
        V: ValidationRunner + ?Sized,
        A: ArtifactPublisher + ?Sized,
    {
        let proposal = self
            .propose(
                SingleProposalDependencies {
                    model: dependencies.model,
                    resources: dependencies.resources,
                },
                run,
                &request,
                &context,
                cancellation,
            )
            .await?;

        let pending =
            dependencies
                .writer
                .apply(context.project_root, run.id(), proposal.writes.clone())?;
        if let Err(error) = check_cancelled(cancellation) {
            rollback(pending)?;
            return Err(error);
        }
        let validation = dependencies
            .validator
            .validate(
                ValidationRequest {
                    primitive: proposal.extension.validation_primitive.clone(),
                    project_root: context.project_root.to_path_buf(),
                    run_id: run.id().clone(),
                },
                cancellation,
            )
            .await;
        if let Err(error) = validation {
            rollback(pending)?;
            return Err(error.into());
        }
        if let Err(error) = check_cancelled(cancellation) {
            rollback(pending)?;
            return Err(error);
        }

        let publish_request = artifact_request(
            &request,
            &context,
            run,
            &proposal.request_snapshot,
            &proposal.provenance.model,
            proposal.provenance.usage.clone(),
            &proposal.generated,
            &proposal.selected,
            proposal.extension.clone(),
        )?;
        let published = match dependencies.artifacts.publish(publish_request) {
            Ok(published) => published,
            Err(_) => {
                rollback(pending)?;
                return Err(SingleGenerateError::ArtifactPublication);
            }
        };
        if let Err(error) = check_cancelled(cancellation) {
            let cleanup_result =
                cleanup_published(dependencies.artifacts, &request.artifact_id, run.id());
            let rollback_result = rollback(pending);
            cleanup_result?;
            rollback_result?;
            return Err(error);
        }

        let result = result_from_published(&published, &proposal.extension);
        let result_payload =
            VersionedPayload::from_typed(SingleGenerateFeature::result_schema(), &result)?;
        if run
            .apply_transition(
                RunTransition::Succeed {
                    result: result_payload,
                },
                Utc::now(),
            )
            .is_err()
        {
            let cleanup_result =
                cleanup_published(dependencies.artifacts, &request.artifact_id, run.id());
            let rollback_result = rollback(pending);
            cleanup_result?;
            rollback_result?;
            return Err(SingleGenerateError::RunTransition);
        }
        pending.commit()?;
        Ok(SingleGenerateExecution {
            result,
            request_snapshot: proposal.request_snapshot,
        })
    }

    pub async fn propose<C, R>(
        &self,
        dependencies: SingleProposalDependencies<'_, C, R>,
        run: &RunRecord,
        request: &SingleGenerateRequest,
        context: &SingleGenerateContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<SingleGenerateProposal, SingleGenerateError>
    where
        C: ModelClient + ?Sized,
        R: ResourceRepository + ?Sized,
    {
        validate_run(run, request)?;
        validate_context(context)?;
        validate_request(request)?;
        check_cancelled(cancellation)?;
        let contribution: GenerateContribution = context.contributions.decode(&generation_slot())?;
        contribution.validate(context.pack)?;
        let item_spec = contribution
            .item_types
            .iter()
            .find(|item| item.id == request.plan.item_type)
            .ok_or(SingleGenerateError::UnsupportedItemType)?;
        let item_type = ItemTypeId::parse(request.plan.item_type.as_str())
            .map_err(|_| SingleGenerateError::UnsupportedItemType)?;
        let item_descriptor = context
            .pack
            .item_type(&item_type)
            .ok_or(SingleGenerateError::UnsupportedItemType)?;
        ItemDefinitionValidator::validate(
            context.pack,
            &request.definition.definition,
            ItemDefinitionValidationMode::Ready,
        )
        .map_err(|_| SingleGenerateError::InvalidItemDefinition)?;
        validate_definition_identity(request)?;
        let resource_specs: ResourceSpecs = context
            .resource_contributions
            .decode(&resource_specs_slot())?;
        resource_specs
            .validate()
            .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        validate_plan_roles(&request.plan, item_descriptor)?;
        let evidence = query_evidence(context.truth, item_descriptor)?;
        let selected = load_resources(
            dependencies.resources,
            &request.definition,
            item_descriptor,
            &resource_specs,
        )?;
        let resource_refs = selected
            .iter()
            .map(|item| item.reference.clone())
            .collect::<Vec<_>>();
        let snapshot = self.assemble_request(
            SingleRequestAssemblyContext {
                request,
                context,
                contribution: &contribution,
                item_spec,
                evidence: &evidence,
                resources: &resource_refs,
            },
            None,
        )?;
        self.complete_proposal_from_model(
            dependencies.model,
            request,
            item_spec,
            selected,
            resource_refs,
            &resource_specs,
            contribution.validation_primitive,
            snapshot,
            cancellation,
        )
        .await
    }

    pub async fn revision_propose<C, R>(
        &self,
        dependencies: SingleProposalDependencies<'_, C, R>,
        run: &RunRecord,
        request: &SingleGenerateRequest,
        context: &SingleGenerateContext<'_>,
        revision: &SingleRevisionRequest,
        cancellation: &CancellationToken,
    ) -> Result<SingleGenerateProposal, SingleGenerateError>
    where
        C: ModelClient + ?Sized,
        R: ResourceRepository + ?Sized,
    {
        validate_run(run, request)?;
        validate_context(context)?;
        validate_request(request)?;
        check_cancelled(cancellation)?;
        let contribution: GenerateContribution = context.contributions.decode(&generation_slot())?;
        contribution.validate(context.pack)?;
        let item_spec = contribution
            .item_types
            .iter()
            .find(|item| item.id == request.plan.item_type)
            .ok_or(SingleGenerateError::UnsupportedItemType)?;
        let item_type = ItemTypeId::parse(request.plan.item_type.as_str())
            .map_err(|_| SingleGenerateError::UnsupportedItemType)?;
        let descriptor = context
            .pack
            .item_type(&item_type)
            .ok_or(SingleGenerateError::UnsupportedItemType)?;
        ItemDefinitionValidator::validate(
            context.pack,
            &request.definition.definition,
            ItemDefinitionValidationMode::Ready,
        )
        .map_err(|_| SingleGenerateError::InvalidItemDefinition)?;
        validate_definition_identity(request)?;
        validate_plan_roles(&request.plan, descriptor)?;
        validate_revision_request(item_spec, revision)?;
        let resource_specs: ResourceSpecs = context
            .resource_contributions
            .decode(&resource_specs_slot())?;
        resource_specs
            .validate()
            .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        let selected = load_resources(
            dependencies.resources,
            &request.definition,
            descriptor,
            &resource_specs,
        )?;
        let resource_refs = selected
            .iter()
            .map(|item| item.reference.clone())
            .collect::<Vec<_>>();
        let evidence = query_evidence(context.truth, descriptor)?;
        let snapshot = self.assemble_request(
            SingleRequestAssemblyContext {
                request,
                context,
                contribution: &contribution,
                item_spec,
                evidence: &evidence,
                resources: &resource_refs,
            },
            Some(SingleFeedbackContext::Revision(revision)),
        )?;
        self.complete_proposal_from_model(
            dependencies.model,
            request,
            item_spec,
            selected,
            resource_refs,
            &resource_specs,
            contribution.validation_primitive,
            snapshot,
            cancellation,
        )
        .await
    }

    pub async fn feedback_propose<C, R>(
        &self,
        dependencies: SingleProposalDependencies<'_, C, R>,
        run: &RunRecord,
        request: &SingleGenerateRequest,
        context: &SingleGenerateContext<'_>,
        feedback: &GenerationFeedbackEnvelope,
        cancellation: &CancellationToken,
    ) -> Result<SingleGenerateProposal, SingleGenerateError>
    where
        C: ModelClient + ?Sized,
        R: ResourceRepository + ?Sized,
    {
        if !feedback.is_valid() || feedback.phase != GenerationFeedbackPhase::OutputContract {
            return Err(SingleGenerateError::InvalidInput);
        }
        validate_run(run, request)?;
        validate_context(context)?;
        validate_request(request)?;
        check_cancelled(cancellation)?;
        let contribution: GenerateContribution = context.contributions.decode(&generation_slot())?;
        contribution.validate(context.pack)?;
        let item_spec = contribution
            .item_types
            .iter()
            .find(|item| item.id == request.plan.item_type)
            .ok_or(SingleGenerateError::UnsupportedItemType)?;
        let item_type = ItemTypeId::parse(request.plan.item_type.as_str())
            .map_err(|_| SingleGenerateError::UnsupportedItemType)?;
        let descriptor = context
            .pack
            .item_type(&item_type)
            .ok_or(SingleGenerateError::UnsupportedItemType)?;
        ItemDefinitionValidator::validate(
            context.pack,
            &request.definition.definition,
            ItemDefinitionValidationMode::Ready,
        )
        .map_err(|_| SingleGenerateError::InvalidItemDefinition)?;
        validate_definition_identity(request)?;
        validate_plan_roles(&request.plan, descriptor)?;
        let resource_specs: ResourceSpecs = context
            .resource_contributions
            .decode(&resource_specs_slot())?;
        resource_specs
            .validate()
            .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        let selected = load_resources(
            dependencies.resources,
            &request.definition,
            descriptor,
            &resource_specs,
        )?;
        let resource_refs = selected
            .iter()
            .map(|item| item.reference.clone())
            .collect::<Vec<_>>();
        let evidence = query_evidence(context.truth, descriptor)?;
        let snapshot = self.assemble_request(
            SingleRequestAssemblyContext {
                request,
                context,
                contribution: &contribution,
                item_spec,
                evidence: &evidence,
                resources: &resource_refs,
            },
            Some(SingleFeedbackContext::OutputContract(feedback)),
        )?;
        self.complete_proposal_from_model(
            dependencies.model,
            request,
            item_spec,
            selected,
            resource_refs,
            &resource_specs,
            contribution.validation_primitive,
            snapshot,
            cancellation,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn complete_proposal_from_model<C: ModelClient + ?Sized>(
        &self,
        model: &C,
        request: &SingleGenerateRequest,
        item_spec: &GenerateItemType,
        selected: Vec<LoadedResource>,
        resource_refs: Vec<ModelResourceRef>,
        resource_specs: &ResourceSpecs,
        validation_primitive: PrimitiveId,
        snapshot: ModelRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<SingleGenerateProposal, SingleGenerateError> {
        check_cancelled(cancellation)?;
        let response = model.complete(snapshot.clone(), cancellation).await?;
        check_cancelled(cancellation)?;
        let response_sha256 = candidate_sha256(response.content.as_bytes());
        if response.finish_reason == FinishReason::MaxTokens {
            return Err(SingleGenerateError::TruncatedModelOutput(
                SingleGenerateFailureDetails::reason(SingleGenerateFailureReason::OutputTruncated)
                    .with_output_feedback(
                        output_contract_diagnostic(
                            SingleGenerateFailureReason::OutputTruncated,
                            &response.content,
                            item_spec,
                        ),
                        response_sha256,
                    ),
            ));
        }
        let bundle: GeneratedModBundle = serde_json::from_str(&response.content).map_err(|_| {
            invalid_model_output_with_feedback(
                SingleGenerateFailureReason::JsonDecode,
                &response.content,
                item_spec,
                response_sha256.clone(),
            )
        })?;
        let generated = validate_bundle(request, item_spec, bundle).map_err(|error| {
            attach_output_feedback(error, &response.content, item_spec, response_sha256)
        })?;
        let writes = build_writes(request, &generated, &selected, resource_specs)?;
        let extension = SingleGenerateArtifactExtension {
            model_request_sha256: snapshot.request_sha256().clone(),
            definition_hash: request.definition.definition_hash.clone(),
            generated_file_count: u32::try_from(generated.len()).map_err(|_| {
                invalid_model_output(SingleGenerateFailureReason::GeneratedFileCountOverflow)
            })?,
            validation_primitive,
            acceptance_notes: generated.acceptance_notes.clone(),
        };
        let artifact_files =
            proposed_artifact_files(request, item_spec, &generated, &selected, resource_specs)?;
        let provenance = SingleGenerationProvenance {
            definition_hash: request.definition.definition_hash.clone(),
            model_request_sha256: snapshot.request_sha256().clone(),
            model: response.model,
            usage: response.usage,
            selected_resources: resource_refs,
        };
        Ok(SingleGenerateProposal {
            result: SingleGenerateResult {
                publication: SingleGeneratePublication::CompositionStaged,
                artifact_manifest_ref: None,
                manifest_sha256: None,
                generated_file_count: extension.generated_file_count,
                validation_primitive: extension.validation_primitive.clone(),
                acceptance_notes: extension.acceptance_notes.clone(),
            },
            request_snapshot: snapshot,
            writes,
            artifact_files,
            provenance,
            generated,
            selected,
            extension,
        })
    }

    pub fn restore_composition_proposal<R>(
        &self,
        resources: &R,
        request: &SingleGenerateRequest,
        context: &SingleGenerateContext<'_>,
        checkpoint: &SingleGenerateProposalCheckpoint,
    ) -> Result<SingleGenerateCompositionProposal, SingleGenerateError>
    where
        R: ResourceRepository + ?Sized,
    {
        validate_context(context)?;
        validate_request(request)?;
        let contribution: GenerateContribution = context.contributions.decode(&generation_slot())?;
        contribution.validate(context.pack)?;
        let item_spec = contribution
            .item_types
            .iter()
            .find(|item| item.id == request.plan.item_type)
            .ok_or(SingleGenerateError::UnsupportedItemType)?;
        let item_type = ItemTypeId::parse(request.plan.item_type.as_str())
            .map_err(|_| SingleGenerateError::UnsupportedItemType)?;
        let item_descriptor = context
            .pack
            .item_type(&item_type)
            .ok_or(SingleGenerateError::UnsupportedItemType)?;
        ItemDefinitionValidator::validate(
            context.pack,
            &request.definition.definition,
            ItemDefinitionValidationMode::Ready,
        )
        .map_err(|_| SingleGenerateError::InvalidItemDefinition)?;
        validate_definition_identity(request)?;
        validate_plan_roles(&request.plan, item_descriptor)?;
        let resource_specs: ResourceSpecs = context
            .resource_contributions
            .decode(&resource_specs_slot())?;
        resource_specs
            .validate()
            .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        let selected = load_resources(
            resources,
            &request.definition,
            item_descriptor,
            &resource_specs,
        )?;
        let resource_refs = selected
            .iter()
            .map(|item| item.reference.clone())
            .collect::<Vec<_>>();
        if checkpoint.provenance.definition_hash != request.definition.definition_hash
            || checkpoint.provenance.selected_resources != resource_refs
            || checkpoint.provenance.model.trim().is_empty()
        {
            return Err(invalid_model_output(
                SingleGenerateFailureReason::CheckpointProvenance,
            ));
        }
        let generated = validate_bundle(
            request,
            item_spec,
            GeneratedModBundle {
                files: decode_checkpoint_files(item_spec, &checkpoint.files)?,
                acceptance_notes: checkpoint.result.acceptance_notes.clone(),
            },
        )?;
        let extension = SingleGenerateArtifactExtension {
            model_request_sha256: checkpoint.provenance.model_request_sha256.clone(),
            definition_hash: request.definition.definition_hash.clone(),
            generated_file_count: u32::try_from(generated.len()).map_err(|_| {
                invalid_model_output(SingleGenerateFailureReason::GeneratedFileCountOverflow)
            })?,
            validation_primitive: contribution.validation_primitive,
            acceptance_notes: generated.acceptance_notes.clone(),
        };
        let result = SingleGenerateResult {
            publication: SingleGeneratePublication::CompositionStaged,
            artifact_manifest_ref: None,
            manifest_sha256: None,
            generated_file_count: extension.generated_file_count,
            validation_primitive: extension.validation_primitive.clone(),
            acceptance_notes: extension.acceptance_notes.clone(),
        };
        if checkpoint.result != result {
            return Err(invalid_model_output(
                SingleGenerateFailureReason::CheckpointResult,
            ));
        }
        Ok(SingleGenerateCompositionProposal {
            result,
            writes: build_writes(request, &generated, &selected, &resource_specs)?,
            artifact_files: proposed_artifact_files(
                request,
                item_spec,
                &generated,
                &selected,
                &resource_specs,
            )?,
            provenance: checkpoint.provenance.clone(),
        })
    }

    fn assemble_request(
        &self,
        assembly: SingleRequestAssemblyContext<'_>,
        feedback: Option<SingleFeedbackContext<'_>>,
    ) -> Result<ModelRequestSnapshot, SingleGenerateError> {
        let SingleRequestAssemblyContext {
            request,
            context,
            contribution,
            item_spec,
            evidence,
            resources,
        } = assembly;
        let output_contract = run_scoped_output_contract(item_spec);
        let pack_contribution = GeneratePromptContribution {
            item_type: &item_spec.id,
            common_guidance: &contribution.guidance,
            item_guidance: &item_spec.guidance,
            generated_files: item_spec
                .generated_files
                .iter()
                .map(|file| GeneratePromptFileSpec {
                    role: &file.role,
                    composition_merge: file.composition_merge,
                    composition_merge_key_policy: file.composition_merge_key_policy,
                })
                .collect(),
        };
        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&output_contract.json_schema)?,
            ),
            ("pack.contribution".into(), serialize(&pack_contribution)?),
            ("truth.evidence".into(), serialize(evidence)?),
            ("resources.selected".into(), serialize(resources)?),
            ("item.definition".into(), serialize(&request.definition)?),
            (
                "project.context".into(),
                bounded(context.project_context, 12_000)?.to_owned(),
            ),
            (
                "runtime.custom_instructions".into(),
                bounded(context.custom_instructions.unwrap_or(""), 4_000)?.to_owned(),
            ),
            ("request.plan".into(), serialize(&request.plan)?),
            (
                "repair.context".into(),
                match feedback {
                    None => String::new(),
                    Some(SingleFeedbackContext::Revision(revision)) => serialize(revision)?,
                    Some(SingleFeedbackContext::OutputContract(feedback)) => serialize(feedback)?,
                },
            ),
        ]);
        let model_request = self.recipe.render_with_output_contract(
            &slots,
            context.model.clone(),
            output_contract,
        )?;
        Ok(ModelRequestSnapshot::new(
            SingleGenerateFeature::id(),
            self.recipe.recipe_ref(),
            ModelGamePackRef {
                id: context.pack.id().clone(),
                sha256: context.pack.content_sha256().clone(),
            },
            Some(context.truth.manifest().snapshot_id().clone()),
            resources.to_vec(),
            model_request,
        )?)
    }
}

pub fn validate_definition_resources<R: ResourceRepository + ?Sized>(
    pack: &LoadedGamePack,
    resource_contributions: &VerifiedContributionSet,
    repository: &R,
    definition: &StoredItemDefinition,
) -> Result<(), SingleGenerateError> {
    definition
        .validate()
        .map_err(|_| SingleGenerateError::InvalidItemDefinition)?;
    ItemDefinitionValidator::validate(
        pack,
        &definition.definition,
        ItemDefinitionValidationMode::Ready,
    )
    .map_err(|_| SingleGenerateError::InvalidItemDefinition)?;
    if resource_contributions.feature_id() != &ResourcePrepareFeature::id()
        || resource_contributions.game_pack_id() != pack.id()
        || resource_contributions.game_pack_sha256() != pack.content_sha256()
    {
        return Err(SingleGenerateError::ContextIdentityMismatch);
    }
    let descriptor = pack
        .item_type(&definition.definition.item_type)
        .ok_or(SingleGenerateError::UnsupportedItemType)?;
    let specs: ResourceSpecs = resource_contributions.decode(&resource_specs_slot())?;
    specs
        .validate()
        .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
    load_resources(repository, definition, descriptor, &specs)?;
    Ok(())
}

pub fn validate_single_generation_readiness<R: ResourceRepository + ?Sized>(
    pack: &LoadedGamePack,
    resource_contributions: &VerifiedContributionSet,
    repository: &R,
    request: &SingleGenerateRequest,
) -> Result<(), SingleGenerateError> {
    validate_request(request)?;
    validate_definition_identity(request)?;
    let descriptor = pack
        .item_type(&request.definition.definition.item_type)
        .ok_or(SingleGenerateError::UnsupportedItemType)?;
    validate_plan_roles(&request.plan, descriptor)?;
    validate_definition_resources(
        pack,
        resource_contributions,
        repository,
        &request.definition,
    )
}

#[derive(Debug, Error)]
pub enum SingleGenerateError {
    #[error("single Mod generation input is invalid")]
    InvalidInput,
    #[error("single Mod ItemDefinition is invalid or does not match the Plan")]
    InvalidItemDefinition,
    #[error("single Mod generation Run does not match the typed request")]
    InvalidRun,
    #[error("single Mod generation context identities do not match")]
    ContextIdentityMismatch,
    #[error("single Mod generation contribution is invalid")]
    InvalidPackContribution,
    #[error("single Mod generation resource specification is invalid")]
    InvalidResourceSpecs,
    #[error("single Mod item type is unsupported by the Pack")]
    UnsupportedItemType,
    #[error("single Mod required resource roles do not match the Pack")]
    ResourceRoleMismatch,
    #[error("single Mod selected resource is invalid")]
    InvalidSelectedResource,
    #[error("single Mod generation has no matching Truth Evidence")]
    MissingEvidence,
    #[error("single Mod generation Recipe does not match its typed contract")]
    InvalidRecipeContract,
    #[error("single Mod model output was truncated")]
    TruncatedModelOutput(SingleGenerateFailureDetails),
    #[error("single Mod model output failed typed validation")]
    InvalidModelOutput(SingleGenerateFailureDetails),
    #[error("single Mod Artifact publication failed")]
    ArtifactPublication,
    #[error("single Mod Artifact cleanup failed")]
    ArtifactCleanup,
    #[error("single Mod Run terminal transition failed")]
    RunTransition,
    #[error("single Mod generation was cancelled")]
    Cancelled,
    #[error("single Mod resource repository operation failed")]
    ResourceRepository,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    EvidenceQuery(#[from] EvidenceQueryError),
    #[error(transparent)]
    Recipe(#[from] FeatureRecipeError),
    #[error(transparent)]
    ModelRequest(#[from] ModelRequestError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    ProjectWrite(#[from] ProjectWriteError),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    Lifecycle(#[from] RunLifecycleError),
}

impl SingleGenerateError {
    #[must_use]
    pub fn output_feedback(&self) -> Option<&SingleOutputFeedbackEvidence> {
        match self {
            Self::TruncatedModelOutput(details) | Self::InvalidModelOutput(details) => {
                details.output_feedback.as_ref()
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn run_failure(&self) -> RunFailure {
        let details = match self {
            Self::TruncatedModelOutput(details) => Some(details.clone()),
            Self::InvalidModelOutput(details) => Some(details.clone()),
            _ => None,
        }
        .map(|details| {
            VersionedPayload::from_typed(single_generate_failure_details_schema(), &details)
                .expect("built-in single generation failure details are valid")
        });
        let (code, stage) = match self {
            Self::InvalidInput | Self::InvalidRun => {
                ("run.input_invalid", "mod.generate.single.request")
            }
            Self::InvalidItemDefinition => {
                ("item.definition_invalid", "mod.generate.single.definition")
            }
            Self::ContextIdentityMismatch => {
                ("truth.context_mismatch", "mod.generate.single.context")
            }
            Self::InvalidPackContribution | Self::Contribution(_) => {
                ("pack.contribution_invalid", "mod.generate.single.pack")
            }
            Self::InvalidResourceSpecs => {
                ("resource.spec_invalid", "mod.generate.single.resources")
            }
            Self::UnsupportedItemType => {
                ("feature.item_type_unsupported", "mod.generate.single.plan")
            }
            Self::ResourceRoleMismatch => {
                ("resource.role_mismatch", "mod.generate.single.resources")
            }
            Self::InvalidSelectedResource => (
                "resource.selection_invalid",
                "mod.generate.single.resources",
            ),
            Self::MissingEvidence => ("truth.evidence_missing", "mod.generate.single.truth"),
            Self::InvalidRecipeContract | Self::Recipe(_) => {
                ("feature.recipe_invalid", "mod.generate.single.recipe")
            }
            Self::TruncatedModelOutput(_) => {
                ("model.output_truncated", "mod.generate.single.model")
            }
            Self::InvalidModelOutput(_) => ("model.output_invalid", "mod.generate.single.model"),
            Self::ArtifactPublication => ("artifact.publish_failed", "mod.generate.single.publish"),
            Self::ArtifactCleanup => ("artifact.cleanup_failed", "mod.generate.single.cleanup"),
            Self::RunTransition | Self::Lifecycle(_) => {
                ("run.transition_failed", "mod.generate.single.result")
            }
            Self::Cancelled => ("run.cancelled", "mod.generate.single.execute"),
            Self::ResourceRepository => {
                ("resource.storage_failed", "mod.generate.single.resources")
            }
            Self::EvidenceQuery(_) => ("truth.query_invalid", "mod.generate.single.truth"),
            Self::ModelRequest(_) => ("model.request_invalid", "mod.generate.single.model"),
            Self::Model(ModelError::Authentication) => {
                ("model.authentication", "mod.generate.single.model")
            }
            Self::Model(ModelError::RateLimited { .. }) => {
                ("model.rate_limited", "mod.generate.single.model")
            }
            Self::Model(ModelError::Configuration) => {
                ("model.configuration", "mod.generate.single.model")
            }
            Self::Model(ModelError::Transport) => {
                ("model.transport_failed", "mod.generate.single.model")
            }
            Self::Model(ModelError::Rejected) => {
                ("model.request_rejected", "mod.generate.single.model")
            }
            Self::Model(ModelError::InvalidResponse) => {
                ("model.response_invalid", "mod.generate.single.model")
            }
            Self::Model(ModelError::Cancelled) => ("run.cancelled", "mod.generate.single.model"),
            Self::ProjectWrite(ProjectWriteError::InvalidWrite)
            | Self::ProjectWrite(ProjectWriteError::DuplicatePath) => {
                ("artifact.write_invalid", "mod.generate.single.write")
            }
            Self::ProjectWrite(ProjectWriteError::Io { .. }) => {
                ("artifact.write_failed", "mod.generate.single.write")
            }
            Self::Validation(ValidationError::UnknownPrimitive) => (
                "validation.primitive_unknown",
                "mod.generate.single.validate",
            ),
            Self::Validation(ValidationError::Rejected(_)) => {
                ("validation.rejected", "mod.generate.single.validate")
            }
            Self::Validation(ValidationError::Unavailable { .. }) => {
                ("validation.unavailable", "mod.generate.single.validate")
            }
            Self::Validation(ValidationError::InvalidReport) => {
                ("validation.report_invalid", "mod.generate.single.validate")
            }
            Self::Validation(ValidationError::Cancelled) => {
                ("run.cancelled", "mod.generate.single.validate")
            }
            Self::Payload(_) => ("run.result_invalid", "mod.generate.single.result"),
        };
        RunFailure::new(
            FailureCode::parse(code).expect("built-in failure code is valid"),
            stage,
            details,
        )
        .expect("built-in Run failure is valid")
    }
}

struct LoadedResource {
    reference: ModelResourceRef,
    bytes: Vec<u8>,
}

struct ValidatedBundle {
    files: Vec<(String, String, String)>,
    acceptance_notes: Vec<String>,
}

impl ValidatedBundle {
    fn len(&self) -> usize {
        self.files.len()
    }
}

impl GenerateContribution {
    fn validate(&self, pack: &LoadedGamePack) -> Result<(), SingleGenerateError> {
        if self.validation_primitive.as_str().is_empty()
            || !valid_text_list(&self.guidance, 64, 2_000, false)
            || self.item_types.is_empty()
            || self.item_types.len() > 64
        {
            return Err(SingleGenerateError::InvalidPackContribution);
        }
        let mut item_ids = BTreeSet::new();
        for item in &self.item_types {
            if ItemTypeId::parse(item.id.as_str()).is_err()
                || !item_ids.insert(item.id.as_str())
                || !valid_text_list(&item.guidance, 64, 2_000, false)
                || item.generated_files.is_empty()
                || item.generated_files.len() > 64
            {
                return Err(SingleGenerateError::InvalidPackContribution);
            }
            let mut roles = BTreeSet::new();
            let mut paths = BTreeSet::new();
            for file in &item.generated_files {
                if !valid_role(&file.role)
                    || !roles.insert(file.role.as_str())
                    || !paths.insert(file.target_path.as_str())
                    || (file.composition_merge.is_some()
                        != file.composition_merge_key_policy.is_some())
                    || expand_generated_target_template(&file.target_path, "fixture", "fixture")
                        .is_err()
                {
                    return Err(SingleGenerateError::InvalidPackContribution);
                }
            }
        }
        let catalog_ids = pack
            .item_types()
            .keys()
            .map(|id| id.as_str())
            .collect::<BTreeSet<_>>();
        if item_ids != catalog_ids {
            return Err(SingleGenerateError::InvalidPackContribution);
        }
        Ok(())
    }
}

fn validate_run(
    run: &RunRecord,
    request: &SingleGenerateRequest,
) -> Result<(), SingleGenerateError> {
    if run.feature_id() != &SingleGenerateFeature::id() || run.status() != RunStatus::Running {
        return Err(SingleGenerateError::InvalidRun);
    }
    let persisted = run
        .request()
        .decode::<SingleGenerateRequest>(&SingleGenerateFeature::request_schema())
        .map_err(|_| SingleGenerateError::InvalidRun)?;
    if &persisted != request {
        return Err(SingleGenerateError::InvalidRun);
    }
    Ok(())
}

fn validate_context(context: &SingleGenerateContext<'_>) -> Result<(), SingleGenerateError> {
    let manifest = context.truth.manifest();
    if context.contributions.feature_id() != &SingleGenerateFeature::id()
        || context.resource_contributions.feature_id() != &ResourcePrepareFeature::id()
        || context.contributions.game_pack_id() != context.pack.id()
        || context.resource_contributions.game_pack_id() != context.pack.id()
        || context.contributions.game_pack_sha256() != context.pack.content_sha256()
        || context.resource_contributions.game_pack_sha256() != context.pack.content_sha256()
        || manifest.game_pack_id() != context.pack.id()
        || manifest.game_pack_sha256() != context.pack.content_sha256()
    {
        return Err(SingleGenerateError::ContextIdentityMismatch);
    }
    bounded(context.project_context, 12_000)?;
    bounded(context.custom_instructions.unwrap_or(""), 4_000)?;
    Ok(())
}

fn validate_request(request: &SingleGenerateRequest) -> Result<(), SingleGenerateError> {
    request
        .plan
        .validate()
        .map_err(|_| SingleGenerateError::InvalidInput)?;
    request
        .definition
        .validate()
        .map_err(|_| SingleGenerateError::InvalidItemDefinition)?;
    if !valid_segment(&request.artifact_id) || !valid_segment(&request.mod_id) {
        return Err(SingleGenerateError::InvalidInput);
    }
    Ok(())
}

fn validate_definition_identity(
    request: &SingleGenerateRequest,
) -> Result<(), SingleGenerateError> {
    if request.definition.definition.item_id.as_str() != request.plan.item_id
        || request.definition.definition.item_type.as_str() != request.plan.item_type
    {
        return Err(SingleGenerateError::InvalidItemDefinition);
    }
    Ok(())
}

fn validate_plan_roles(
    plan: &PlanItem,
    item_descriptor: &ItemTypeDescriptor,
) -> Result<(), SingleGenerateError> {
    let planned = plan
        .required_resource_roles
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let catalog = item_descriptor
        .resource_profiles()
        .iter()
        .flat_map(|profile| profile.required_resource_roles())
        .map(ResourceId::as_str)
        .collect::<BTreeSet<_>>();
    if planned != catalog {
        return Err(SingleGenerateError::ResourceRoleMismatch);
    }
    Ok(())
}

fn query_evidence(
    truth: &VerifiedTruthSnapshot,
    item_descriptor: &ItemTypeDescriptor,
) -> Result<Vec<TruthEvidenceRecord>, SingleGenerateError> {
    let mut matched = Vec::with_capacity(item_descriptor.evidence_queries().len());
    for query in item_descriptor.evidence_queries() {
        let records = truth.query(&query.as_query())?;
        if records.is_empty() {
            return Err(SingleGenerateError::MissingEvidence);
        }
        matched.push(records);
    }

    let mut evidence = Vec::new();
    let mut identities = BTreeSet::new();
    for records in &matched {
        push_unique_evidence(&mut evidence, &mut identities, records[0].clone());
    }
    for records in matched {
        for record in records {
            if evidence.len() >= usize::from(MAX_EVIDENCE_RECORDS) {
                return Ok(evidence);
            }
            push_unique_evidence(&mut evidence, &mut identities, record);
        }
    }
    Ok(evidence)
}

fn push_unique_evidence(
    evidence: &mut Vec<TruthEvidenceRecord>,
    identities: &mut BTreeSet<(String, String, String)>,
    record: TruthEvidenceRecord,
) {
    let identity = (
        record.source_id.clone(),
        record.symbol.clone(),
        record.relative_path.clone(),
    );
    if identities.insert(identity) {
        evidence.push(record);
    }
}

fn load_resources<R: ResourceRepository + ?Sized>(
    repository: &R,
    definition: &StoredItemDefinition,
    item_descriptor: &ItemTypeDescriptor,
    specs: &ResourceSpecs,
) -> Result<Vec<LoadedResource>, SingleGenerateError> {
    let bindings = &definition.definition.resource_bindings;
    let required_roles =
        ItemDefinitionValidator::required_resource_roles(item_descriptor, &definition.definition)
            .map_err(|_| SingleGenerateError::InvalidItemDefinition)?;
    if bindings.len() != required_roles.len() {
        return Err(SingleGenerateError::ResourceRoleMismatch);
    }
    let mut loaded = Vec::with_capacity(bindings.len());
    let mut roles = BTreeSet::new();
    for (logical_role, selected) in bindings {
        let asset = repository
            .load(&selected.resource_id)
            .map_err(|_| SingleGenerateError::ResourceRepository)?;
        validate_selected_asset(
            &asset,
            logical_role.as_str(),
            selected,
            &required_roles,
            specs,
        )?;
        if !roles.insert(asset.logical_role().to_owned()) {
            return Err(SingleGenerateError::ResourceRoleMismatch);
        }
        let bytes = repository
            .read_selected_bytes(&selected.resource_id, &selected.selected_version)
            .map_err(|_| SingleGenerateError::ResourceRepository)?;
        loaded.push(LoadedResource {
            reference: ModelResourceRef {
                resource_id: selected.resource_id.clone(),
                logical_role: asset.logical_role().to_owned(),
                selected_version: selected.selected_version.clone(),
                media_type: asset
                    .selected()
                    .ok_or(SingleGenerateError::InvalidSelectedResource)?
                    .blob
                    .media_type
                    .clone(),
            },
            bytes,
        });
    }
    let expected = required_roles
        .iter()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();
    if roles != expected {
        return Err(SingleGenerateError::ResourceRoleMismatch);
    }
    loaded.sort_by(|left, right| {
        left.reference
            .logical_role
            .cmp(&right.reference.logical_role)
    });
    Ok(loaded)
}

fn validate_selected_asset(
    asset: &ResourceAsset,
    logical_role: &str,
    selected: &ItemResourceBinding,
    required_roles: &[ResourceId],
    specs: &ResourceSpecs,
) -> Result<(), SingleGenerateError> {
    if asset.resource_id() != &selected.resource_id
        || asset.selected_version() != Some(&selected.selected_version)
        || asset.logical_role() != logical_role
        || !required_roles
            .iter()
            .any(|role| role.as_str() == asset.logical_role())
        || asset.selected().is_none_or(|version| {
            specs
                .validate_blob(asset.logical_role(), &version.blob)
                .is_err()
        })
    {
        return Err(SingleGenerateError::InvalidSelectedResource);
    }
    Ok(())
}

fn run_scoped_output_contract(item_spec: &GenerateItemType) -> ModelOutputContract {
    let file_properties = item_spec
        .generated_files
        .iter()
        .map(|file| {
            let content_schema = match file.composition_merge {
                Some(CompositionFileMerge::JsonObject) => serde_json::json!({
                    "type": "object",
                    "additionalProperties": {
                        "type": "string",
                        "maxLength": 16 * 1024 * 1024,
                        "pattern": r"^[^\u0000]*$"
                    }
                }),
                None => serde_json::json!({
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 16 * 1024 * 1024,
                    "pattern": r"^[^\u0000]*\S[^\u0000]*$"
                }),
            };
            (file.role.clone(), content_schema)
        })
        .collect::<serde_json::Map<_, _>>();
    let required_roles = item_spec
        .generated_files
        .iter()
        .map(|file| file.role.as_str())
        .collect::<Vec<_>>();
    ModelOutputContract {
        schema: bundle_schema(),
        json_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["files", "acceptanceNotes"],
            "properties": {
                "files": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": required_roles,
                    "properties": file_properties
                },
                "acceptanceNotes": {
                    "type": "array",
                    "maxItems": 64,
                    "items": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 2_000,
                        "pattern": r"^[^\u0000]*\S[^\u0000]*$"
                    }
                }
            }
        }),
    }
}

fn validate_bundle(
    request: &SingleGenerateRequest,
    item_spec: &GenerateItemType,
    bundle: GeneratedModBundle,
) -> Result<ValidatedBundle, SingleGenerateError> {
    if bundle.files.len() != item_spec.generated_files.len() {
        return Err(invalid_model_output(SingleGenerateFailureReason::FileCount));
    }
    if !valid_text_list(&bundle.acceptance_notes, 64, 2_000, true) {
        return Err(invalid_model_output(
            SingleGenerateFailureReason::AcceptanceNotes,
        ));
    }
    for role in bundle.files.keys() {
        if !valid_role(role) {
            return Err(invalid_model_output(SingleGenerateFailureReason::FileRole));
        }
    }
    let mut by_role = bundle.files;
    let mut files = Vec::with_capacity(item_spec.generated_files.len());
    for spec in &item_spec.generated_files {
        let content = by_role
            .remove(&spec.role)
            .ok_or_else(|| invalid_model_output(SingleGenerateFailureReason::FileRole))?;
        let content = normalize_generated_content(spec, content)?;
        let path = expand_generated_target_template(
            &spec.target_path,
            &request.mod_id,
            &request.plan.item_id,
        )?;
        files.push((spec.role.clone(), path, content));
    }
    if !by_role.is_empty() {
        return Err(invalid_model_output(SingleGenerateFailureReason::FileRole));
    }
    Ok(ValidatedBundle {
        files,
        acceptance_notes: bundle.acceptance_notes,
    })
}

fn validate_revision_request(
    item_spec: &GenerateItemType,
    revision: &SingleRevisionRequest,
) -> Result<(), SingleGenerateError> {
    let current_files = match revision {
        SingleRevisionRequest::GeneratedContent {
            current_files,
            issues,
            output_feedback,
        } => {
            if issues.is_empty()
                || issues.len() > 256
                || issues.iter().any(|issue| {
                    issue.repairability
                        != ats_runtime::ValidationIssueRepairability::GeneratedContent
                })
                || output_feedback.as_ref().is_some_and(|feedback| {
                    !feedback.is_valid()
                        || feedback.phase != GenerationFeedbackPhase::OutputContract
                        || feedback.mode != GenerationFeedbackMode::ReplaceCompleteRoles
                        || feedback.validation_issues != *issues
                })
            {
                return Err(SingleGenerateError::InvalidInput);
            }
            current_files
        }
        SingleRevisionRequest::OperatorAdjustment {
            current_files,
            instruction,
            output_feedback,
        } => {
            if instruction.is_empty()
                || instruction.trim() != instruction
                || instruction.chars().count() > 4_000
                || instruction.contains('\0')
                || output_feedback.as_ref().is_some_and(|feedback| {
                    !feedback.is_valid()
                        || feedback.phase != GenerationFeedbackPhase::OutputContract
                        || feedback.mode != GenerationFeedbackMode::RegenerateCompleteBundle
                })
            {
                return Err(SingleGenerateError::InvalidInput);
            }
            current_files
        }
    };
    if current_files.len() != item_spec.generated_files.len()
        || item_spec.generated_files.iter().any(|spec| {
            current_files
                .get(&spec.role)
                .is_none_or(|content| content.trim().is_empty() || content.contains('\0'))
        })
    {
        return Err(SingleGenerateError::InvalidInput);
    }
    Ok(())
}

fn normalize_generated_content(
    spec: &GeneratedFileSpec,
    content: serde_json::Value,
) -> Result<String, SingleGenerateError> {
    match spec.composition_merge {
        None => {
            let content = content
                .as_str()
                .ok_or_else(|| invalid_model_output(SingleGenerateFailureReason::FileContent))?;
            if content.trim().is_empty()
                || content.len() > 16 * 1024 * 1024
                || content.contains('\0')
            {
                return Err(invalid_model_output(
                    SingleGenerateFailureReason::FileContent,
                ));
            }
            Ok(content.to_owned())
        }
        Some(CompositionFileMerge::JsonObject) => {
            let values = content
                .as_object()
                .ok_or_else(|| invalid_model_output(SingleGenerateFailureReason::MergeContent))?;
            let values = values
                .iter()
                .map(|(key, value)| {
                    value
                        .as_str()
                        .filter(|value| !value.contains('\0'))
                        .map(|value| (key.clone(), value.to_owned()))
                        .ok_or_else(|| {
                            invalid_model_output(SingleGenerateFailureReason::MergeContent)
                        })
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            let content = serde_json::to_string(&values)
                .map_err(|_| invalid_model_output(SingleGenerateFailureReason::MergeContent))?;
            if content.len() > 16 * 1024 * 1024 {
                return Err(invalid_model_output(
                    SingleGenerateFailureReason::MergeContent,
                ));
            }
            Ok(content)
        }
    }
}

fn decode_checkpoint_files(
    item_spec: &GenerateItemType,
    files: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, serde_json::Value>, SingleGenerateError> {
    files
        .iter()
        .map(|(role, content)| {
            let spec = item_spec
                .generated_files
                .iter()
                .find(|spec| &spec.role == role);
            let content = match spec.and_then(|spec| spec.composition_merge) {
                Some(CompositionFileMerge::JsonObject) => {
                    let value: serde_json::Value = serde_json::from_str(content).map_err(|_| {
                        invalid_model_output(SingleGenerateFailureReason::MergeContent)
                    })?;
                    normalize_generated_content(
                        spec.expect("a merge policy can only come from the matched file spec"),
                        value.clone(),
                    )?;
                    value
                }
                None => serde_json::Value::String(content.clone()),
            };
            Ok((role.clone(), content))
        })
        .collect()
}

fn invalid_model_output(reason: SingleGenerateFailureReason) -> SingleGenerateError {
    SingleGenerateError::InvalidModelOutput(SingleGenerateFailureDetails::reason(reason))
}

fn invalid_model_output_with_feedback(
    reason: SingleGenerateFailureReason,
    content: &str,
    item_spec: &GenerateItemType,
    candidate_sha256: Sha256Digest,
) -> SingleGenerateError {
    SingleGenerateError::InvalidModelOutput(
        SingleGenerateFailureDetails::reason(reason).with_output_feedback(
            output_contract_diagnostic(reason, content, item_spec),
            candidate_sha256,
        ),
    )
}

fn attach_output_feedback(
    error: SingleGenerateError,
    content: &str,
    item_spec: &GenerateItemType,
    candidate_sha256: Sha256Digest,
) -> SingleGenerateError {
    let SingleGenerateError::InvalidModelOutput(mut details) = error else {
        return error;
    };
    if is_repairable_output_reason(details.reason_code) {
        let reason = details.reason_code;
        details = details.with_output_feedback(
            output_contract_diagnostic(reason, content, item_spec),
            candidate_sha256,
        );
    }
    SingleGenerateError::InvalidModelOutput(details)
}

fn is_repairable_output_reason(reason: SingleGenerateFailureReason) -> bool {
    matches!(
        reason,
        SingleGenerateFailureReason::OutputTruncated
            | SingleGenerateFailureReason::JsonDecode
            | SingleGenerateFailureReason::FileCount
            | SingleGenerateFailureReason::AcceptanceNotes
            | SingleGenerateFailureReason::FileRole
            | SingleGenerateFailureReason::FileContent
            | SingleGenerateFailureReason::MergeContent
            | SingleGenerateFailureReason::MergeKeyConflict
    )
}

fn output_contract_diagnostic(
    reason: SingleGenerateFailureReason,
    content: &str,
    item_spec: &GenerateItemType,
) -> OutputContractDiagnostic {
    let parsed = serde_json::from_str::<serde_json::Value>(content).ok();
    let files = parsed
        .as_ref()
        .and_then(|value| value.get("files"))
        .and_then(serde_json::Value::as_object);
    let role = match reason {
        SingleGenerateFailureReason::FileContent => item_spec.generated_files.iter().find(|spec| {
            spec.composition_merge.is_none()
                && files
                    .and_then(|values| values.get(&spec.role))
                    .is_none_or(|value| {
                        value
                            .as_str()
                            .is_none_or(|text| text.trim().is_empty() || text.contains('\0'))
                    })
        }),
        SingleGenerateFailureReason::MergeContent => {
            item_spec.generated_files.iter().find(|spec| {
                spec.composition_merge == Some(CompositionFileMerge::JsonObject)
                    && files
                        .and_then(|values| values.get(&spec.role))
                        .is_none_or(|value| {
                            value.as_object().is_none_or(|object| {
                                object.values().any(|entry| {
                                    entry.as_str().is_none_or(|text| text.contains('\0'))
                                })
                            })
                        })
            })
        }
        _ => None,
    };
    let observed = role
        .and_then(|spec| files.and_then(|values| values.get(&spec.role)))
        .or(parsed.as_ref());
    OutputContractDiagnostic {
        code: match reason {
            SingleGenerateFailureReason::OutputTruncated => {
                OutputContractDiagnosticCode::OutputTruncated
            }
            SingleGenerateFailureReason::JsonDecode => OutputContractDiagnosticCode::JsonDecode,
            SingleGenerateFailureReason::FileCount => OutputContractDiagnosticCode::FileCount,
            SingleGenerateFailureReason::AcceptanceNotes => {
                OutputContractDiagnosticCode::AcceptanceNotes
            }
            SingleGenerateFailureReason::FileRole => OutputContractDiagnosticCode::FileRole,
            SingleGenerateFailureReason::FileContent => OutputContractDiagnosticCode::FileContent,
            SingleGenerateFailureReason::MergeContent => OutputContractDiagnosticCode::MergeShape,
            SingleGenerateFailureReason::MergeKeyConflict => {
                OutputContractDiagnosticCode::MergeKeyConflict
            }
            SingleGenerateFailureReason::GeneratedFileCountOverflow
            | SingleGenerateFailureReason::CheckpointProvenance
            | SingleGenerateFailureReason::CheckpointResult => {
                unreachable!("system failures do not produce output feedback")
            }
        },
        role_id: role.map(|spec| spec.role.clone()),
        expected_shape: match reason {
            SingleGenerateFailureReason::MergeContent => ExpectedOutputShape::FlatStringObject,
            SingleGenerateFailureReason::MergeKeyConflict => ExpectedOutputShape::UniqueObjectKeys,
            SingleGenerateFailureReason::FileContent => ExpectedOutputShape::NonEmptyString,
            SingleGenerateFailureReason::FileCount | SingleGenerateFailureReason::FileRole => {
                ExpectedOutputShape::ExactDeclaredRoles
            }
            SingleGenerateFailureReason::AcceptanceNotes => ExpectedOutputShape::BoundedStringArray,
            _ => ExpectedOutputShape::CompleteBundle,
        },
        observed_shape: if reason == SingleGenerateFailureReason::OutputTruncated {
            ObservedJsonShape::Truncated
        } else {
            observed_json_shape(observed)
        },
    }
}

fn observed_json_shape(value: Option<&serde_json::Value>) -> ObservedJsonShape {
    match value {
        None => ObservedJsonShape::Unparseable,
        Some(serde_json::Value::Null) => ObservedJsonShape::Null,
        Some(serde_json::Value::Bool(_)) => ObservedJsonShape::Boolean,
        Some(serde_json::Value::Number(_)) => ObservedJsonShape::Number,
        Some(serde_json::Value::String(value)) => {
            if serde_json::from_str::<serde_json::Value>(value).is_ok() {
                ObservedJsonShape::JsonEncodedString
            } else {
                ObservedJsonShape::String
            }
        }
        Some(serde_json::Value::Array(_)) => ObservedJsonShape::Array,
        Some(serde_json::Value::Object(values)) => {
            if values
                .values()
                .any(|value| value.is_array() || value.is_object())
            {
                ObservedJsonShape::ObjectWithNestedValue
            } else if values.values().any(|value| !value.is_string()) {
                ObservedJsonShape::ObjectWithNonStringValue
            } else {
                ObservedJsonShape::Object
            }
        }
    }
}

fn build_writes(
    request: &SingleGenerateRequest,
    generated: &ValidatedBundle,
    resources: &[LoadedResource],
    resource_specs: &ResourceSpecs,
) -> Result<Vec<ProjectFileWrite>, SingleGenerateError> {
    let mut writes = Vec::with_capacity(generated.files.len() + resources.len());
    for (_, path, content) in &generated.files {
        writes.push(ProjectFileWrite::new(
            path.clone(),
            content.as_bytes().to_vec(),
        )?);
    }
    for resource in resources {
        let spec = resource_specs
            .require_role(
                &resource.reference.logical_role,
                &resource.reference.media_type,
            )
            .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        let path = expand_target_template(
            spec.target_path
                .as_deref()
                .ok_or(SingleGenerateError::InvalidResourceSpecs)?,
            &request.mod_id,
            &request.plan.item_id,
        )
        .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        writes.push(ProjectFileWrite::new(path, resource.bytes.clone())?);
    }
    ats_runtime::validate_project_writes(&writes)?;
    Ok(writes)
}

#[allow(clippy::too_many_arguments)]
fn artifact_request(
    request: &SingleGenerateRequest,
    context: &SingleGenerateContext<'_>,
    run: &RunRecord,
    snapshot: &ModelRequestSnapshot,
    response_model: &str,
    usage: TokenUsage,
    generated: &ValidatedBundle,
    resources: &[LoadedResource],
    extension: SingleGenerateArtifactExtension,
) -> Result<ArtifactPublishRequest, SingleGenerateError> {
    let mut files = Vec::with_capacity(generated.files.len() + resources.len());
    for (role, relative_path, _) in &generated.files {
        files.push(ArtifactFileInput {
            role: role.clone(),
            source_path: context.project_root.join(relative_path),
            published_relative_path: Some(relative_path.clone()),
        });
    }
    let resource_specs: ResourceSpecs = context
        .resource_contributions
        .decode(&resource_specs_slot())?;
    for resource in resources {
        let spec = resource_specs
            .require_role(
                &resource.reference.logical_role,
                &resource.reference.media_type,
            )
            .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        let relative_path = expand_target_template(
            spec.target_path
                .as_deref()
                .ok_or(SingleGenerateError::InvalidResourceSpecs)?,
            &request.mod_id,
            &request.plan.item_id,
        )
        .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        files.push(ArtifactFileInput {
            role: resource.reference.logical_role.clone(),
            source_path: context.project_root.join(&relative_path),
            published_relative_path: Some(relative_path),
        });
    }
    Ok(ArtifactPublishRequest {
        artifact_id: request.artifact_id.clone(),
        artifact_kind: "mod".into(),
        feature_id: SingleGenerateFeature::id(),
        producing_run_id: run.id().clone(),
        contexts: vec![VersionedPayload::from_typed(
            schema("artifact.game-context"),
            &ArtifactGameContext {
                game_pack_id: context.pack.id().clone(),
                game_pack_sha256: context.pack.content_sha256().clone(),
                truth_snapshot_id: context.truth.manifest().snapshot_id().clone(),
            },
        )?],
        provenance: vec![
            VersionedPayload::from_typed(
                schema("artifact.item-definition-provenance"),
                &ArtifactItemDefinitionProvenance {
                    definition_hash: request.definition.definition_hash.clone(),
                },
            )?,
            VersionedPayload::from_typed(
                schema("artifact.model-provenance"),
                &ArtifactModelProvenance {
                    request_sha256: snapshot.request_sha256().clone(),
                    model: response_model.to_owned(),
                    usage,
                },
            )?,
            VersionedPayload::from_typed(
                schema("artifact.resource-provenance"),
                &ArtifactResourceProvenance {
                    selected_resources: resources
                        .iter()
                        .map(|item| item.reference.clone())
                        .collect(),
                },
            )?,
        ],
        feature_extension: VersionedPayload::from_typed(
            SingleGenerateFeature::artifact_extension_schema(),
            &extension,
        )?,
        files,
    })
}

fn proposed_artifact_files(
    request: &SingleGenerateRequest,
    item_spec: &GenerateItemType,
    generated: &ValidatedBundle,
    resources: &[LoadedResource],
    resource_specs: &ResourceSpecs,
) -> Result<Vec<ProposedArtifactFile>, SingleGenerateError> {
    let mut files = generated
        .files
        .iter()
        .map(|(role, relative_path, _)| {
            let spec = item_spec
                .generated_files
                .iter()
                .find(|spec| spec.role == *role)
                .ok_or_else(|| invalid_model_output(SingleGenerateFailureReason::FileRole))?;
            Ok(ProposedArtifactFile {
                role: role.clone(),
                relative_path: relative_path.clone(),
                composition_merge: spec.composition_merge,
                composition_merge_key_policy: spec.composition_merge_key_policy,
            })
        })
        .collect::<Result<Vec<_>, SingleGenerateError>>()?;
    for resource in resources {
        let spec = resource_specs
            .require_role(
                &resource.reference.logical_role,
                &resource.reference.media_type,
            )
            .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        let relative_path = expand_target_template(
            spec.target_path
                .as_deref()
                .ok_or(SingleGenerateError::InvalidResourceSpecs)?,
            &request.mod_id,
            &request.plan.item_id,
        )
        .map_err(|_| SingleGenerateError::InvalidResourceSpecs)?;
        files.push(ProposedArtifactFile {
            role: resource.reference.logical_role.clone(),
            relative_path,
            composition_merge: None,
            composition_merge_key_policy: None,
        });
    }
    Ok(files)
}

fn result_from_published(
    published: &PublishedArtifact,
    extension: &SingleGenerateArtifactExtension,
) -> SingleGenerateResult {
    SingleGenerateResult {
        publication: SingleGeneratePublication::Published,
        artifact_manifest_ref: Some(published.artifact_manifest_ref.clone()),
        manifest_sha256: Some(published.manifest_sha256.clone()),
        generated_file_count: extension.generated_file_count,
        validation_primitive: extension.validation_primitive.clone(),
        acceptance_notes: extension.acceptance_notes.clone(),
    }
}

fn rollback(
    pending: Box<dyn ats_runtime::PendingProjectWrites>,
) -> Result<(), SingleGenerateError> {
    pending
        .rollback()
        .map_err(SingleGenerateError::ProjectWrite)
}

fn cleanup_published<A: ArtifactPublisher + ?Sized>(
    artifacts: &A,
    artifact_id: &str,
    run_id: &ats_runtime::RunId,
) -> Result<(), SingleGenerateError> {
    artifacts
        .remove_published_run(artifact_id, run_id)
        .map_err(|_| SingleGenerateError::ArtifactCleanup)
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), SingleGenerateError> {
    if cancellation.is_cancelled() {
        Err(SingleGenerateError::Cancelled)
    } else {
        Ok(())
    }
}

fn bounded(value: &str, max_chars: usize) -> Result<&str, SingleGenerateError> {
    if value.chars().count() > max_chars || value.contains('\0') {
        Err(SingleGenerateError::InvalidInput)
    } else {
        Ok(value)
    }
}

fn serialize<T: Serialize + ?Sized>(value: &T) -> Result<String, SingleGenerateError> {
    serde_json::to_string_pretty(value).map_err(|_| SingleGenerateError::InvalidInput)
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn valid_role(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn valid_text_list(values: &[String], max_items: usize, max_chars: usize, empty_ok: bool) -> bool {
    (empty_ok || !values.is_empty())
        && values.len() <= max_items
        && values.iter().all(|value| {
            !value.trim().is_empty() && value.chars().count() <= max_chars && !value.contains('\0')
        })
}

fn expand_generated_target_template(
    template: &str,
    mod_id: &str,
    item_id: &str,
) -> Result<String, SingleGenerateError> {
    if template.is_empty()
        || template.len() > 512
        || template.starts_with('/')
        || template.contains('\\')
        || template
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || !valid_segment(mod_id)
        || !valid_segment(item_id)
    {
        return Err(SingleGenerateError::InvalidPackContribution);
    }
    let expanded = template
        .replace("{mod_id}", mod_id)
        .replace("{item_id}", item_id);
    if expanded.contains('{') || expanded.contains('}') {
        return Err(SingleGenerateError::InvalidPackContribution);
    }
    ats_runtime::normalize_relative_path(Path::new(&expanded))
        .map_err(|_| SingleGenerateError::InvalidPackContribution)
}

fn generation_slot() -> ContributionId {
    ContributionId::parse("mod.generate.single").expect("built-in contribution ID is valid")
}

fn resource_specs_slot() -> ContributionId {
    ContributionId::parse("resource.prepare.specs").expect("built-in contribution ID is valid")
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

fn bundle_schema() -> SchemaRef {
    schema_version("feature.mod-generate-single-bundle", 2)
}

fn single_generate_failure_details_schema() -> SchemaRef {
    schema_version("feature.mod-generate-single-failure-details", 2)
}

#[cfg(test)]
mod tests {
    use ats_game_context::{ContributionResolver, GamePackLoader};
    use ats_kernel::{ItemFieldId, ItemId};
    use ats_workspace::{ItemDefinition, ItemFieldValue};
    use sha2::{Digest, Sha256};

    use super::*;

    fn stored_definition(item_id: &str, item_type: &str) -> StoredItemDefinition {
        let definition = ItemDefinition::new(
            ItemId::parse(item_id).unwrap(),
            ItemTypeId::parse(item_type).unwrap(),
        );
        StoredItemDefinition {
            definition_hash: definition.definition_hash().unwrap(),
            definition,
        }
    }

    #[test]
    fn synthetic_pack_uses_the_same_generation_and_resource_contracts() {
        let value = serde_json::json!({
            "schemaVersion": 4,
            "id": "fixture-game",
            "displayName": "Fixture Game",
            "itemTypes": [{
                "id": "fixture_item",
                "displayNames": {"eng":"Fixture item"},
                "requiredLocales": [],
                "fields": [],
                "evidenceQueries": [{
                    "symbols": ["Fixture.Symbol"],
                    "terms": []
                }],
                "resourceProfiles": [{
                    "id":"default",
                    "displayNames":{"eng":"Default"},
                    "requiredResourceRoles":["fixture.icon"]
                }]
            }],
            "contributions": [
                {
                    "slotId": "mod.generate.single",
                    "featureId": "mod.generate.single",
                    "schema": {"id":"pack.mod-generate-single", "version":5},
                    "requiredPrimitives": ["code.fixture-validate"],
                    "payload": {
                        "validationPrimitive": "code.fixture-validate",
                        "guidance": ["Use fixture evidence."],
                        "itemTypes": [{
                            "id": "fixture_item",
                            "guidance": ["Use the fixture item contract."],
                            "generatedFiles": [{
                                "role": "metadata",
                                "targetPath": "{mod_id}/metadata.json"
                            }]
                        }]
                    }
                },
                {
                    "slotId": "resource.prepare.specs",
                    "featureId": "resource.prepare",
                    "schema": {"id":"pack.resource-specs", "version":3},
                    "requiredPrimitives": ["image.fixture-transform"],
                    "payload": {"roles": [
                        {
                            "id": "fixture.master",
                            "mediaTypes": ["image/png"],
                            "width": 128,
                            "height": 128,
                            "requireAlpha": true,
                            "source": {"kind":"master"}
                        },
                        {
                            "id": "fixture.icon",
                            "mediaTypes": ["image/png"],
                            "width": 64,
                            "height": 64,
                            "requireAlpha": true,
                            "targetPath": "{mod_id}/images/{item_id}.png",
                            "source": {
                                "kind":"derived",
                                "sourceRole":"fixture.master",
                                "transform": {
                                    "operation":"resize",
                                    "primitive":"image.fixture-transform",
                                    "version":1
                                }
                            }
                        }
                    ]}
                }
            ]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        let digest = Sha256Digest::parse(format!("{:x}", Sha256::digest(&bytes))).unwrap();
        let pack = GamePackLoader::load(&bytes, &digest).unwrap();
        let generate =
            ContributionResolver::new([PrimitiveId::parse("code.fixture-validate").unwrap()])
                .resolve(
                    &pack,
                    &SingleGenerateFeature::id(),
                    &[SingleGenerateFeature::contribution_requirement()],
                )
                .unwrap();
        let resources =
            ContributionResolver::new([PrimitiveId::parse("image.fixture-transform").unwrap()])
                .resolve(
                    &pack,
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )
                .unwrap();

        let contribution: GenerateContribution = generate.decode(&generation_slot()).unwrap();
        contribution.validate(&pack).unwrap();
        let mut incomplete_generation_catalog = contribution.clone();
        incomplete_generation_catalog.item_types.clear();
        assert!(matches!(
            incomplete_generation_catalog.validate(&pack),
            Err(SingleGenerateError::InvalidPackContribution)
        ));
        let mut merge_without_policy = contribution.clone();
        merge_without_policy.item_types[0].generated_files[0].composition_merge =
            Some(CompositionFileMerge::JsonObject);
        assert!(matches!(
            merge_without_policy.validate(&pack),
            Err(SingleGenerateError::InvalidPackContribution)
        ));
        let mut policy_without_merge = contribution.clone();
        policy_without_merge.item_types[0].generated_files[0].composition_merge_key_policy =
            Some(CompositionMergeKeyPolicy::UniqueKeys);
        assert!(matches!(
            policy_without_merge.validate(&pack),
            Err(SingleGenerateError::InvalidPackContribution)
        ));
        let specs: ResourceSpecs = resources.decode(&resource_specs_slot()).unwrap();
        specs.validate().unwrap();
        assert_eq!(
            expand_generated_target_template(
                &contribution.item_types[0].generated_files[0].target_path,
                "fixture_mod",
                "fixture_item"
            )
            .unwrap(),
            "fixture_mod/metadata.json"
        );
    }

    #[test]
    fn run_scoped_bundle_contract_exposes_and_enforces_exact_pack_roles() {
        let item_spec = GenerateItemType {
            id: "fixture_item".into(),
            guidance: vec!["Use the fixture item contract.".into()],
            generated_files: vec![
                GeneratedFileSpec {
                    role: "source".into(),
                    target_path: "Generated/{item_id}.cs".into(),
                    composition_merge: None,
                    composition_merge_key_policy: None,
                },
                GeneratedFileSpec {
                    role: "localization.eng".into(),
                    target_path: "{mod_id}/localization/eng/items.json".into(),
                    composition_merge: Some(CompositionFileMerge::JsonObject),
                    composition_merge_key_policy: Some(CompositionMergeKeyPolicy::UniqueKeys),
                },
            ],
        };
        let contract = run_scoped_output_contract(&item_spec);
        assert_eq!(contract.schema, bundle_schema());
        assert_eq!(
            contract.json_schema["properties"]["files"]["required"],
            serde_json::json!(["source", "localization.eng"])
        );
        assert_eq!(
            contract.json_schema["properties"]["files"]["additionalProperties"],
            false
        );
        assert_eq!(
            contract.json_schema["properties"]["files"]["properties"]
                .as_object()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            contract.json_schema["properties"]["files"]["properties"]["source"]["type"],
            "string"
        );
        assert_eq!(
            contract.json_schema["properties"]["files"]["properties"]["localization.eng"]["type"],
            "object"
        );
        assert_eq!(
            contract.json_schema["properties"]["files"]["properties"]["localization.eng"]["additionalProperties"]
                ["type"],
            "string"
        );

        let generated = ValidatedBundle {
            // Validated bundles are role-sorted, independently of Pack declaration order.
            files: vec![
                (
                    "localization.eng".into(),
                    "FixtureMod/localization/eng/items.json".into(),
                    r#"{"FIXTURE.title":"Fixture"}"#.into(),
                ),
                (
                    "source".into(),
                    "Generated/fixture_item.cs".into(),
                    "public class Fixture {}".into(),
                ),
            ],
            acceptance_notes: Vec::new(),
        };
        let files = proposed_artifact_files(
            &SingleGenerateRequest {
                artifact_id: "fixture-artifact".into(),
                mod_id: "FixtureMod".into(),
                plan: PlanItem {
                    item_id: "fixture_item".into(),
                    item_type: "fixture_item".into(),
                    name: "Fixture Item".into(),
                    summary: "Fixture summary".into(),
                    behavior_intent: vec!["Expose a fixture".into()],
                    implementation_constraints: Vec::new(),
                    evidence_requirements: Vec::new(),
                    required_resource_roles: Vec::new(),
                    acceptance_criteria: vec!["The fixture compiles".into()],
                },
                definition: stored_definition("fixture_item", "fixture_item"),
            },
            &item_spec,
            &generated,
            &[],
            &ResourceSpecs { roles: Vec::new() },
        )
        .unwrap();
        assert_eq!(files[0].role, "localization.eng");
        assert_eq!(
            files[0].composition_merge,
            Some(CompositionFileMerge::JsonObject)
        );
        assert_eq!(
            files[0].composition_merge_key_policy,
            Some(CompositionMergeKeyPolicy::UniqueKeys)
        );
        assert_eq!(files[1].role, "source");
        assert_eq!(files[1].composition_merge, None);
        assert_eq!(files[1].composition_merge_key_policy, None);
        let prompt_contribution = GeneratePromptContribution {
            item_type: &item_spec.id,
            common_guidance: &[],
            item_guidance: &item_spec.guidance,
            generated_files: item_spec
                .generated_files
                .iter()
                .map(|file| GeneratePromptFileSpec {
                    role: &file.role,
                    composition_merge: file.composition_merge,
                    composition_merge_key_policy: file.composition_merge_key_policy,
                })
                .collect(),
        };
        let prompt_value = serde_json::to_value(prompt_contribution).unwrap();
        assert_eq!(
            prompt_value["generatedFiles"][1],
            serde_json::json!({
                "role": "localization.eng",
                "compositionMerge": "json_object",
                "compositionMergeKeyPolicy": "unique_keys"
            })
        );

        let request = SingleGenerateRequest {
            artifact_id: "fixture-artifact".into(),
            mod_id: "FixtureMod".into(),
            plan: PlanItem {
                item_id: "fixture_item".into(),
                item_type: "fixture_item".into(),
                name: "Fixture Item".into(),
                summary: "Fixture summary".into(),
                behavior_intent: vec!["Expose a fixture".into()],
                implementation_constraints: Vec::new(),
                evidence_requirements: Vec::new(),
                required_resource_roles: Vec::new(),
                acceptance_criteria: vec!["The fixture compiles".into()],
            },
            definition: stored_definition("fixture_item", "fixture_item"),
        };
        let wrong = GeneratedModBundle {
            files: BTreeMap::from([
                (
                    "source".into(),
                    serde_json::Value::String("public class Fixture {}".into()),
                ),
                ("localization.zhs".into(), serde_json::json!({})),
            ]),
            acceptance_notes: Vec::new(),
        };
        assert!(matches!(
            validate_bundle(&request, &item_spec, wrong),
            Err(SingleGenerateError::InvalidModelOutput(_))
        ));

        let valid = GeneratedModBundle {
            files: BTreeMap::from([
                (
                    "source".into(),
                    serde_json::Value::String("public class Fixture {}".into()),
                ),
                (
                    "localization.eng".into(),
                    serde_json::json!({"E2EMOD-FIXTURE_ITEM.title": "Fixture"}),
                ),
            ]),
            acceptance_notes: Vec::new(),
        };
        let generated = validate_bundle(&request, &item_spec, valid).unwrap();
        assert_eq!(generated.files[0].0, "source");
        assert_eq!(generated.files[1].0, "localization.eng");
        assert_eq!(
            generated.files[1].2,
            r#"{"E2EMOD-FIXTURE_ITEM.title":"Fixture"}"#
        );

        for invalid_content in [
            serde_json::Value::String(r#"{"FIXTURE.title":"Fixture"}"#.into()),
            serde_json::json!({"FIXTURE.title": {"nested": "Fixture"}}),
            serde_json::json!({"FIXTURE.title": 42}),
            serde_json::json!({"FIXTURE.title": "Fixture\0"}),
        ] {
            let invalid = GeneratedModBundle {
                files: BTreeMap::from([
                    (
                        "source".into(),
                        serde_json::Value::String("public class Fixture {}".into()),
                    ),
                    ("localization.eng".into(), invalid_content),
                ]),
                acceptance_notes: Vec::new(),
            };
            assert!(matches!(
                validate_bundle(&request, &item_spec, invalid),
                Err(SingleGenerateError::InvalidModelOutput(details))
                    if details.reason_code == SingleGenerateFailureReason::MergeContent
            ));
        }

        let oversized = GeneratedModBundle {
            files: BTreeMap::from([
                (
                    "source".into(),
                    serde_json::Value::String("public class Fixture {}".into()),
                ),
                (
                    "localization.eng".into(),
                    serde_json::json!({"FIXTURE.title": "x".repeat(16 * 1024 * 1024)}),
                ),
            ]),
            acceptance_notes: Vec::new(),
        };
        assert!(matches!(
            validate_bundle(&request, &item_spec, oversized),
            Err(SingleGenerateError::InvalidModelOutput(details))
                if details.reason_code == SingleGenerateFailureReason::MergeContent
        ));

        let checkpoint_files = generated
            .files
            .iter()
            .map(|(role, _, content)| (role.clone(), content.clone()))
            .collect::<BTreeMap<_, _>>();
        let restored = GeneratedModBundle {
            files: decode_checkpoint_files(&item_spec, &checkpoint_files).unwrap(),
            acceptance_notes: Vec::new(),
        };
        assert_eq!(
            validate_bundle(&request, &item_spec, restored)
                .unwrap()
                .files,
            generated.files
        );
        let corrupt_checkpoint = BTreeMap::from([
            ("source".into(), "public class Fixture {}".into()),
            ("localization.eng".into(), "not-json".into()),
        ]);
        assert!(matches!(
            decode_checkpoint_files(&item_spec, &corrupt_checkpoint),
            Err(SingleGenerateError::InvalidModelOutput(details))
                if details.reason_code == SingleGenerateFailureReason::MergeContent
        ));
    }

    #[test]
    fn output_feedback_reports_only_trusted_shapes_and_candidate_hashes() {
        let item_spec = GenerateItemType {
            id: "fixture_item".into(),
            guidance: Vec::new(),
            generated_files: vec![GeneratedFileSpec {
                role: "localization.eng".into(),
                target_path: "localization/eng.json".into(),
                composition_merge: Some(CompositionFileMerge::JsonObject),
                composition_merge_key_policy: Some(CompositionMergeKeyPolicy::UniqueKeys),
            }],
        };
        let secret = "SECRET_LOCALIZATION_VALUE";
        let cases = [
            (
                format!(
                    r#"{{"files":{{"localization.eng":"{{\"title\":\"{secret}\"}}"}},"acceptanceNotes":[]}}"#
                ),
                ObservedJsonShape::JsonEncodedString,
            ),
            (
                format!(
                    r#"{{"files":{{"localization.eng":{{"title":{{"nested":"{secret}"}}}}}},"acceptanceNotes":[]}}"#
                ),
                ObservedJsonShape::ObjectWithNestedValue,
            ),
            (
                r#"{"files":{"localization.eng":{"title":42}},"acceptanceNotes":[]}"#.into(),
                ObservedJsonShape::ObjectWithNonStringValue,
            ),
        ];
        let mut fingerprint = None;
        for (content, expected_shape) in cases {
            let diagnostic = output_contract_diagnostic(
                SingleGenerateFailureReason::MergeContent,
                &content,
                &item_spec,
            );
            assert_eq!(diagnostic.code, OutputContractDiagnosticCode::MergeShape);
            assert_eq!(diagnostic.role_id.as_deref(), Some("localization.eng"));
            assert_eq!(diagnostic.observed_shape, expected_shape);
            let envelope = GenerationFeedbackEnvelope::output_contract(diagnostic);
            let encoded = serde_json::to_string(&envelope).unwrap();
            assert!(!encoded.contains(secret));
            let current = crate::generation_feedback::diagnostic_fingerprint(&envelope).unwrap();
            fingerprint.get_or_insert(current);
        }
        assert_ne!(
            candidate_sha256(b"candidate-a"),
            candidate_sha256(b"candidate-b")
        );
    }

    #[test]
    fn merge_key_conflict_feedback_is_typed_and_redacted() {
        let checkpoint = SingleGenerateProposalCheckpoint {
            result: SingleGenerateResult {
                publication: SingleGeneratePublication::CompositionStaged,
                artifact_manifest_ref: None,
                manifest_sha256: None,
                generated_file_count: 1,
                validation_primitive: PrimitiveId::parse("code.fixture-validate").unwrap(),
                acceptance_notes: Vec::new(),
            },
            provenance: SingleGenerationProvenance {
                definition_hash: Sha256Digest::parse("a".repeat(64)).unwrap(),
                model_request_sha256: Sha256Digest::parse("b".repeat(64)).unwrap(),
                model: "fixture-model".into(),
                usage: TokenUsage::default(),
                selected_resources: Vec::new(),
            },
            files: BTreeMap::from([(
                "localization.eng".into(),
                r#"{"E2EMOD-CARD.title":"Fixture"}"#.into(),
            )]),
        };
        let error = merge_key_conflict_error(&checkpoint, "localization.eng".into());
        let evidence = error.output_feedback().unwrap();
        assert_eq!(
            evidence.envelope.output_diagnostics[0].code,
            OutputContractDiagnosticCode::MergeKeyConflict
        );
        assert_eq!(
            evidence.envelope.output_diagnostics[0].expected_shape,
            ExpectedOutputShape::UniqueObjectKeys
        );
        assert_eq!(
            error.run_failure().details.unwrap().payload(),
            &serde_json::json!({"reasonCode": "merge_key_conflict"})
        );
    }

    #[test]
    fn single_accepts_only_explicitly_selected_pack_conformant_resource_versions() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let item_type = ItemTypeId::parse("relic").unwrap();
        let descriptor = pack.item_type(&item_type).unwrap();
        let contributions =
            ContributionResolver::new([PrimitiveId::parse("image.role-transform").unwrap()])
                .resolve(
                    &pack,
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )
                .unwrap();
        let specs: ResourceSpecs = contributions.decode(&resource_specs_slot()).unwrap();
        let required_roles = descriptor.resource_profiles()[0].required_resource_roles();
        let digest = Sha256Digest::parse("a".repeat(64)).unwrap();
        let make_asset = |width| {
            ResourceAsset::new(
                ResourceId::parse(format!("resource.relic-normal-{width}")).unwrap(),
                "relic.normal".into(),
                ats_workspace::ResourceOrigin::UserUpload,
                ats_workspace::ResourceVersion {
                    id: digest.clone(),
                    parent_version: None,
                    blob: ats_workspace::ResourceBlob {
                        relative_path: format!("versions/{digest}/original.png"),
                        media_type: "image/png".into(),
                        byte_length: 4,
                        sha256: digest.clone(),
                        width,
                        height: 128,
                        has_alpha: true,
                    },
                    provenance: ats_workspace::ResourceVersionProvenance::Original,
                },
            )
            .unwrap()
        };

        let mut asset = make_asset(128);
        let selected = ItemResourceBinding {
            resource_id: asset.resource_id().clone(),
            selected_version: digest.clone(),
        };
        assert!(matches!(
            validate_selected_asset(&asset, "relic.normal", &selected, required_roles, &specs),
            Err(SingleGenerateError::InvalidSelectedResource)
        ));
        asset.select(&digest).unwrap();
        validate_selected_asset(&asset, "relic.normal", &selected, required_roles, &specs).unwrap();

        let mut wrong_dimensions = make_asset(127);
        let wrong_selected = ItemResourceBinding {
            resource_id: wrong_dimensions.resource_id().clone(),
            selected_version: digest.clone(),
        };
        wrong_dimensions.select(&digest).unwrap();
        assert!(matches!(
            validate_selected_asset(
                &wrong_dimensions,
                "relic.normal",
                &wrong_selected,
                required_roles,
                &specs
            ),
            Err(SingleGenerateError::InvalidSelectedResource)
        ));
    }

    #[test]
    fn conditional_profile_plan_union_is_distinct_from_selected_definition_roles() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let descriptor = pack
            .item_type(&ItemTypeId::parse("character").unwrap())
            .unwrap();
        let catalog_roles = descriptor
            .resource_profiles()
            .iter()
            .flat_map(|profile| profile.required_resource_roles())
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut plan = PlanItem {
            item_id: "fixture-character".into(),
            item_type: "character".into(),
            name: "Fixture Character".into(),
            summary: "A conditional Resource profile fixture.".into(),
            behavior_intent: vec!["Provide one observable Character behavior.".into()],
            implementation_constraints: Vec::new(),
            evidence_requirements: Vec::new(),
            required_resource_roles: catalog_roles,
            acceptance_criteria: vec!["The fixture compiles.".into()],
        };
        validate_plan_roles(&plan, descriptor).unwrap();

        let mut placeholder = ItemDefinition::new(
            ItemId::parse("fixture-character").unwrap(),
            ItemTypeId::parse("character").unwrap(),
        );
        placeholder.canonical_fields.insert(
            ItemFieldId::parse("visual_profile").unwrap(),
            ItemFieldValue::Choice("placeholder".into()),
        );
        assert!(
            ItemDefinitionValidator::required_resource_roles(descriptor, &placeholder)
                .unwrap()
                .is_empty()
        );

        plan.required_resource_roles.clear();
        assert!(matches!(
            validate_plan_roles(&plan, descriptor),
            Err(SingleGenerateError::ResourceRoleMismatch)
        ));
    }

    #[test]
    fn single_generation_errors_keep_stable_typed_run_failures() {
        let missing = SingleGenerateError::MissingEvidence.run_failure();
        assert_eq!(missing.code.as_str(), "truth.evidence_missing");
        assert_eq!(missing.stage, "mod.generate.single.truth");

        let publish = SingleGenerateError::ArtifactPublication.run_failure();
        assert_eq!(publish.code.as_str(), "artifact.publish_failed");
        assert_eq!(publish.stage, "mod.generate.single.publish");

        let authentication = SingleGenerateError::Model(ModelError::Authentication).run_failure();
        assert_eq!(authentication.code.as_str(), "model.authentication");

        let invalid = invalid_model_output(SingleGenerateFailureReason::JsonDecode).run_failure();
        assert_eq!(invalid.code.as_str(), "model.output_invalid");
        assert_eq!(invalid.stage, "mod.generate.single.model");
        assert_eq!(
            invalid.details.as_ref().map(VersionedPayload::schema),
            Some(&single_generate_failure_details_schema())
        );
        assert_eq!(
            invalid.details.as_ref().map(VersionedPayload::payload),
            Some(&serde_json::json!({"reasonCode": "json_decode"}))
        );

        let truncated = SingleGenerateError::TruncatedModelOutput(
            SingleGenerateFailureDetails::reason(SingleGenerateFailureReason::OutputTruncated),
        )
        .run_failure();
        assert_eq!(truncated.code.as_str(), "model.output_truncated");
        assert_eq!(
            truncated.details.as_ref().map(VersionedPayload::payload),
            Some(&serde_json::json!({"reasonCode": "output_truncated"}))
        );

        let rejected = SingleGenerateError::Validation(ValidationError::Rejected(
            ats_runtime::ValidationReport {
                exit_code: 1,
                stdout_tail: String::new(),
                stderr_tail: "fixture failure".into(),
                issues: Vec::new(),
            },
        ))
        .run_failure();
        assert_eq!(rejected.code.as_str(), "validation.rejected");
    }
}
