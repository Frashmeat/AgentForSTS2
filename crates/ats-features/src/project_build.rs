use std::collections::BTreeSet;
use std::path::Path;

use ats_game_context::{ContributionResolverError, LoadedGamePack, VerifiedContributionSet};
use ats_kernel::{ContributionId, FeatureId, PrimitiveId, SchemaId, SchemaRef, SchemaVersion};
use ats_runtime::{
    BuildError, BuildRunner, BuildStepReport, BuildStepRequest, CancellationToken, PayloadError,
    RunLifecycleError, RunRecord, RunStatus, RunTransition, VersionedPayload,
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
        schema("feature.project-build-request")
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
pub struct ProjectBuildRequest {}

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
}

impl BuildRecipe {
    fn validate(&self) -> Result<(), ProjectBuildError> {
        if self.steps.is_empty() || self.steps.len() > 32 {
            return Err(ProjectBuildError::InvalidRecipe);
        }
        let mut ids = BTreeSet::new();
        for step in &self.steps {
            if !valid_id(&step.id) || !ids.insert(step.id.as_str()) {
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
        let recipe: BuildRecipe = context.contributions.decode(&build_slot())?;
        recipe.validate()?;
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

fn build_slot() -> ContributionId {
    ContributionId::parse("project.build.recipe").expect("built-in contribution ID is valid")
}

fn schema(id: &str) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(1).expect("built-in schema version is valid"),
    }
}
