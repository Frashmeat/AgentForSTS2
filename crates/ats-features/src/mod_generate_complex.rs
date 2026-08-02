use std::path::Path;

use ats_game_context::{
    ContributionResolverError, LoadedGamePack, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{ContributionId, FeatureId, SchemaId, SchemaRef, SchemaVersion};
use ats_runtime::{
    ArtifactPublisher, BuildRunner, CancellationToken, ModelClient, PackageWriter, PayloadError,
    ProjectFileWriter, RunId, RunLifecycleError, RunRecord, RunStatus, RunTransition,
    ValidationRunner, VersionedPayload,
};
use ats_workspace::ResourceRepository;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;
use crate::mod_generate_batch::{
    BatchGenerateContext, BatchGenerateError, BatchGenerateRequest, BatchGenerateResult,
    BatchGenerateService,
};
use crate::mod_generate_single::{
    SelectedResource, SingleGenerateDependencies, SingleGenerateRequest,
};
use crate::mod_plan::{
    ModPlanContext, ModPlanError, ModPlanFeature, ModPlanRequest, ModPlanService, PlanItem,
};
use crate::project_build::{
    ProjectBuildContext, ProjectBuildError, ProjectBuildFeature, ProjectBuildRequest,
    ProjectBuildResult, ProjectBuildService,
};
use crate::project_package::{
    ProjectPackageContext, ProjectPackageError, ProjectPackageFeature, ProjectPackageRequest,
    ProjectPackageResult, ProjectPackageService,
};

pub struct ComplexGenerateFeature;

impl FeatureSpec for ComplexGenerateFeature {
    type Request = ComplexGenerateRequest;
    type Result = ComplexGenerateResult;
    type ArtifactExtension = ComplexGenerateArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("mod.generate.complex").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema("feature.mod-generate-complex-request")
    }

    fn result_schema() -> SchemaRef {
        schema("feature.mod-generate-complex-result")
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.mod-generate-complex-artifact-extension")
    }
}

