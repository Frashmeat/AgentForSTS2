use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ats_game_context::{
    ContributionResolverError, GamePipelineRegistry, LoadedGamePack, PipelineRegistryError,
    PipelineSelection, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{
    ContributionId, ExecutionGraphId, FailureCode, FeatureId, SchemaId, SchemaRef, SchemaVersion,
    Sha256Digest,
};
use ats_runtime::{
    ArtifactFileInput, ArtifactPublishRequest, ArtifactPublisher, BuildRunner, CancellationToken,
    ModelClient, PackageWriter, PayloadError, ProjectFileWrite, ProjectFileWriter,
    ProjectStageError, ProjectStageRequest, ProjectStager, ProjectWriteError, RunFailure, RunId,
    RunLifecycleError, RunRecord, RunStatus, RunTransition, ValidationError, ValidationRequest,
    ValidationRunner, VersionedPayload,
};
use ats_workspace::{ItemRepository, ResourceRepository, StoredItemDefinition};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::FeatureSpec;
use crate::composition::{CompositionDraftRef, CompositionGraphError, ResolvedItemGraph};
use crate::mod_generate_single::{
    CompositionFileMerge, CompositionMergeKeyPolicy, ProposedArtifactFile,
    SingleGenerateCompositionProposal, SingleGenerateContext, SingleGenerateError,
    SingleGenerateFeature, SingleGenerateRequest, SingleGenerateResult, SingleGenerateService,
    SingleGenerationProvenance, SingleProposalDependencies,
};
use crate::mod_plan::{
    ModPlanContext, ModPlanError, ModPlanFeature, ModPlanRequest, ModPlanService,
};
use crate::project_build::{
    ProjectBuildContext, ProjectBuildError, ProjectBuildFeature, ProjectBuildRequest,
    ProjectBuildResult, ProjectBuildService,
};
use crate::project_package::{
    ProjectPackageContext, ProjectPackageError, ProjectPackageFeature, ProjectPackageRequest,
    ProjectPackageResult, ProjectPackageService,
};

mod staged;
pub use staged::{
    CompositionAdjustmentItem, StagedCompositionGenerateExecution, StagedCompositionGenerateStart,
    composition_adjustment_items,
};

pub struct CompositionGenerateFeature;

impl FeatureSpec for CompositionGenerateFeature {
    type Request = CompositionGenerateRequest;
    type Result = CompositionGenerateResult;
    type ArtifactExtension = CompositionGenerateArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("composition.generate").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema_version("feature.composition-generate-request", 6)
    }

    fn result_schema() -> SchemaRef {
        schema_version("feature.composition-generate-result", 3)
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema_version("feature.composition-generate-artifact-extension", 3)
    }
}

