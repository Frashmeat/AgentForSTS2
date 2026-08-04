use std::path::Path;

use ats_game_context::{
    ContributionResolverError, LoadedGamePack, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{ContributionId, FailureCode, FeatureId, SchemaId, SchemaRef, SchemaVersion};
use ats_runtime::{
    ArtifactPublisher, CancellationToken, ModelClient, PayloadError, ProjectFileWriter, RunId,
    RunLifecycleError, RunRecord, RunStatus, RunTransition, ValidationRunner, VersionedPayload,
};
use ats_workspace::{ResourceRepository, StoredItemDefinition};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;
use crate::mod_generate_single::{
    SingleGenerateContext, SingleGenerateDependencies, SingleGenerateError, SingleGenerateRequest,
    SingleGenerateResult, SingleGenerateService,
};
use crate::mod_plan::{ModPlanContext, ModPlanFeature, ModPlanRequest, ModPlanService, PlanItem};

pub struct BatchGenerateFeature;

impl FeatureSpec for BatchGenerateFeature {
    type Request = BatchGenerateRequest;
    type Result = BatchGenerateResult;
    type ArtifactExtension = BatchGenerateArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("mod.generate.batch").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema_version("feature.mod-generate-batch-request", 4)
    }

    fn result_schema() -> SchemaRef {
        schema_version("feature.mod-generate-batch-result", 2)
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.mod-generate-batch-artifact-extension")
    }
}