impl ComplexGenerateFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: complex_slot(),
            schema: schema("pack.mod-generate-complex"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComplexGenerateRequest {
    pub mod_id: String,
    pub planning_items: Vec<ComplexPlanningItem>,
    pub fail_fast: bool,
    pub package: ProjectPackageRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComplexPlanningItem {
    pub request: ModPlanRequest,
    pub artifact_id: String,
    pub selected_resources: Vec<SelectedResource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComplexGenerateResult {
    pub plans: Vec<PlanItem>,
    pub plan_run_ids: Vec<RunId>,
    pub batch_run_id: RunId,
    pub batch: BatchGenerateResult,
    pub build_run_id: RunId,
    pub build: ProjectBuildResult,
    pub package_run_id: RunId,
    pub package: ProjectPackageResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ComplexGenerateArtifactExtension {
    pub plan_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ComplexContribution {
    compose: FeatureId,
}

pub struct ComplexGenerateContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub complex_contributions: &'a VerifiedContributionSet,
    pub plan_contributions: &'a VerifiedContributionSet,
    pub batch_contributions: &'a VerifiedContributionSet,
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

pub struct ComplexGenerateDependencies<'a, C, R, W, V, A, B, P>
where
    C: ModelClient + ?Sized,
    R: ResourceRepository + ?Sized,
    W: ProjectFileWriter + ?Sized,
    V: ValidationRunner + ?Sized,
    A: ArtifactPublisher + ?Sized,
    B: BuildRunner + ?Sized,
    P: PackageWriter + ?Sized,
{
    pub model: &'a C,
    pub resources: &'a R,
    pub writer: &'a W,
    pub validator: &'a V,
    pub artifacts: &'a A,
    pub build_runner: &'a B,
    pub package_writer: &'a P,
}

pub struct ComplexGenerateExecution {
    pub result: ComplexGenerateResult,
    pub child_runs: Vec<RunRecord>,
}

pub struct ComplexGenerateService<'a> {
    plan: &'a ModPlanService,
    batch: &'a BatchGenerateService<'a>,
    build: &'a ProjectBuildService,
    package: &'a ProjectPackageService,
}

impl<'a> ComplexGenerateService<'a> {
    #[must_use]
    pub fn new(
        plan: &'a ModPlanService,
        batch: &'a BatchGenerateService<'a>,
        build: &'a ProjectBuildService,
        package: &'a ProjectPackageService,
    ) -> Self {
        Self {
            plan,
            batch,
            build,
            package,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C, R, W, V, A, B, P>(
        &self,
        dependencies: ComplexGenerateDependencies<'_, C, R, W, V, A, B, P>,
        run: &mut RunRecord,
        request: ComplexGenerateRequest,
        context: ComplexGenerateContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<ComplexGenerateExecution, ComplexGenerateError>
    where
        C: ModelClient + ?Sized,
        R: ResourceRepository + ?Sized,
        W: ProjectFileWriter + ?Sized,
        V: ValidationRunner + ?Sized,
        A: ArtifactPublisher + ?Sized,
        B: BuildRunner + ?Sized,
        P: PackageWriter + ?Sized,
    {
        validate_run(run, &request)?;
        validate_context(&context)?;
        if request.planning_items.is_empty()
            || request.planning_items.len() > 64
            || request.package.mod_id != request.mod_id
        {
            return Err(ComplexGenerateError::InvalidInput);
        }
        let contribution: ComplexContribution =
            context.complex_contributions.decode(&complex_slot())?;
        if contribution.compose != crate::mod_generate_batch::BatchGenerateFeature::id() {
            return Err(ComplexGenerateError::InvalidContribution);
        }

        let mut child_runs = Vec::new();
        let mut plans = Vec::with_capacity(request.planning_items.len());
        let mut singles = Vec::with_capacity(request.planning_items.len());
        for item in &request.planning_items {
            check_cancelled(cancellation)?;
            let mut plan_run = running_run::<ModPlanFeature, _>(&item.request)?;
            let execution = self
                .plan
                .execute(
                    dependencies.model,
                    item.request.clone(),
                    ModPlanContext {
                        pack: context.pack,
                        contributions: context.plan_contributions,
                        project_context: Some(context.project_context),
                        custom_instructions: context.custom_instructions,
                        model: context.model.clone(),
                    },
                )
                .await?;
            let plan_payload =
                VersionedPayload::from_typed(ModPlanFeature::result_schema(), &execution.item)?;
            plan_run.apply_transition(
                RunTransition::Succeed {
                    result: plan_payload,
                },
                Utc::now(),
            )?;
            singles.push(SingleGenerateRequest {
                artifact_id: item.artifact_id.clone(),
                mod_id: request.mod_id.clone(),
                plan: execution.item.clone(),
                selected_resources: item.selected_resources.clone(),
            });
            plans.push(execution.item);
            child_runs.push(plan_run);
        }

        check_cancelled(cancellation)?;
        let batch_request = BatchGenerateRequest {
            items: singles,
            fail_fast: request.fail_fast,
        };
        let mut batch_run =
            running_run::<crate::mod_generate_batch::BatchGenerateFeature, _>(&batch_request)?;
        let batch_execution = self
            .batch
            .execute(
                SingleGenerateDependencies {
                    model: dependencies.model,
                    resources: dependencies.resources,
                    writer: dependencies.writer,
                    validator: dependencies.validator,
                    artifacts: dependencies.artifacts,
                },
                &mut batch_run,
                batch_request,
                BatchGenerateContext {
                    pack: context.pack,
                    batch_contributions: context.batch_contributions,
                    single_contributions: context.single_contributions,
                    resource_contributions: context.resource_contributions,
                    truth: context.truth,
                    project_root: context.project_root,
                    project_context: context.project_context,
                    custom_instructions: context.custom_instructions,
                    model: context.model.clone(),
                },
                cancellation,
            )
            .await?;
        let batch_run_id = batch_run.id().clone();
        child_runs.extend(batch_execution.child_runs);
        child_runs.push(batch_run);

        check_cancelled(cancellation)?;
        let build_request = ProjectBuildRequest {};
        let mut build_run = running_run::<ProjectBuildFeature, _>(&build_request)?;
        let build_result = self
            .build
            .execute(
                dependencies.build_runner,
                &mut build_run,
                build_request,
                ProjectBuildContext {
                    pack: context.pack,
                    contributions: context.build_contributions,
                    project_root: context.project_root,
                },
                cancellation,
            )
            .await?;
        let build_run_id = build_run.id().clone();
        child_runs.push(build_run);

        check_cancelled(cancellation)?;
        let mut package_run = running_run::<ProjectPackageFeature, _>(&request.package)?;
        let package_result = self.package.execute(
            dependencies.package_writer,
            dependencies.artifacts,
            &mut package_run,
            request.package,
            ProjectPackageContext {
                pack: context.pack,
                contributions: context.package_contributions,
                project_root: context.project_root,
            },
            cancellation,
        )?;
        let package_run_id = package_run.id().clone();
        child_runs.push(package_run);

        let plan_run_ids = child_runs
            .iter()
            .filter(|child| child.feature_id() == &ModPlanFeature::id())
            .map(|child| child.id().clone())
            .collect();
        let result = ComplexGenerateResult {
            plans,
            plan_run_ids,
            batch_run_id,
            batch: batch_execution.result,
            build_run_id,
            build: build_result,
            package_run_id,
            package: package_result,
        };
        let payload =
            VersionedPayload::from_typed(ComplexGenerateFeature::result_schema(), &result)?;
        run.apply_transition(RunTransition::Succeed { result: payload }, Utc::now())?;
        Ok(ComplexGenerateExecution { result, child_runs })
    }
}

#[derive(Debug, Error)]
pub enum ComplexGenerateError {
    #[error("complex generation input is invalid")]
    InvalidInput,
    #[error("complex generation Run does not match its typed request")]
    InvalidRun,
    #[error("complex generation context identities do not match")]
    ContextIdentityMismatch,
    #[error("complex generation Pack contribution is invalid")]
    InvalidContribution,
    #[error("complex generation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Plan(#[from] ModPlanError),
    #[error(transparent)]
    Batch(#[from] BatchGenerateError),
    #[error(transparent)]
    Build(#[from] ProjectBuildError),
    #[error(transparent)]
    Package(#[from] ProjectPackageError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    Lifecycle(#[from] RunLifecycleError),
}

fn running_run<S, T>(request: &T) -> Result<RunRecord, ComplexGenerateError>
where
    S: FeatureSpec,
    T: Serialize,
{
    let payload = VersionedPayload::from_typed(S::request_schema(), request)?;
    let mut run = RunRecord::new(S::id(), payload);
    run.apply_transition(RunTransition::Start, Utc::now())?;
    Ok(run)
}

fn validate_run(
    run: &RunRecord,
    request: &ComplexGenerateRequest,
) -> Result<(), ComplexGenerateError> {
    if run.feature_id() != &ComplexGenerateFeature::id() || run.status() != RunStatus::Running {
        return Err(ComplexGenerateError::InvalidRun);
    }
    let persisted = run
        .request()
        .decode::<ComplexGenerateRequest>(&ComplexGenerateFeature::request_schema())
        .map_err(|_| ComplexGenerateError::InvalidRun)?;
    if &persisted != request {
        return Err(ComplexGenerateError::InvalidRun);
    }
    Ok(())
}

fn validate_context(context: &ComplexGenerateContext<'_>) -> Result<(), ComplexGenerateError> {
    let expected = [
        (context.complex_contributions, ComplexGenerateFeature::id()),
        (context.plan_contributions, ModPlanFeature::id()),
        (
            context.batch_contributions,
            crate::mod_generate_batch::BatchGenerateFeature::id(),
        ),
        (
            context.single_contributions,
            crate::mod_generate_single::SingleGenerateFeature::id(),
        ),
        (
            context.resource_contributions,
            crate::resource_prepare::ResourcePrepareFeature::id(),
        ),
        (context.build_contributions, ProjectBuildFeature::id()),
        (context.package_contributions, ProjectPackageFeature::id()),
    ];
    if expected.iter().any(|(contributions, feature)| {
        contributions.feature_id() != feature
            || contributions.game_pack_id() != context.pack.id()
            || contributions.game_pack_sha256() != context.pack.content_sha256()
    }) {
        return Err(ComplexGenerateError::ContextIdentityMismatch);
    }
    Ok(())
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), ComplexGenerateError> {
    if cancellation.is_cancelled() {
        Err(ComplexGenerateError::Cancelled)
    } else {
        Ok(())
    }
}

fn complex_slot() -> ContributionId {
    ContributionId::parse("mod.generate.complex").expect("built-in contribution ID is valid")
}

fn schema(id: &str) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(1).expect("built-in schema version is valid"),
    }
}
