use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ats_game_context::{
    ContributionResolverError, LoadedGamePack, VerifiedContributionSet, VerifiedTruthSnapshot,
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
use thiserror::Error;

use crate::FeatureSpec;
use crate::composition::{CompositionDraftRef, CompositionGraphError, ResolvedItemGraph};
use crate::mod_generate_single::{
    CompositionFileMerge, ProposedArtifactFile, SingleGenerateCompositionProposal,
    SingleGenerateContext, SingleGenerateError, SingleGenerateFeature, SingleGenerateRequest,
    SingleGenerateResult, SingleGenerateService, SingleGenerationProvenance,
    SingleProposalDependencies,
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
pub use staged::{StagedCompositionGenerateExecution, StagedCompositionGenerateStart};

pub struct CompositionGenerateFeature;

impl FeatureSpec for CompositionGenerateFeature {
    type Request = CompositionGenerateRequest;
    type Result = CompositionGenerateResult;
    type ArtifactExtension = CompositionGenerateArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("composition.generate").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema_version("feature.composition-generate-request", 4)
    }

    fn result_schema() -> SchemaRef {
        schema_version("feature.composition-generate-result", 2)
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema_version("feature.composition-generate-artifact-extension", 2)
    }
}

impl CompositionGenerateFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: generation_slot(),
            schema: schema("pack.composition-generate"),
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
    pub package: ProjectPackageRequest,
    pub repair_policy: RepairPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<CompositionGenerateExecutionRequest>,
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
    pub build_run_id: RunId,
    pub build: ProjectBuildResult,
    pub package_run_id: RunId,
    pub package: ProjectPackageResult,
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
    pub package_output_relative_path: String,
    pub package_report: ats_runtime::PackageReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_graph_id: Option<ExecutionGraphId>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompositionGenerateContribution {
    compose: Vec<FeatureId>,
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
    pub build_contributions: &'a VerifiedContributionSet,
    pub package_contributions: &'a VerifiedContributionSet,
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
}

impl<'a> CompositionGenerateService<'a> {
    #[must_use]
    pub fn new(
        plan: &'a ModPlanService,
        single: &'a SingleGenerateService,
        build: &'a ProjectBuildService,
        package: &'a ProjectPackageService,
    ) -> Self {
        Self {
            plan,
            single,
            build,
            package,
        }
    }
}

impl CompositionGenerateContribution {
    fn validate(&self) -> Result<(), CompositionGenerateError> {
        let actual = self.compose.iter().cloned().collect::<BTreeSet<_>>();
        let expected = [
            ModPlanFeature::id(),
            SingleGenerateFeature::id(),
            ProjectBuildFeature::id(),
            ProjectPackageFeature::id(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        if self.compose.len() != expected.len() || actual != expected {
            Err(CompositionGenerateError::InvalidContribution)
        } else {
            Ok(())
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
    #[error("composition generation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Graph(#[from] CompositionGraphError),
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
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
            let bytes = write
                .bytes()
                .ok_or(CompositionGenerateError::GeneratedFileConflict)?;
            match pending.get_mut(write.relative_path()) {
                Some(existing)
                    if existing.role == file.role
                        && existing.merge == Some(CompositionFileMerge::JsonObject)
                        && file.composition_merge == existing.merge =>
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
    if !paths.insert(request.package.output_relative_path.clone()) {
        return Err(CompositionGenerateError::ProjectWrite(
            ProjectWriteError::DuplicatePath,
        ));
    }
    files.push(ArtifactFileInput {
        role: "package.zip".into(),
        source_path: context
            .project_root
            .join(&request.package.output_relative_path),
        published_relative_path: Some(request.package.output_relative_path.clone()),
    });
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
        (context.build_contributions, ProjectBuildFeature::id()),
        (context.package_contributions, ProjectPackageFeature::id()),
    ];
    if expected.iter().any(|(contributions, feature_id)| {
        contributions.feature_id() != feature_id
            || contributions.game_pack_id() != context.pack.id()
            || contributions.game_pack_sha256() != context.pack.content_sha256()
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
        || request.package.artifact_id != request.artifact_id
        || request.package.mod_id != request.mod_id
        || request.root.validate().is_err()
        || request.root.definition.composition_profile.is_none()
        || request
            .draft
            .as_ref()
            .is_some_and(|draft| draft.revision == 0)
    {
        Err(CompositionGenerateError::InvalidInput)
    } else {
        request.repair_policy.validate()
    }
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