impl BatchGenerateFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: batch_slot(),
            schema: schema("pack.mod-generate-batch"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchGenerateRequest {
    pub mod_id: String,
    pub items: Vec<BatchDefinitionItem>,
    pub fail_fast: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchDefinitionItem {
    pub artifact_id: String,
    pub definition: StoredItemDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchGenerateResult {
    pub total: u32,
    pub processed: u32,
    pub succeeded: u32,
    pub failed: u32,
    pub items: Vec<BatchItemResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchItemResult {
    pub item_id: String,
    pub definition_hash: ats_kernel::Sha256Digest,
    pub plan_run_id: RunId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_run_id: Option<RunId>,
    pub status: RunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<PlanItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<SingleGenerateResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<FailureCode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BatchGenerateArtifactExtension {
    pub child_run_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchContribution {
    compose: FeatureId,
}

pub struct BatchGenerateContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub batch_contributions: &'a VerifiedContributionSet,
    pub plan_contributions: &'a VerifiedContributionSet,
    pub single_contributions: &'a VerifiedContributionSet,
    pub resource_contributions: &'a VerifiedContributionSet,
    pub truth: &'a VerifiedTruthSnapshot,
    pub project_root: &'a Path,
    pub project_context: &'a str,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
}

pub struct BatchGenerateExecution {
    pub result: BatchGenerateResult,
    pub child_runs: Vec<RunRecord>,
}

pub struct BatchGenerateService<'a> {
    plan: &'a ModPlanService,
    single: &'a SingleGenerateService,
}

impl<'a> BatchGenerateService<'a> {
    #[must_use]
    pub fn new(plan: &'a ModPlanService, single: &'a SingleGenerateService) -> Self {
        Self { plan, single }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute<C, R, W, V, A>(
        &self,
        dependencies: SingleGenerateDependencies<'_, C, R, W, V, A>,
        run: &mut RunRecord,
        request: BatchGenerateRequest,
        context: BatchGenerateContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<BatchGenerateExecution, BatchGenerateError>
    where
        C: ModelClient + ?Sized,
        R: ResourceRepository + ?Sized,
        W: ProjectFileWriter + ?Sized,
        V: ValidationRunner + ?Sized,
        A: ArtifactPublisher + ?Sized,
    {
        validate_run(run, &request)?;
        validate_context(&context)?;
        validate_batch_generation_input(&request)?;
        let contribution: BatchContribution = context.batch_contributions.decode(&batch_slot())?;
        if contribution.compose != crate::mod_generate_single::SingleGenerateFeature::id() {
            return Err(BatchGenerateError::InvalidContribution);
        }

        let total =
            u32::try_from(request.items.len()).map_err(|_| BatchGenerateError::InvalidInput)?;
        let mut child_runs = Vec::with_capacity(request.items.len() * 2);
        let mut items = Vec::with_capacity(request.items.len());
        for item in request.items {
            if cancellation.is_cancelled() {
                return Err(BatchGenerateError::Cancelled);
            }
            let plan_request = ModPlanRequest {
                requirements: item.definition.definition.behavior_intent.join("\n"),
                item_type: Some(item.definition.definition.item_type.to_string()),
            };
            let request_payload =
                VersionedPayload::from_typed(ModPlanFeature::request_schema(), &plan_request)?;
            let mut child = RunRecord::new(ModPlanFeature::id(), request_payload);
            child.apply_transition(RunTransition::Start, Utc::now())?;
            let plan_result = self
                .plan
                .execute(
                    dependencies.model,
                    plan_request,
                    ModPlanContext {
                        pack: context.pack,
                        contributions: context.plan_contributions,
                        project_context: Some(context.project_context),
                        custom_instructions: context.custom_instructions,
                        model: context.model.clone(),
                    },
                    cancellation,
                )
                .await;
            let mut plan = match plan_result {
                Ok(execution) => execution.item,
                Err(error) => {
                    let failure = error.run_failure();
                    let code = failure.code.clone();
                    if cancellation.is_cancelled() {
                        child.apply_transition(
                            RunTransition::Cancel {
                                reason: cancellation
                                    .reason()
                                    .expect("cancelled token has a reason"),
                            },
                            Utc::now(),
                        )?;
                    } else {
                        child.apply_transition(RunTransition::Fail { failure }, Utc::now())?;
                    }
                    items.push(BatchItemResult {
                        item_id: item.definition.definition.item_id.to_string(),
                        definition_hash: item.definition.definition_hash.clone(),
                        plan_run_id: child.id().clone(),
                        generation_run_id: None,
                        status: child.status(),
                        plan: None,
                        result: None,
                        failure_code: if cancellation.is_cancelled() {
                            None
                        } else {
                            Some(code)
                        },
                    });
                    child_runs.push(child);
                    if cancellation.is_cancelled() {
                        return Err(BatchGenerateError::Cancelled);
                    }
                    if request.fail_fast {
                        break;
                    }
                    continue;
                }
            };
            plan.item_id = item.definition.definition.item_id.to_string();
            plan.item_type = item.definition.definition.item_type.to_string();
            let plan_payload =
                VersionedPayload::from_typed(ModPlanFeature::result_schema(), &plan)?;
            child.apply_transition(
                RunTransition::Succeed {
                    result: plan_payload,
                },
                Utc::now(),
            )?;
            let plan_run_id = child.id().clone();
            child_runs.push(child);

            let single_request = SingleGenerateRequest {
                artifact_id: item.artifact_id,
                mod_id: request.mod_id.clone(),
                plan: plan.clone(),
                definition: item.definition.clone(),
            };
            let request_payload = VersionedPayload::from_typed(
                crate::mod_generate_single::SingleGenerateFeature::request_schema(),
                &single_request,
            )?;
            let mut child = RunRecord::new(
                crate::mod_generate_single::SingleGenerateFeature::id(),
                request_payload,
            );
            child.apply_transition(RunTransition::Start, Utc::now())?;
            let result = self
                .single
                .execute(
                    SingleGenerateDependencies {
                        model: dependencies.model,
                        resources: dependencies.resources,
                        writer: dependencies.writer,
                        validator: dependencies.validator,
                        artifacts: dependencies.artifacts,
                    },
                    &mut child,
                    single_request,
                    SingleGenerateContext {
                        pack: context.pack,
                        contributions: context.single_contributions,
                        resource_contributions: context.resource_contributions,
                        truth: context.truth,
                        project_root: context.project_root,
                        project_context: context.project_context,
                        custom_instructions: context.custom_instructions,
                        model: context.model.clone(),
                    },
                    cancellation,
                )
                .await;
            match result {
                Ok(execution) => items.push(BatchItemResult {
                    item_id: item.definition.definition.item_id.to_string(),
                    definition_hash: item.definition.definition_hash.clone(),
                    plan_run_id,
                    generation_run_id: Some(child.id().clone()),
                    status: child.status(),
                    plan: Some(plan),
                    result: Some(execution.result),
                    failure_code: None,
                }),
                Err(error) => {
                    let failure = error.run_failure();
                    let code = failure.code.clone();
                    if cancellation.is_cancelled() {
                        child.apply_transition(
                            RunTransition::Cancel {
                                reason: cancellation
                                    .reason()
                                    .expect("cancelled token has a reason"),
                            },
                            Utc::now(),
                        )?;
                    } else {
                        child.apply_transition(RunTransition::Fail { failure }, Utc::now())?;
                    }
                    items.push(BatchItemResult {
                        item_id: item.definition.definition.item_id.to_string(),
                        definition_hash: item.definition.definition_hash.clone(),
                        plan_run_id,
                        generation_run_id: Some(child.id().clone()),
                        status: child.status(),
                        plan: Some(plan),
                        result: None,
                        failure_code: if cancellation.is_cancelled() {
                            None
                        } else {
                            Some(code)
                        },
                    });
                    child_runs.push(child);
                    if cancellation.is_cancelled() {
                        return Err(BatchGenerateError::Cancelled);
                    }
                    if request.fail_fast {
                        break;
                    }
                    continue;
                }
            }
            child_runs.push(child);
        }
        let succeeded = u32::try_from(
            items
                .iter()
                .filter(|item| item.status == RunStatus::Succeeded)
                .count(),
        )
        .map_err(|_| BatchGenerateError::InvalidInput)?;
        let processed = u32::try_from(items.len()).map_err(|_| BatchGenerateError::InvalidInput)?;
        let failed = processed.saturating_sub(succeeded);
        let result = BatchGenerateResult {
            total,
            processed,
            succeeded,
            failed,
            items,
        };
        let payload = VersionedPayload::from_typed(BatchGenerateFeature::result_schema(), &result)?;
        run.apply_transition(RunTransition::Succeed { result: payload }, Utc::now())?;
        Ok(BatchGenerateExecution { result, child_runs })
    }
}

#[derive(Debug, Error)]
pub enum BatchGenerateError {
    #[error("batch generation input is invalid")]
    InvalidInput,
    #[error("batch generation Run does not match its typed request")]
    InvalidRun,
    #[error("batch generation context identities do not match")]
    ContextIdentityMismatch,
    #[error("batch generation Pack contribution is invalid")]
    InvalidContribution,
    #[error("batch generation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Single(#[from] SingleGenerateError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error(transparent)]
    Lifecycle(#[from] RunLifecycleError),
}

fn validate_run(run: &RunRecord, request: &BatchGenerateRequest) -> Result<(), BatchGenerateError> {
    if run.feature_id() != &BatchGenerateFeature::id() || run.status() != RunStatus::Running {
        return Err(BatchGenerateError::InvalidRun);
    }
    let persisted = run
        .request()
        .decode::<BatchGenerateRequest>(&BatchGenerateFeature::request_schema())
        .map_err(|_| BatchGenerateError::InvalidRun)?;
    if &persisted != request {
        return Err(BatchGenerateError::InvalidRun);
    }
    Ok(())
}

fn validate_context(context: &BatchGenerateContext<'_>) -> Result<(), BatchGenerateError> {
    for contributions in [
        context.batch_contributions,
        context.plan_contributions,
        context.single_contributions,
        context.resource_contributions,
    ] {
        if contributions.game_pack_id() != context.pack.id()
            || contributions.game_pack_sha256() != context.pack.content_sha256()
        {
            return Err(BatchGenerateError::ContextIdentityMismatch);
        }
    }
    if context.batch_contributions.feature_id() != &BatchGenerateFeature::id()
        || context.plan_contributions.feature_id() != &ModPlanFeature::id()
        || context.single_contributions.feature_id()
            != &crate::mod_generate_single::SingleGenerateFeature::id()
        || context.resource_contributions.feature_id()
            != &crate::resource_prepare::ResourcePrepareFeature::id()
    {
        return Err(BatchGenerateError::ContextIdentityMismatch);
    }
    Ok(())
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub fn validate_batch_generation_input(
    request: &BatchGenerateRequest,
) -> Result<(), BatchGenerateError> {
    if request.items.is_empty()
        || request.items.len() > 128
        || !valid_segment(&request.mod_id)
        || request.items.iter().any(|item| {
            !valid_segment(&item.artifact_id)
                || item.definition.validate().is_err()
                || item.definition.definition.behavior_intent.is_empty()
        })
    {
        Err(BatchGenerateError::InvalidInput)
    } else {
        Ok(())
    }
}

fn batch_slot() -> ContributionId {
    ContributionId::parse("mod.generate.batch").expect("built-in contribution ID is valid")
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
