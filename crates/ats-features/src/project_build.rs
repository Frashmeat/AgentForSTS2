use std::collections::BTreeSet;
use std::path::Path;

use ats_game_context::{ContributionResolverError, LoadedGamePack, VerifiedContributionSet};
use ats_kernel::{
    ContributionId, FailureCode, FeatureId, PrimitiveId, SchemaId, SchemaRef, SchemaVersion,
};
use ats_runtime::{
    BuildError, BuildRunner, BuildStepReport, BuildStepRequest, CancellationToken, PayloadError,
    RunFailure, RunLifecycleError, RunRecord, RunStatus, RunTransition, VersionedPayload,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;

pub struct ProjectBuildFeature;

impl FeatureSpec for ProjectBuildFeature {
    type Request = ProjectBuildRequest;
    type Result = ProjectBuildResult;
    type ArtifactExtension = ProjectBuildArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("project.build").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema_version("feature.project-build-request", 2)
    }

    fn result_schema() -> SchemaRef {
        schema("feature.project-build-result")
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.project-build-artifact-extension")
    }
}

impl ProjectBuildFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: build_slot(),
            schema: schema("pack.build-recipe"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectBuildRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_relative_root: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectBuildResult {
    pub steps: Vec<ProjectBuildStepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectBuildStepResult {
    pub id: String,
    pub report: BuildStepReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectBuildArtifactExtension {
    pub step_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BuildRecipe {
    steps: Vec<BuildStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BuildStep {
    id: String,
    primitive: PrimitiveId,
    #[serde(default)]
    isolated_output_property: Option<String>,
}

impl BuildRecipe {
    fn validate(&self) -> Result<(), ProjectBuildError> {
        if self.steps.is_empty() || self.steps.len() > 32 {
            return Err(ProjectBuildError::InvalidRecipe);
        }
        let mut ids = BTreeSet::new();
        for step in &self.steps {
            if !valid_id(&step.id)
                || !ids.insert(step.id.as_str())
                || step
                    .isolated_output_property
                    .as_deref()
                    .is_some_and(|property| !valid_property(property))
            {
                return Err(ProjectBuildError::InvalidRecipe);
            }
        }
        Ok(())
    }
}

pub struct ProjectBuildContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
    pub project_root: &'a Path,
}

pub struct ProjectBuildService;

impl ProjectBuildService {
    pub async fn execute<B: BuildRunner + ?Sized>(
        &self,
        runner: &B,
        run: &mut RunRecord,
        request: ProjectBuildRequest,
        context: ProjectBuildContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<ProjectBuildResult, ProjectBuildError> {
        validate_run(run, &request)?;
        validate_context(&context)?;
        validate_request(&request)?;
        let recipe: BuildRecipe = context.contributions.decode(&build_slot())?;
        recipe.validate()?;
        if request.output_relative_root.is_some()
            && recipe
                .steps
                .iter()
                .any(|step| step.isolated_output_property.is_none())
        {
            return Err(ProjectBuildError::InvalidRecipe);
        }
        let mut steps = Vec::with_capacity(recipe.steps.len());
        for step in recipe.steps {
            if cancellation.is_cancelled() {
                return Err(ProjectBuildError::Cancelled);
            }
            let report = runner
                .run_step(
                    BuildStepRequest {
                        primitive: step.primitive,
                        project_root: context.project_root.to_path_buf(),
                        run_id: run.id().clone(),
                        isolated_output_property: request
                            .output_relative_root
                            .as_ref()
                            .and(step.isolated_output_property),
                        output_relative_root: request.output_relative_root.clone(),
                    },
                    cancellation,
                )
                .await?;
            report.validate()?;
            steps.push(ProjectBuildStepResult {
                id: step.id,
                report,
            });
        }
        if cancellation.is_cancelled() {
            return Err(ProjectBuildError::Cancelled);
        }
        let result = ProjectBuildResult { steps };
        let payload = VersionedPayload::from_typed(ProjectBuildFeature::result_schema(), &result)?;
        run.apply_transition(RunTransition::Succeed { result: payload }, Utc::now())?;
        Ok(result)
    }
}

#[derive(Debug, Error)]
pub enum ProjectBuildError {
    #[error("project build Run does not match its typed request")]
    InvalidRun,
    #[error("project build context identities do not match")]
    ContextIdentityMismatch,
    #[error("project build Pack recipe is invalid")]
    InvalidRecipe,
    #[error("project build was cancelled")]
    Cancelled,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Build(#[from] BuildError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    Lifecycle(#[from] RunLifecycleError),
}

impl ProjectBuildError {
    #[must_use]
    pub fn run_failure(&self) -> RunFailure {
        let (code, stage) = match self {
            Self::InvalidRun => ("run.input_invalid", "project.build.request"),
            Self::ContextIdentityMismatch => ("truth.context_mismatch", "project.build.context"),
            Self::InvalidRecipe | Self::Contribution(_) => {
                ("pack.contribution_invalid", "project.build.pack")
            }
            Self::Cancelled | Self::Build(BuildError::Cancelled) => {
                ("run.cancelled", "project.build.execute")
            }
            Self::Build(BuildError::InvalidRequest) => {
                ("validation.input_invalid", "project.build.request")
            }
            Self::Build(BuildError::UnknownPrimitive) => {
                ("validation.primitive_unknown", "project.build.execute")
            }
            Self::Build(BuildError::Rejected(_)) => {
                ("validation.rejected", "project.build.execute")
            }
            Self::Build(BuildError::Unavailable { .. }) => {
                ("validation.unavailable", "project.build.execute")
            }
            Self::Build(BuildError::InvalidReport) => {
                ("validation.report_invalid", "project.build.execute")
            }
            Self::Payload(_) => ("run.result_invalid", "project.build.result"),
            Self::Lifecycle(_) => ("run.transition_failed", "project.build.result"),
        };
        RunFailure::new(
            FailureCode::parse(code).expect("built-in failure code is valid"),
            stage,
            None,
        )
        .expect("built-in Run failure is valid")
    }
}

fn validate_run(run: &RunRecord, request: &ProjectBuildRequest) -> Result<(), ProjectBuildError> {
    if run.feature_id() != &ProjectBuildFeature::id() || run.status() != RunStatus::Running {
        return Err(ProjectBuildError::InvalidRun);
    }
    let persisted = run
        .request()
        .decode::<ProjectBuildRequest>(&ProjectBuildFeature::request_schema())
        .map_err(|_| ProjectBuildError::InvalidRun)?;
    if &persisted != request {
        return Err(ProjectBuildError::InvalidRun);
    }
    Ok(())
}

fn validate_context(context: &ProjectBuildContext<'_>) -> Result<(), ProjectBuildError> {
    if context.contributions.feature_id() != &ProjectBuildFeature::id()
        || context.contributions.game_pack_id() != context.pack.id()
        || context.contributions.game_pack_sha256() != context.pack.content_sha256()
    {
        return Err(ProjectBuildError::ContextIdentityMismatch);
    }
    Ok(())
}

fn validate_request(request: &ProjectBuildRequest) -> Result<(), ProjectBuildError> {
    if request.output_relative_root.as_deref().is_some_and(|path| {
        path.starts_with(".ats/")
            || ats_runtime::normalize_relative_path(Path::new(path)).as_deref() != Ok(path)
    }) {
        Err(ProjectBuildError::InvalidRecipe)
    } else {
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
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

fn valid_property(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn build_slot() -> ContributionId {
    ContributionId::parse("project.build.recipe").expect("built-in contribution ID is valid")
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