impl CompositionGenerateFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: generation_slot(),
            schema: schema_version("pack.composition-generate", 2),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionGenerateRequest {
    pub artifact_id: String,
    pub mod_id: String,
    pub root: StoredItemDefinition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<CompositionDraftRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<ProjectPackageRequest>,
    pub repair_policy: RepairPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<ItemAdjustment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<CompositionGenerateExecutionRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemAdjustment {
    pub item_id: ats_kernel::ItemId,
    pub expected_definition_hash: Sha256Digest,
    pub instruction: String,
    pub instruction_sha256: Sha256Digest,
    pub created_at: chrono::DateTime<Utc>,
}

impl ItemAdjustment {
    pub fn new(
        item_id: ats_kernel::ItemId,
        expected_definition_hash: Sha256Digest,
        instruction: impl Into<String>,
        created_at: chrono::DateTime<Utc>,
    ) -> Result<Self, CompositionGenerateError> {
        let instruction = instruction.into().trim().to_owned();
        let value = Self {
            instruction_sha256: digest_text(&instruction)?,
            item_id,
            expected_definition_hash,
            instruction,
            created_at,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), CompositionGenerateError> {
        if self.instruction.is_empty()
            || self.instruction.chars().count() > 4_000
            || self.instruction.contains('\0')
            || self.instruction.trim() != self.instruction
            || digest_text(&self.instruction)? != self.instruction_sha256
        {
            Err(CompositionGenerateError::AdjustmentInvalid)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RepairPolicy {
    UntilPassed,
    MaxRounds { max_rounds: u32 },
}

impl RepairPolicy {
    const MAX_ROUNDS: u32 = 20;

    fn validate(&self) -> Result<(), CompositionGenerateError> {
        match self {
            Self::UntilPassed => Ok(()),
            Self::MaxRounds { max_rounds } if (1..=Self::MAX_ROUNDS).contains(max_rounds) => Ok(()),
            Self::MaxRounds { .. } => Err(CompositionGenerateError::InvalidInput),
        }
    }

    fn permits(&self, completed_rounds: u32) -> bool {
        match self {
            Self::UntilPassed => completed_rounds < Self::MAX_ROUNDS,
            Self::MaxRounds { max_rounds } => completed_rounds < *max_rounds,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CompositionGenerateExecutionRequest {
    Start {
        execution_graph_id: ExecutionGraphId,
    },
    Resume {
        execution_graph_id: ExecutionGraphId,
        expected_revision: u64,
        previous_run_id: RunId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionItemRunResult {
    pub item_id: ats_kernel::ItemId,
    pub definition_hash: Sha256Digest,
    pub plan_run_id: RunId,
    pub generation_run_id: RunId,
    pub generation: SingleGenerateResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionGenerateResult {
    pub artifact_manifest_ref: String,
    pub manifest_sha256: Sha256Digest,
    pub graph_digest: Sha256Digest,
    pub node_count: u32,
    pub generated_file_count: u32,
    pub items: Vec<CompositionItemRunResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_run_id: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<ProjectBuildResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_run_id: Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<ProjectPackageResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_graph_id: Option<ExecutionGraphId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionGenerateArtifactExtension {
    pub graph_digest: Sha256Digest,
    pub root_item_id: ats_kernel::ItemId,
    pub root_definition_hash: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<CompositionDraftRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composition_profile: Option<ats_workspace::ItemCompositionProfile>,
    pub node_count: u32,
    pub generated_file_count: u32,
    pub child_run_ids: Vec<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_output_relative_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_report: Option<ats_runtime::PackageReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_graph_id: Option<ExecutionGraphId>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionGenerateContribution {
    pipeline: PipelineSelection,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompositionGenerationProvenance {
    graph_digest: Sha256Digest,
    nodes: Vec<SingleGenerationProvenance>,
}

pub struct CompositionGenerateContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub composition_contributions: &'a VerifiedContributionSet,
    pub plan_contributions: &'a VerifiedContributionSet,
    pub single_contributions: &'a VerifiedContributionSet,
    pub resource_contributions: &'a VerifiedContributionSet,
    pub build_contributions: Option<&'a VerifiedContributionSet>,
    pub package_contributions: Option<&'a VerifiedContributionSet>,
    pub truth: &'a VerifiedTruthSnapshot,
    pub project_root: &'a Path,
    pub project_context: &'a str,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
}

pub struct CompositionGenerateDependencies<'a, C, I, R, W, S, V, A, B, P>
where
    C: ModelClient + ?Sized,
    I: ItemRepository + ?Sized,
    R: ResourceRepository + ?Sized,
    W: ProjectFileWriter + ?Sized,
    S: ProjectStager + ?Sized,
    V: ValidationRunner + ?Sized,
    A: ArtifactPublisher + ?Sized,
    B: BuildRunner + ?Sized,
    P: PackageWriter + ?Sized,
{
    pub model: &'a C,
    pub items: &'a I,
    pub resources: &'a R,
    pub writer: &'a W,
    pub stager: &'a S,
    pub validator: &'a V,
    pub artifacts: &'a A,
    pub build_runner: &'a B,
    pub package_writer: &'a P,
}

pub struct CompositionGenerateService<'a> {
    plan: &'a ModPlanService,
    single: &'a SingleGenerateService,
    build: &'a ProjectBuildService,
    package: &'a ProjectPackageService,
    pipelines: &'a GamePipelineRegistry,
}

impl<'a> CompositionGenerateService<'a> {
    #[must_use]
    pub fn new(
        plan: &'a ModPlanService,
        single: &'a SingleGenerateService,
        build: &'a ProjectBuildService,
        package: &'a ProjectPackageService,
        pipelines: &'a GamePipelineRegistry,
    ) -> Self {
        Self {
            plan,
            single,
            build,
            package,
            pipelines,
        }
    }
}

#[derive(Debug, Error)]
pub enum CompositionGenerateError {
    #[error("composition generation input is invalid")]
    InvalidInput,
    #[error("composition generation Run does not match its typed request")]
    InvalidRun,
    #[error("composition generation context identities do not match")]
    ContextIdentityMismatch,
    #[error("composition generation Pack contribution is invalid")]
    InvalidContribution,
    #[error("composition nodes do not share one validation Primitive")]
    ValidationPrimitiveMismatch,
    #[error("composition generated files cannot be merged safely")]
    GeneratedFileConflict,
    #[error("composition Artifact publication failed")]
    ArtifactPublication,
    #[error("composition Artifact cleanup failed")]
    ArtifactCleanup,
    #[error("composition Run transition failed")]
    RunTransition,
    #[error("composition execution graph revision or claim conflicts")]
    ExecutionGraphConflict,
    #[error("composition execution graph storage failed")]
    ExecutionGraphStorage,
    #[error("composition child Run storage failed")]
    RunStorage,
    #[error("composition generation checkpoint is invalid")]
    InvalidCheckpoint,
    #[error("composition adjustment is invalid")]
    AdjustmentInvalid,
    #[error("composition adjustment targets a stale definition")]
    AdjustmentStale,
    #[error("composition adjustment requires a new composition plan")]
    AdjustmentRequiresReplan,
    #[error("composition generation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Graph(#[from] CompositionGraphError),
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Pipeline(#[from] PipelineRegistryError),
    #[error(transparent)]
    Plan(#[from] ModPlanError),
    #[error(transparent)]
    Single(#[from] SingleGenerateError),
    #[error(transparent)]
    Stage(#[from] ProjectStageError),
    #[error(transparent)]
    ProjectWrite(#[from] ProjectWriteError),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error(transparent)]
    Build(#[from] ProjectBuildError),
    #[error(transparent)]
    Package(#[from] ProjectPackageError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    Lifecycle(#[from] RunLifecycleError),
}

impl CompositionGenerateError {
    #[must_use]
    pub fn run_failure(&self) -> RunFailure {
        let (code, stage) = match self {
            Self::InvalidInput | Self::InvalidRun => {
                ("run.input_invalid", "composition.generate.request")
            }
            Self::ContextIdentityMismatch => (
                "composition.graph.context_mismatch",
                "composition.generate.context",
            ),
            Self::InvalidContribution | Self::Contribution(_) => {
                ("pack.contribution_invalid", "composition.generate.pack")
            }
            Self::Pipeline(_) => ("game.pipeline.invalid", "composition.generate.pipeline"),
            Self::ValidationPrimitiveMismatch => (
                "composition.generate.validation_mismatch",
                "composition.generate.propose",
            ),
            Self::GeneratedFileConflict => ("model.output_invalid", "composition.generate.merge"),
            Self::ArtifactPublication => {
                ("artifact.publish_failed", "composition.generate.publish")
            }
            Self::ArtifactCleanup => ("artifact.cleanup_failed", "composition.generate.cleanup"),
            Self::RunTransition | Self::Lifecycle(_) => {
                ("run.transition_failed", "composition.generate.result")
            }
            Self::ExecutionGraphConflict => (
                "composition.execution.conflict",
                "composition.generate.execution",
            ),
            Self::ExecutionGraphStorage => (
                "composition.execution.storage_failed",
                "composition.generate.execution",
            ),
            Self::RunStorage => ("run.storage_failed", "composition.generate.child_run"),
            Self::InvalidCheckpoint => (
                "composition.execution.invalid",
                "composition.generate.checkpoint",
            ),
            Self::AdjustmentInvalid => (
                "composition.adjustment.invalid",
                "composition.generate.adjustment",
            ),
            Self::AdjustmentStale => (
                "composition.adjustment.stale",
                "composition.generate.adjustment",
            ),
            Self::AdjustmentRequiresReplan => (
                "composition.adjustment.requires_replan",
                "composition.generate.adjustment",
            ),
            Self::Cancelled => ("run.cancelled", "composition.generate.execute"),
            Self::Graph(error) => (error.code(), "composition.generate.graph"),
            Self::Plan(error) => return error.run_failure(),
            Self::Single(error) => return error.run_failure(),
            Self::Stage(ProjectStageError::InvalidSource | ProjectStageError::LimitExceeded) => (
                "composition.staging.invalid",
                "composition.generate.staging",
            ),
            Self::Stage(ProjectStageError::Io { .. }) => {
                ("composition.staging.failed", "composition.generate.staging")
            }
            Self::ProjectWrite(
                ProjectWriteError::InvalidWrite | ProjectWriteError::DuplicatePath,
            ) => (
                "composition.publication.invalid",
                "composition.generate.publication",
            ),
            Self::ProjectWrite(ProjectWriteError::Io { .. }) => (
                "composition.publication.failed",
                "composition.generate.publication",
            ),
            Self::Validation(_) => ("validation.rejected", "composition.generate.validate"),
            Self::Build(error) => return error.run_failure(),
            Self::Package(error) => return error.run_failure(),
            Self::Payload(_) => ("run.result_invalid", "composition.generate.result"),
        };
        failure(code, stage)
    }
}

fn consolidate_proposed_files(
    proposals: &[SingleGenerateCompositionProposal],
) -> Result<(Vec<ProjectFileWrite>, Vec<ProposedArtifactFile>), CompositionGenerateError> {
    struct PendingFile {
        role: String,
        merge: Option<CompositionFileMerge>,
        merge_key_policy: Option<CompositionMergeKeyPolicy>,
        contents: Vec<Vec<u8>>,
    }

    let mut pending = BTreeMap::<String, PendingFile>::new();
    for proposal in proposals {
        if proposal.writes.len() != proposal.artifact_files.len() {
            return Err(CompositionGenerateError::GeneratedFileConflict);
        }
        for (write, file) in proposal.writes.iter().zip(&proposal.artifact_files) {
            if write.relative_path() != file.relative_path || write.source_path().is_some() {
                return Err(CompositionGenerateError::GeneratedFileConflict);
            }
            if file.composition_merge.is_some() != file.composition_merge_key_policy.is_some() {
                return Err(CompositionGenerateError::GeneratedFileConflict);
            }
            let bytes = write
                .bytes()
                .ok_or(CompositionGenerateError::GeneratedFileConflict)?;
            match pending.get_mut(write.relative_path()) {
                Some(existing)
                    if existing.role == file.role
                        && existing.merge == Some(CompositionFileMerge::JsonObject)
                        && existing.merge_key_policy
                            == Some(CompositionMergeKeyPolicy::UniqueKeys)
                        && file.composition_merge == existing.merge
                        && file.composition_merge_key_policy == existing.merge_key_policy =>
                {
                    existing.contents.push(bytes.to_vec());
                }
                Some(_) => return Err(CompositionGenerateError::GeneratedFileConflict),
                None => {
                    pending.insert(
                        write.relative_path().to_owned(),
                        PendingFile {
                            role: file.role.clone(),
                            merge: file.composition_merge,
                            merge_key_policy: file.composition_merge_key_policy,
                            contents: vec![bytes.to_vec()],
                        },
                    );
                }
            }
        }
    }

    let mut writes = Vec::with_capacity(pending.len());
    let mut artifact_files = Vec::with_capacity(pending.len());
    for (path, file) in pending {
        let bytes = if file.merge == Some(CompositionFileMerge::JsonObject) {
            merge_json_objects(&file.contents)?
        } else if file.contents.len() == 1 {
            file.contents.into_iter().next().unwrap_or_default()
        } else {
            return Err(CompositionGenerateError::GeneratedFileConflict);
        };
        writes.push(ProjectFileWrite::new(path.clone(), bytes)?);
        artifact_files.push(ProposedArtifactFile {
            role: file.role,
            relative_path: path,
            composition_merge: file.merge,
            composition_merge_key_policy: file.merge_key_policy,
        });
    }
    Ok((writes, artifact_files))
}

fn merge_json_objects(contents: &[Vec<u8>]) -> Result<Vec<u8>, CompositionGenerateError> {
    let mut merged = BTreeMap::<String, serde_json::Value>::new();
    for bytes in contents {
        let values = serde_json::from_slice::<BTreeMap<String, serde_json::Value>>(bytes)
            .map_err(|_| CompositionGenerateError::GeneratedFileConflict)?;
        if values.values().any(|value| !value.is_string()) {
            return Err(CompositionGenerateError::GeneratedFileConflict);
        }
        for (key, value) in values {
            if merged.insert(key, value).is_some() {
                return Err(CompositionGenerateError::GeneratedFileConflict);
            }
        }
    }
    serde_json::to_vec_pretty(&merged).map_err(|_| CompositionGenerateError::GeneratedFileConflict)
}

enum NextProposalMergeConflict {
    DuplicateKey { role: String },
    Contract,
}

fn validate_next_proposal_merge_claims(
    previous: &[SingleGenerateCompositionProposal],
    current: &SingleGenerateCompositionProposal,
) -> Result<(), NextProposalMergeConflict> {
    consolidate_proposed_files(std::slice::from_ref(current))
        .map_err(|_| NextProposalMergeConflict::Contract)?;
    let (existing_writes, existing_files) =
        consolidate_proposed_files(previous).map_err(|_| NextProposalMergeConflict::Contract)?;
    let existing = existing_writes
        .iter()
        .zip(&existing_files)
        .map(|(write, file)| (write.relative_path(), (write, file)))
        .collect::<BTreeMap<_, _>>();

    if current.writes.len() != current.artifact_files.len() {
        return Err(NextProposalMergeConflict::Contract);
    }
    for (write, file) in current.writes.iter().zip(&current.artifact_files) {
        if write.relative_path() != file.relative_path
            || write.source_path().is_some()
            || file.composition_merge.is_some() != file.composition_merge_key_policy.is_some()
        {
            return Err(NextProposalMergeConflict::Contract);
        }
        let Some((existing_write, existing_file)) = existing.get(write.relative_path()) else {
            continue;
        };
        if existing_file.role != file.role
            || existing_file.composition_merge != Some(CompositionFileMerge::JsonObject)
            || existing_file.composition_merge_key_policy
                != Some(CompositionMergeKeyPolicy::UniqueKeys)
            || file.composition_merge != existing_file.composition_merge
            || file.composition_merge_key_policy != existing_file.composition_merge_key_policy
        {
            return Err(NextProposalMergeConflict::Contract);
        }
        let existing_keys =
            merge_object_keys(existing_write).map_err(|_| NextProposalMergeConflict::Contract)?;
        let current_keys =
            merge_object_keys(write).map_err(|_| NextProposalMergeConflict::Contract)?;
        if existing_keys.iter().any(|key| current_keys.contains(key)) {
            return Err(NextProposalMergeConflict::DuplicateKey {
                role: file.role.clone(),
            });
        }
    }
    Ok(())
}

fn merge_object_keys(write: &ProjectFileWrite) -> Result<BTreeSet<String>, ()> {
    let bytes = write.bytes().ok_or(())?;
    let values =
        serde_json::from_slice::<BTreeMap<String, serde_json::Value>>(bytes).map_err(|_| ())?;
    if values.values().any(|value| !value.is_string()) {
        return Err(());
    }
    Ok(values.into_keys().collect())
}

fn composition_artifact_request(
    request: &CompositionGenerateRequest,
    context: &CompositionGenerateContext<'_>,
    run: &RunRecord,
    graph: &ResolvedItemGraph,
    proposals: &[SingleGenerateCompositionProposal],
    artifact_files: &[ProposedArtifactFile],
    extension: &CompositionGenerateArtifactExtension,
) -> Result<ArtifactPublishRequest, CompositionGenerateError> {
    let mut paths = BTreeSet::new();
    let mut files = Vec::new();
    for file in artifact_files {
        if !paths.insert(file.relative_path.clone()) {
            return Err(CompositionGenerateError::ProjectWrite(
                ProjectWriteError::DuplicatePath,
            ));
        }
        files.push(ArtifactFileInput {
            role: file.role.clone(),
            source_path: context.project_root.join(&file.relative_path),
            published_relative_path: Some(file.relative_path.clone()),
        });
    }
    if let Some(package) = &request.package {
        if !paths.insert(package.output_relative_path.clone()) {
            return Err(CompositionGenerateError::ProjectWrite(
                ProjectWriteError::DuplicatePath,
            ));
        }
        files.push(ArtifactFileInput {
            role: "package.zip".into(),
            source_path: context.project_root.join(&package.output_relative_path),
            published_relative_path: Some(package.output_relative_path.clone()),
        });
    }
    Ok(ArtifactPublishRequest {
        artifact_id: request.artifact_id.clone(),
        artifact_kind: "composition".into(),
        feature_id: CompositionGenerateFeature::id(),
        producing_run_id: run.id().clone(),
        contexts: vec![VersionedPayload::from_typed(
            schema("artifact.composition-graph"),
            graph,
        )?],
        provenance: vec![VersionedPayload::from_typed(
            schema("artifact.composition-generation-provenance"),
            &CompositionGenerationProvenance {
                graph_digest: graph.graph_digest.clone(),
                nodes: proposals
                    .iter()
                    .map(|proposal| proposal.provenance.clone())
                    .collect(),
            },
        )?],
        feature_extension: VersionedPayload::from_typed(
            CompositionGenerateFeature::artifact_extension_schema(),
            extension,
        )?,
        files,
    })
}

fn validate_run(
    run: &RunRecord,
    request: &CompositionGenerateRequest,
) -> Result<(), CompositionGenerateError> {
    if run.feature_id() != &CompositionGenerateFeature::id() || run.status() != RunStatus::Running {
        return Err(CompositionGenerateError::InvalidRun);
    }
    let persisted = run
        .request()
        .decode::<CompositionGenerateRequest>(&CompositionGenerateFeature::request_schema())
        .map_err(|_| CompositionGenerateError::InvalidRun)?;
    if &persisted != request {
        return Err(CompositionGenerateError::InvalidRun);
    }
    Ok(())
}

fn validate_context(
    context: &CompositionGenerateContext<'_>,
) -> Result<(), CompositionGenerateError> {
    let expected = [
        (
            context.composition_contributions,
            CompositionGenerateFeature::id(),
        ),
        (context.plan_contributions, ModPlanFeature::id()),
        (context.single_contributions, SingleGenerateFeature::id()),
        (
            context.resource_contributions,
            crate::resource_prepare::ResourcePrepareFeature::id(),
        ),
    ];
    if expected.iter().any(|(contributions, feature_id)| {
        contributions.feature_id() != feature_id
            || contributions.game_pack_id() != context.pack.id()
            || contributions.game_pack_sha256() != context.pack.content_sha256()
    }) || [
        (context.build_contributions, ProjectBuildFeature::id()),
        (context.package_contributions, ProjectPackageFeature::id()),
    ]
    .into_iter()
    .any(|(contributions, feature_id)| {
        contributions.is_some_and(|contributions| {
            contributions.feature_id() != &feature_id
                || contributions.game_pack_id() != context.pack.id()
                || contributions.game_pack_sha256() != context.pack.content_sha256()
        })
    }) || context.truth.manifest().game_pack_id() != context.pack.id()
        || context.truth.manifest().game_pack_sha256() != context.pack.content_sha256()
    {
        Err(CompositionGenerateError::ContextIdentityMismatch)
    } else {
        Ok(())
    }
}

fn validate_request(request: &CompositionGenerateRequest) -> Result<(), CompositionGenerateError> {
    if !valid_segment(&request.artifact_id)
        || !valid_segment(&request.mod_id)
        || request.package.as_ref().is_some_and(|package| {
            package.artifact_id != request.artifact_id || package.mod_id != request.mod_id
        })
        || request.root.validate().is_err()
        || request.root.definition.composition_profile.is_none()
        || request
            .draft
            .as_ref()
            .is_some_and(|draft| draft.revision == 0)
    {
        Err(CompositionGenerateError::InvalidInput)
    } else {
        request.repair_policy.validate()?;
        request
            .adjustment
            .as_ref()
            .map_or(Ok(()), ItemAdjustment::validate)
    }
}

fn digest_text(value: &str) -> Result<Sha256Digest, CompositionGenerateError> {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(value.as_bytes())))
        .map_err(|_| CompositionGenerateError::AdjustmentInvalid)
}

fn running_run<F, T>(request: &T) -> Result<RunRecord, CompositionGenerateError>
where
    F: FeatureSpec,
    T: Serialize,
{
    let payload = VersionedPayload::from_typed(F::request_schema(), request)?;
    let mut run = RunRecord::new(F::id(), payload);
    run.apply_transition(RunTransition::Start, Utc::now())?;
    Ok(run)
}

fn succeed_child<F, T>(run: &mut RunRecord, result: &T) -> Result<(), CompositionGenerateError>
where
    F: FeatureSpec,
    T: Serialize,
{
    let payload = VersionedPayload::from_typed(F::result_schema(), result)?;
    run.apply_transition(RunTransition::Succeed { result: payload }, Utc::now())?;
    Ok(())
}

fn finish_failed_child(
    run: &mut RunRecord,
    failure: RunFailure,
    cancellation: &CancellationToken,
) -> Result<(), CompositionGenerateError> {
    let transition = cancellation.reason().map_or_else(
        || RunTransition::Fail { failure },
        |reason| RunTransition::Cancel { reason },
    );
    run.apply_transition(transition, Utc::now())?;
    Ok(())
}

fn cleanup_artifact<A: ArtifactPublisher + ?Sized>(
    artifacts: &A,
    artifact_id: &str,
    run_id: &RunId,
) -> Result<(), CompositionGenerateError> {
    artifacts
        .remove_published_run(artifact_id, run_id)
        .map_err(|_| CompositionGenerateError::ArtifactCleanup)
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), CompositionGenerateError> {
    if cancellation.is_cancelled() {
        Err(CompositionGenerateError::Cancelled)
    } else {
        Ok(())
    }
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn failure(code: &str, stage: &str) -> RunFailure {
    RunFailure::new(
        FailureCode::parse(code).expect("built-in failure code is valid"),
        stage,
        None,
    )
    .expect("built-in Run failure is valid")
}

fn generation_slot() -> ContributionId {
    ContributionId::parse("composition.generate").expect("built-in contribution ID is valid")
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

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal(
        relative_path: &str,
        content: &[u8],
        merge_key_policy: CompositionMergeKeyPolicy,
    ) -> SingleGenerateCompositionProposal {
        SingleGenerateCompositionProposal {
            result: SingleGenerateResult {
                publication:
                    crate::mod_generate_single::SingleGeneratePublication::CompositionStaged,
                artifact_manifest_ref: None,
                manifest_sha256: None,
                generated_file_count: 1,
                validation_primitive: ats_kernel::PrimitiveId::parse("code.fixture-validate")
                    .unwrap(),
                acceptance_notes: Vec::new(),
            },
            writes: vec![ProjectFileWrite::new(relative_path, content.to_vec()).unwrap()],
            artifact_files: vec![ProposedArtifactFile {
                role: "localization.eng".into(),
                relative_path: relative_path.into(),
                composition_merge: Some(CompositionFileMerge::JsonObject),
                composition_merge_key_policy: Some(merge_key_policy),
            }],
            provenance: SingleGenerationProvenance {
                definition_hash: Sha256Digest::parse("a".repeat(64)).unwrap(),
                model_request_sha256: Sha256Digest::parse("b".repeat(64)).unwrap(),
                model: "fixture-model".into(),
                usage: ats_runtime::TokenUsage::default(),
                selected_resources: Vec::new(),
            },
        }
    }

    #[test]
    fn json_object_merge_is_sorted_and_rejects_invalid_or_duplicate_entries() {
        let merged =
            merge_json_objects(&[br#"{"b":"two"}"#.to_vec(), br#"{"a":"one"}"#.to_vec()]).unwrap();
        assert_eq!(
            String::from_utf8(merged).unwrap(),
            "{\n  \"a\": \"one\",\n  \"b\": \"two\"\n}"
        );
        assert!(matches!(
            merge_json_objects(&[br#"{"a":"one"}"#.to_vec(), br#"{"a":"two"}"#.to_vec()]),
            Err(CompositionGenerateError::GeneratedFileConflict)
        ));
        assert!(matches!(
            merge_json_objects(&[br#"{"a":{"nested":true}}"#.to_vec()]),
            Err(CompositionGenerateError::GeneratedFileConflict)
        ));
        assert!(matches!(
            merge_json_objects(&[b"not-json".to_vec()]),
            Err(CompositionGenerateError::GeneratedFileConflict)
        ));
        assert_eq!(
            CompositionGenerateError::GeneratedFileConflict
                .run_failure()
                .code
                .as_str(),
            "model.output_invalid"
        );
    }

    #[test]
    fn composition_merge_enforces_shared_and_exclusive_path_policies() {
        let shared = vec![
            proposal(
                "localization/eng/cards.json",
                br#"{"card_a.title":"A"}"#,
                CompositionMergeKeyPolicy::UniqueKeys,
            ),
            proposal(
                "localization/eng/cards.json",
                br#"{"card_b.title":"B"}"#,
                CompositionMergeKeyPolicy::UniqueKeys,
            ),
        ];
        let (writes, files) = consolidate_proposed_files(&shared).unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].composition_merge_key_policy,
            Some(CompositionMergeKeyPolicy::UniqueKeys)
        );
        assert_eq!(
            String::from_utf8(writes[0].bytes().unwrap().to_vec()).unwrap(),
            "{\n  \"card_a.title\": \"A\",\n  \"card_b.title\": \"B\"\n}"
        );

        let exclusive = vec![proposal(
            "localization/eng/ancients.json",
            br#"{"THE_ARCHITECT.TALK":"Hello"}"#,
            CompositionMergeKeyPolicy::ExclusivePath,
        )];
        assert!(consolidate_proposed_files(&exclusive).is_ok());

        let duplicate_exclusive = vec![
            proposal(
                "localization/eng/ancients.json",
                br#"{"THE_ARCHITECT.TALK":"Hello"}"#,
                CompositionMergeKeyPolicy::ExclusivePath,
            ),
            proposal(
                "localization/eng/ancients.json",
                br#"{"THE_ARCHITECT.TALK_AGAIN":"Hello again"}"#,
                CompositionMergeKeyPolicy::ExclusivePath,
            ),
        ];
        assert!(matches!(
            consolidate_proposed_files(&duplicate_exclusive),
            Err(CompositionGenerateError::GeneratedFileConflict)
        ));

        let inconsistent = vec![
            proposal(
                "localization/eng/items.json",
                br#"{"item_a.title":"A"}"#,
                CompositionMergeKeyPolicy::UniqueKeys,
            ),
            proposal(
                "localization/eng/items.json",
                br#"{"FIXED.TITLE":"B"}"#,
                CompositionMergeKeyPolicy::ExclusivePath,
            ),
        ];
        assert!(matches!(
            consolidate_proposed_files(&inconsistent),
            Err(CompositionGenerateError::GeneratedFileConflict)
        ));
    }

    #[test]
    fn single_boundary_assigns_duplicate_keys_to_the_current_proposal() {
        let previous = vec![proposal(
            "localization/eng/cards.json",
            br#"{"E2EMOD-FIRST_CARD.title":"First"}"#,
            CompositionMergeKeyPolicy::UniqueKeys,
        )];
        let distinct = proposal(
            "localization/eng/cards.json",
            br#"{"E2EMOD-SECOND_CARD.title":"Second"}"#,
            CompositionMergeKeyPolicy::UniqueKeys,
        );
        assert!(validate_next_proposal_merge_claims(&previous, &distinct).is_ok());

        let duplicate = proposal(
            "localization/eng/cards.json",
            br#"{"E2EMOD-FIRST_CARD.title":"Duplicate"}"#,
            CompositionMergeKeyPolicy::UniqueKeys,
        );
        assert!(matches!(
            validate_next_proposal_merge_claims(&previous, &duplicate),
            Err(NextProposalMergeConflict::DuplicateKey { role })
                if role == "localization.eng"
        ));

        let exclusive = proposal(
            "localization/eng/cards.json",
            br#"{"E2EMOD-SECOND_CARD.title":"Second"}"#,
            CompositionMergeKeyPolicy::ExclusivePath,
        );
        assert!(matches!(
            validate_next_proposal_merge_claims(&previous, &exclusive),
            Err(NextProposalMergeConflict::Contract)
        ));
    }

    #[test]
    fn repair_policy_validates_bounds_and_counts_completed_rounds() {
        assert!(RepairPolicy::UntilPassed.validate().is_ok());
        assert!(RepairPolicy::UntilPassed.permits(19));
        assert!(!RepairPolicy::UntilPassed.permits(20));

        let one = RepairPolicy::MaxRounds { max_rounds: 1 };
        assert!(one.validate().is_ok());
        assert!(one.permits(0));
        assert!(!one.permits(1));

        assert!(
            RepairPolicy::MaxRounds { max_rounds: 0 }
                .validate()
                .is_err()
        );
        assert!(
            RepairPolicy::MaxRounds { max_rounds: 20 }
                .validate()
                .is_ok()
        );
        assert!(
            RepairPolicy::MaxRounds { max_rounds: 21 }
                .validate()
                .is_err()
        );
    }
}
