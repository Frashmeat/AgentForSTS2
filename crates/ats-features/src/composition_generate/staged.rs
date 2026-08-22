use std::collections::{BTreeMap, BTreeSet};

use ats_game_context::{
    BehaviorAdapterIdentity, BehaviorProposal, CapabilityCatalogIdentity, PipelineCheckpointPolicy,
    PipelineNodePhase, PipelineNodeScope, PipelineResolveRequest, PipelineWorkItem, RenderedFile,
    RenderedFileMerge, RenderedFileMergeKeyPolicy, RenderedItemBundle, ResolvedPipelineGraph,
};
use ats_kernel::{ExecutionNodeId, GamePackId, ItemId};
use ats_runtime::{
    ExecutionAdjustment, ExecutionCommitIntent, ExecutionFailure, ExecutionGraphRecord,
    ExecutionGraphRepository, ExecutionGraphRepositoryError, ExecutionGraphStatus,
    ExecutionNodeSpec, ExecutionNodeStatus, ExecutionRepairTargetSpec, HashedExecutionPayload,
    ProjectFileWrite, RunRepository, RunRepositoryError, hash_json,
};

use super::behavior::{BehaviorGenerationContext, BehaviorGenerationService};
use super::render::{RenderItemRequest, render_item};
use super::*;
const BLUEPRINT_SCHEMA_ID: &str = "feature.composition-generate-blueprint";
const PLAN_CHECKPOINT_SCHEMA_ID: &str = "feature.composition-generate-plan-checkpoint";
const BEHAVIOR_CHECKPOINT_SCHEMA_ID: &str = "feature.composition-generate-behavior-checkpoint";
const RENDER_CHECKPOINT_SCHEMA_ID: &str = "feature.composition-generate-render-checkpoint";
const FINALIZE_CHECKPOINT_SCHEMA_ID: &str = "feature.composition-generate-finalize-checkpoint";
const COMMIT_INTENT_SCHEMA_ID: &str = "feature.composition-generate-commit-intent";

#[derive(Debug, Clone)]
pub struct StagedCompositionGenerateStart {
    pub request: CompositionGenerateRequest,
    pub graph: ExecutionGraphRecord,
}

pub struct StagedCompositionGenerateExecution {
    pub result: CompositionGenerateResult,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompositionAdjustmentItem {
    pub item_id: ItemId,
    pub definition_hash: Sha256Digest,
    pub behavior_sha256: Sha256Digest,
}

pub fn composition_adjustment_items(
    graph: &ExecutionGraphRecord,
) -> Result<Vec<CompositionAdjustmentItem>, CompositionGenerateError> {
    if graph.owner_feature_id() != &CompositionGenerateFeature::id()
        || !matches!(graph.status(), ExecutionGraphStatus::Succeeded)
        || graph.repair_campaign().is_some()
    {
        return Ok(Vec::new());
    }
    let blueprint: StagedCompositionGenerateBlueprint = graph
        .blueprint()
        .payload
        .decode(&blueprint_schema())
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    blueprint
        .items
        .into_iter()
        .map(
            |item| -> Result<CompositionAdjustmentItem, CompositionGenerateError> {
                let behavior_sha256 = graph
                    .nodes()
                    .get(&item.behavior_node_id)
                    .and_then(|node| node.active_checkpoint.as_ref())
                    .and_then(|checkpoint| {
                        checkpoint
                            .payload
                            .decode::<StagedBehaviorCheckpoint>(&behavior_checkpoint_schema())
                            .ok()
                    })
                    .map(|checkpoint| checkpoint.behavior_sha256)
                    .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
                Ok(CompositionAdjustmentItem {
                    item_id: item.item_id,
                    definition_hash: item.definition_hash,
                    behavior_sha256,
                })
            },
        )
        .collect::<Result<Vec<_>, _>>()
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedCompositionGenerateBlueprint {
    schema_version: u32,
    model_request_limits: ats_runtime::ModelRequestLimits,
    game_pack_id: GamePackId,
    game_pack_sha256: Sha256Digest,
    truth_snapshot_id: Sha256Digest,
    request: CompositionGenerateRequest,
    graph_digest: Sha256Digest,
    pipeline: ResolvedPipelineGraph,
    prepare: CompiledPreparePipeline,
    items: Vec<StagedCompositionGenerateItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum CompiledPreparePipeline {
    ItemGeneration { output_node_id: ExecutionNodeId },
    DataJson { output_node_id: ExecutionNodeId },
}

impl CompiledPreparePipeline {
    fn output_node_id(&self) -> &ExecutionNodeId {
        match self {
            Self::ItemGeneration { output_node_id } | Self::DataJson { output_node_id } => {
                output_node_id
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CompiledDeliveryPipeline {
    validation: Option<(ExecutionNodeId, ats_kernel::PrimitiveId)>,
    build_node_id: Option<ExecutionNodeId>,
    package_node_id: Option<ExecutionNodeId>,
    publish_node_id: ExecutionNodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedCompositionGenerateItem {
    item_id: ItemId,
    definition_hash: Sha256Digest,
    plan_node_id: ExecutionNodeId,
    behavior_node_id: ExecutionNodeId,
    render_node_id: ExecutionNodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedPlanCheckpoint {
    plan: crate::mod_plan::PlanItem,
    child_run: RunRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedBehaviorCheckpoint {
    pack_id: GamePackId,
    pack_sha256: Sha256Digest,
    truth_snapshot_id: Sha256Digest,
    catalog: CapabilityCatalogIdentity,
    adapter: BehaviorAdapterIdentity,
    definition_hash: Sha256Digest,
    request_commitment: ats_runtime::ModelRequestCommitment,
    response_model: String,
    usage: ats_runtime::TokenUsage,
    behavior_sha256: Sha256Digest,
    proposal: BehaviorProposal,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedRenderCheckpoint {
    pack_id: GamePackId,
    pack_sha256: Sha256Digest,
    truth_snapshot_id: Sha256Digest,
    catalog: CapabilityCatalogIdentity,
    adapter: BehaviorAdapterIdentity,
    behavior_sha256: Sha256Digest,
    rendered_bundle_sha256: Sha256Digest,
    bundle: RenderedItemBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedFinalizeCheckpoint {
    graph_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    validation_primitive: Option<ats_kernel::PrimitiveId>,
    generated_file_count: u32,
    items: Vec<CompositionItemRunResult>,
}

type PreparedCompositionAssembly = Result<
    (
        Vec<CompositionBehaviorProvenance>,
        Vec<ProjectFileWrite>,
        Vec<ProposedArtifactFile>,
        StagedFinalizeCheckpoint,
    ),
    CompositionGenerateError,
>;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedPublicationIntent {
    execution_graph_id: ExecutionGraphId,
    artifact_id: String,
    graph_digest: Sha256Digest,
    generated_file_count: u32,
}

impl CompositionGenerateService<'_> {
    pub fn prepare_staged_start<I, R>(
        &self,
        mut request: CompositionGenerateRequest,
        context: CompositionGenerateContext<'_>,
        items: &I,
        resources: &R,
        run_id: RunId,
    ) -> Result<StagedCompositionGenerateStart, CompositionGenerateError>
    where
        I: ItemRepository + ?Sized,
        R: ResourceRepository + ?Sized,
    {
        validate_context(&context)?;
        validate_request(&request)?;
        if request.execution.is_some() {
            return Err(CompositionGenerateError::InvalidInput);
        }
        let contribution: CompositionGenerateContribution = context
            .composition_contributions
            .decode(&generation_slot())?;
        let resolved = ResolvedItemGraph::resolve(
            context.pack,
            context.truth,
            context.resource_contributions,
            items,
            resources,
            request.root.clone(),
            request.draft.clone(),
        )?;
        let execution_graph_id = ExecutionGraphId::new();
        let mut blueprint_request = request.clone();
        blueprint_request.execution = None;
        let pipeline_request = pipeline_request(&resolved, &contribution.pipeline.profile_id);
        let pipeline = self
            .pipelines
            .resolve(&contribution.pipeline, &pipeline_request)?;
        let (prepare, staged_items, _, specs) = compile_generation_nodes(&pipeline, &resolved)?;
        let blueprint = StagedCompositionGenerateBlueprint {
            schema_version: 8,
            model_request_limits: context.model_request_limits,
            game_pack_id: context.pack.id().clone(),
            game_pack_sha256: context.pack.content_sha256().clone(),
            truth_snapshot_id: context.truth.manifest().snapshot_id().clone(),
            request: blueprint_request,
            graph_digest: resolved.graph_digest.clone(),
            pipeline,
            prepare,
            items: staged_items,
        };
        blueprint.validate(self.pipelines, &context, &resolved, &request)?;
        request.execution = Some(CompositionGenerateExecutionRequest::Start {
            execution_graph_id: execution_graph_id.clone(),
        });
        let request_payload =
            VersionedPayload::from_typed(CompositionGenerateFeature::request_schema(), &request)?;
        let graph = ExecutionGraphRecord::new_claimed(
            execution_graph_id,
            CompositionGenerateFeature::id(),
            hash_json(&request_payload).map_err(|_| CompositionGenerateError::InvalidInput)?,
            VersionedPayload::from_typed(blueprint_schema(), &blueprint)?,
            specs,
            run_id,
            Utc::now(),
        )
        .map_err(|_| CompositionGenerateError::InvalidInput)?;
        Ok(StagedCompositionGenerateStart { request, graph })
    }

    pub fn prepare_staged_resume(
        &self,
        mut graph: ExecutionGraphRecord,
        expected_revision: u64,
        run_id: RunId,
        context: CompositionGenerateContext<'_>,
    ) -> Result<StagedCompositionGenerateStart, CompositionGenerateError> {
        validate_context(&context)?;
        if graph.owner_feature_id() != &CompositionGenerateFeature::id()
            || graph.revision() != expected_revision
        {
            return Err(CompositionGenerateError::ExecutionGraphConflict);
        }
        let blueprint: StagedCompositionGenerateBlueprint = graph
            .blueprint()
            .payload
            .decode(&blueprint_schema())
            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
        blueprint.validate_identity(self.pipelines, &context)?;
        let previous_run_id = graph
            .previous_run_id()
            .cloned()
            .ok_or(CompositionGenerateError::ExecutionGraphConflict)?;
        if graph.status() != ExecutionGraphStatus::Succeeded {
            graph
                .claim(
                    expected_revision,
                    run_id,
                    previous_run_id.clone(),
                    Utc::now(),
                )
                .map_err(|_| CompositionGenerateError::ExecutionGraphConflict)?;
        }
        let mut request = blueprint.request;
        request.execution = Some(CompositionGenerateExecutionRequest::Resume {
            execution_graph_id: graph.id().clone(),
            expected_revision,
            previous_run_id,
        });
        Ok(StagedCompositionGenerateStart { request, graph })
    }

    pub fn prepare_staged_adjustment(
        &self,
        graph: ExecutionGraphRecord,
        expected_revision: u64,
        run_id: RunId,
        adjustment: HumanSemanticFeedbackRef,
        context: CompositionGenerateContext<'_>,
    ) -> Result<StagedCompositionGenerateStart, CompositionGenerateError> {
        validate_context(&context)?;
        adjustment.validate()?;
        if graph.owner_feature_id() != &CompositionGenerateFeature::id()
            || graph.revision() != expected_revision
            || graph.status() != ExecutionGraphStatus::Succeeded
            || adjustment.source_execution_graph_id != *graph.id()
        {
            return Err(CompositionGenerateError::AdjustmentRequiresReplan);
        }
        let blueprint: StagedCompositionGenerateBlueprint = graph
            .blueprint()
            .payload
            .decode(&blueprint_schema())
            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
        blueprint.validate_identity(self.pipelines, &context)?;
        let item = blueprint
            .items
            .iter()
            .find(|item| item.item_id == adjustment.item_id)
            .ok_or(CompositionGenerateError::AdjustmentInvalid)?;
        if item.definition_hash != adjustment.expected_definition_hash {
            return Err(CompositionGenerateError::AdjustmentStale);
        }
        let checkpoint_hash = graph.nodes()[&item.behavior_node_id]
            .active_checkpoint
            .as_ref()
            .ok_or(CompositionGenerateError::InvalidCheckpoint)?
            .sha256
            .clone();
        let behavior_checkpoint: StagedBehaviorCheckpoint = graph.nodes()[&item.behavior_node_id]
            .active_checkpoint
            .as_ref()
            .ok_or(CompositionGenerateError::InvalidCheckpoint)?
            .payload
            .decode(&behavior_checkpoint_schema())
            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
        if behavior_checkpoint.behavior_sha256 != adjustment.expected_behavior_sha256 {
            return Err(CompositionGenerateError::AdjustmentStale);
        }
        let feedback = VersionedPayload::from_typed(item_adjustment_schema(), &adjustment)?;
        let runtime_adjustment = ExecutionAdjustment {
            item_id: adjustment.item_id.clone(),
            expected_definition_hash: adjustment.expected_definition_hash.clone(),
            instruction_sha256: adjustment.instruction_sha256.clone(),
            created_at: adjustment.created_at,
            feedback: HashedExecutionPayload::new(feedback.clone())
                .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?,
        };
        let adjustment_item_id = adjustment.item_id.clone();
        let instruction_sha256 = adjustment.instruction_sha256.clone();
        let mut request = blueprint.request;
        request.adjustment = Some(adjustment);
        let target = ExecutionRepairTargetSpec {
            item_id: adjustment_item_id,
            node_id: item.behavior_node_id.clone(),
            checkpoint_hash,
            diagnostic_fingerprints: vec![instruction_sha256.clone()],
            feedback,
        };
        match graph.status() {
            ExecutionGraphStatus::Succeeded => {
                let execution_graph_id = ExecutionGraphId::new();
                request.execution = Some(CompositionGenerateExecutionRequest::Start {
                    execution_graph_id: execution_graph_id.clone(),
                });
                let request_payload = VersionedPayload::from_typed(
                    CompositionGenerateFeature::request_schema(),
                    &request,
                )?;
                let derived = graph
                    .derive_adjustment(
                        execution_graph_id,
                        hash_json(&request_payload)
                            .map_err(|_| CompositionGenerateError::InvalidInput)?,
                        run_id,
                        instruction_sha256,
                        target,
                        runtime_adjustment,
                        Utc::now(),
                    )
                    .map_err(|_| CompositionGenerateError::AdjustmentRequiresReplan)?;
                Ok(StagedCompositionGenerateStart {
                    request,
                    graph: derived,
                })
            }
            _ => Err(CompositionGenerateError::AdjustmentRequiresReplan),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute_staged<C, I, R, W, S, V, A, B, P, G, RR>(
        &self,
        dependencies: CompositionGenerateDependencies<'_, C, I, R, W, S, V, A, B, P>,
        runs: &RR,
        graphs: &G,
        run: &mut RunRecord,
        request: CompositionGenerateRequest,
        context: CompositionGenerateContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<StagedCompositionGenerateExecution, CompositionGenerateError>
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
        G: ExecutionGraphRepository + ?Sized,
        RR: RunRepository + ?Sized,
    {
        self.execute_staged_with_input(
            dependencies,
            runs,
            graphs,
            run,
            request,
            context,
            EphemeralCompositionInput::None,
            cancellation,
        )
        .await
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub async fn execute_staged_with_input<C, I, R, W, S, V, A, B, P, G, RR>(
        &self,
        dependencies: CompositionGenerateDependencies<'_, C, I, R, W, S, V, A, B, P>,
        runs: &RR,
        graphs: &G,
        run: &mut RunRecord,
        request: CompositionGenerateRequest,
        context: CompositionGenerateContext<'_>,
        ephemeral_input: EphemeralCompositionInput,
        cancellation: &CancellationToken,
    ) -> Result<StagedCompositionGenerateExecution, CompositionGenerateError>
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
        G: ExecutionGraphRepository + ?Sized,
        RR: RunRepository + ?Sized,
    {
        validate_run(run, &request)?;
        validate_context(&context)?;
        validate_request(&request)?;
        let graph_id = execution_graph_id(&request)?;
        let mut graph = graphs.get(graph_id).map_err(map_graph_repository_error)?;
        if graph.owner_feature_id() != &CompositionGenerateFeature::id() {
            return Err(CompositionGenerateError::ExecutionGraphConflict);
        }
        let blueprint: StagedCompositionGenerateBlueprint = graph
            .blueprint()
            .payload
            .decode(&blueprint_schema())
            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
        blueprint.validate_identity(self.pipelines, &context)?;
        let resolved = ResolvedItemGraph::resolve(
            context.pack,
            context.truth,
            context.resource_contributions,
            dependencies.items,
            dependencies.resources,
            request.root.clone(),
            request.draft.clone(),
        )?;
        blueprint.validate(self.pipelines, &context, &resolved, &request)?;
        match request.execution.as_ref() {
            Some(CompositionGenerateExecutionRequest::Start { .. })
                if graph.previous_run_id().is_none() && request.adjustment.is_none() => {}
            Some(CompositionGenerateExecutionRequest::Start { .. })
                if request.adjustment.is_some()
                    && graph.repair_campaign().is_some_and(|campaign| {
                        matches!(
                            &campaign.cause,
                            ats_runtime::RepairCause::HumanSemanticFeedback { .. }
                        ) && campaign.targets.len() == 1
                    }) => {}
            Some(CompositionGenerateExecutionRequest::Resume {
                previous_run_id, ..
            }) if graph.previous_run_id() == Some(previous_run_id) => {}
            _ => return Err(CompositionGenerateError::InvalidCheckpoint),
        }
        if graph.status() == ExecutionGraphStatus::Succeeded {
            let result: CompositionGenerateResult = graph
                .final_result_ref()
                .ok_or(CompositionGenerateError::InvalidCheckpoint)?
                .payload
                .decode(&CompositionGenerateFeature::result_schema())
                .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
            succeed_parent(run, &result)?;
            return Ok(StagedCompositionGenerateExecution { result });
        }
        if graph.active_run_id() != Some(run.id()) {
            return Err(CompositionGenerateError::ExecutionGraphConflict);
        }

        if graph.status() == ExecutionGraphStatus::Running {
            for (item, definition) in blueprint.items.iter().zip(&resolved.nodes) {
                handle_cancellation(&mut graph, graphs, run.id(), cancellation)?;
                let plan_checkpoint = if node_status(&graph, &item.plan_node_id)?
                    == ExecutionNodeStatus::Succeeded
                {
                    let checkpoint = decode_plan_checkpoint(&graph, item, definition)?;
                    persist_completed_child(
                        runs,
                        &checkpoint.child_run,
                        &mut graph,
                        graphs,
                        run.id(),
                    )?;
                    checkpoint
                } else {
                    let plan_request = ModPlanRequest {
                        requirements: definition.definition.behavior_intent.join("\n"),
                        item_type: Some(definition.definition.item_type.to_string()),
                    };
                    bind_and_start_node(
                        &mut graph,
                        graphs,
                        &item.plan_node_id,
                        run.id(),
                        request_hash::<ModPlanFeature, _>(&plan_request)?,
                    )?;
                    let mut child_run = running_run::<ModPlanFeature, _>(&plan_request)?;
                    let execution = match self
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
                                model_request_limits: blueprint.model_request_limits,
                                authoritative_definition: Some(definition),
                            },
                            cancellation,
                        )
                        .await
                    {
                        Ok(execution) => execution,
                        Err(error) => {
                            finish_failed_child(&mut child_run, error.run_failure(), cancellation)?;
                            if let Err(storage_error) = persist_child(runs, &child_run) {
                                pause_failed_node(
                                    &mut graph,
                                    graphs,
                                    &item.plan_node_id,
                                    run.id(),
                                    &storage_error,
                                )?;
                                return Err(storage_error);
                            }
                            if cancellation.is_cancelled() {
                                return handle_cancellation(
                                    &mut graph,
                                    graphs,
                                    run.id(),
                                    cancellation,
                                )
                                .and(Err(CompositionGenerateError::Cancelled));
                            }
                            let error = CompositionGenerateError::Plan(error);
                            pause_failed_node(
                                &mut graph,
                                graphs,
                                &item.plan_node_id,
                                run.id(),
                                &error,
                            )?;
                            return Err(error);
                        }
                    };
                    let mut plan = execution.item;
                    plan.item_id = definition.definition.item_id.to_string();
                    plan.item_type = definition.definition.item_type.to_string();
                    plan.behavior_intent = definition.definition.behavior_intent.clone();
                    succeed_child::<ModPlanFeature, _>(&mut child_run, &plan)?;
                    let checkpoint = StagedPlanCheckpoint { plan, child_run };
                    complete_node(
                        &mut graph,
                        graphs,
                        &item.plan_node_id,
                        run.id(),
                        VersionedPayload::from_typed(plan_checkpoint_schema(), &checkpoint)?,
                    )?;
                    persist_completed_child(
                        runs,
                        &checkpoint.child_run,
                        &mut graph,
                        graphs,
                        run.id(),
                    )?;
                    checkpoint
                };

                handle_cancellation(&mut graph, graphs, run.id(), cancellation)?;
                let behavior_checkpoint = if node_status(&graph, &item.behavior_node_id)?
                    == ExecutionNodeStatus::Succeeded
                {
                    decode_behavior_checkpoint(&graph, item, definition, &context)?
                } else {
                    let behavior = BehaviorGenerationService::built_in()?;
                    let mut first_request = true;
                    let generated = loop {
                        let feedback = graph.nodes()[&item.behavior_node_id]
                            .feedback_state
                            .as_ref()
                            .filter(|state| {
                                state.phase == ats_runtime::ExecutionFeedbackPhase::OutputContract
                            })
                            .map(|state| {
                                state
                                    .feedback
                                    .payload
                                    .decode::<super::behavior::BehaviorFeedback>(
                                        &behavior_feedback_schema(),
                                    )
                            })
                            .transpose()
                            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
                        let behavior_context = BehaviorGenerationContext {
                            pack: context.pack,
                            truth_snapshot_id: context.truth.manifest().snapshot_id(),
                            definition,
                            plan: &plan_checkpoint.plan,
                            project_context: context.project_context,
                            custom_instructions: context.custom_instructions,
                            model: context.model.clone(),
                            model_request_limits: blueprint.model_request_limits,
                            feedback: feedback.as_ref(),
                            human_semantic_feedback: None,
                        };
                        let snapshot = behavior.prepare(&behavior_context)?;
                        if first_request {
                            bind_and_start_node(
                                &mut graph,
                                graphs,
                                &item.behavior_node_id,
                                run.id(),
                                snapshot.request_sha256().clone(),
                            )?;
                            if feedback.is_none() {
                                mutate_graph(&mut graph, graphs, |graph| {
                                    graph.record_semantic_baseline_request(
                                        &item.behavior_node_id,
                                        run.id(),
                                        Utc::now(),
                                    )
                                })?;
                            }
                            first_request = false;
                        } else {
                            mutate_graph(&mut graph, graphs, |graph| {
                                graph.update_running_node_request_snapshot_hash(
                                    &item.behavior_node_id,
                                    run.id(),
                                    snapshot.request_sha256().clone(),
                                    Utc::now(),
                                )
                            })?;
                        }
                        match behavior
                            .generate(dependencies.model, behavior_context, snapshot, cancellation)
                            .await
                        {
                            Ok(generated) => break generated,
                            Err(error) => {
                                if cancellation.is_cancelled() {
                                    return handle_cancellation(
                                        &mut graph,
                                        graphs,
                                        run.id(),
                                        cancellation,
                                    )
                                    .and(Err(CompositionGenerateError::Cancelled));
                                }
                                let Some(evidence) = error.feedback_evidence() else {
                                    let error = CompositionGenerateError::Behavior(error);
                                    pause_failed_node(
                                        &mut graph,
                                        graphs,
                                        &item.behavior_node_id,
                                        run.id(),
                                        &error,
                                    )?;
                                    return Err(error);
                                };
                                let completed_rounds = graph.nodes()[&item.behavior_node_id]
                                    .feedback_state
                                    .as_ref()
                                    .map_or(0, |state| state.round);
                                if !request.repair_policy.permits(completed_rounds) {
                                    pause_running_node_with_code(
                                        &mut graph,
                                        graphs,
                                        &item.behavior_node_id,
                                        run.id(),
                                        "model.feedback_exhausted",
                                    )?;
                                    return Err(CompositionGenerateError::Behavior(error));
                                }
                                let fingerprint = hash_json(&evidence.feedback)
                                    .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
                                if graph.nodes()[&item.behavior_node_id]
                                    .feedback_state
                                    .as_ref()
                                    .is_some_and(|state| {
                                        state.diagnostic_fingerprint == fingerprint
                                            && state.candidate_sha256.as_ref()
                                                == Some(&evidence.candidate_sha256)
                                    })
                                {
                                    pause_running_node_with_code(
                                        &mut graph,
                                        graphs,
                                        &item.behavior_node_id,
                                        run.id(),
                                        "model.feedback_no_progress",
                                    )?;
                                    return Err(CompositionGenerateError::Behavior(error));
                                }
                                mutate_graph(&mut graph, graphs, |graph| {
                                    graph.record_output_feedback(
                                        &item.behavior_node_id,
                                        run.id(),
                                        ats_runtime::ExecutionOutputFeedback {
                                            diagnostic_fingerprint: fingerprint,
                                            candidate_sha256: evidence.candidate_sha256,
                                            feedback: VersionedPayload::from_typed(
                                                behavior_feedback_schema(),
                                                &evidence.feedback,
                                            )
                                            .map_err(|_| {
                                                ats_runtime::ExecutionGraphError::InvalidMetadata
                                            })?,
                                        },
                                        Utc::now(),
                                    )
                                })?;
                            }
                        }
                    };
                    let behavior_sha256 = generated
                        .proposal
                        .sha256()
                        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
                    let checkpoint = StagedBehaviorCheckpoint {
                        pack_id: context.pack.id().clone(),
                        pack_sha256: context.pack.content_sha256().clone(),
                        truth_snapshot_id: context.truth.manifest().snapshot_id().clone(),
                        catalog: context.pack.capability_catalog_identity().clone(),
                        adapter: context.pack.behavior_adapter().clone(),
                        definition_hash: definition.definition_hash.clone(),
                        request_commitment: generated.request_snapshot.commitment().clone(),
                        response_model: generated.response_model,
                        usage: generated.usage,
                        behavior_sha256,
                        proposal: generated.proposal,
                    };
                    complete_node(
                        &mut graph,
                        graphs,
                        &item.behavior_node_id,
                        run.id(),
                        VersionedPayload::from_typed(behavior_checkpoint_schema(), &checkpoint)?,
                    )?;
                    checkpoint
                };

                handle_cancellation(&mut graph, graphs, run.id(), cancellation)?;
                if node_status(&graph, &item.render_node_id)? != ExecutionNodeStatus::Succeeded {
                    bind_and_start_node(
                        &mut graph,
                        graphs,
                        &item.render_node_id,
                        run.id(),
                        behavior_checkpoint.behavior_sha256.clone(),
                    )?;
                    let (bundle, _) = match render_item(
                        dependencies.behavior_adapters,
                        context.pack,
                        context.resource_contributions,
                        dependencies.resources,
                        RenderItemRequest {
                            graph: &resolved,
                            definition,
                            mod_id: &request.mod_id,
                            proposal: &behavior_checkpoint.proposal,
                        },
                    ) {
                        Ok(rendered) => rendered,
                        Err(error) => {
                            let error = CompositionGenerateError::Render(error);
                            pause_failed_node(
                                &mut graph,
                                graphs,
                                &item.render_node_id,
                                run.id(),
                                &error,
                            )?;
                            return Err(error);
                        }
                    };
                    let checkpoint = StagedRenderCheckpoint {
                        pack_id: context.pack.id().clone(),
                        pack_sha256: context.pack.content_sha256().clone(),
                        truth_snapshot_id: context.truth.manifest().snapshot_id().clone(),
                        catalog: context.pack.capability_catalog_identity().clone(),
                        adapter: context.pack.behavior_adapter().clone(),
                        behavior_sha256: behavior_checkpoint.behavior_sha256.clone(),
                        rendered_bundle_sha256: hash_json(&bundle)
                            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?,
                        bundle,
                    };
                    complete_node(
                        &mut graph,
                        graphs,
                        &item.render_node_id,
                        run.id(),
                        VersionedPayload::from_typed(render_checkpoint_schema(), &checkpoint)?,
                    )?;
                } else {
                    decode_render_checkpoint(
                        &graph,
                        item,
                        definition,
                        &behavior_checkpoint,
                        &context,
                    )?;
                }
            }
        }

        if graph.status() == ExecutionGraphStatus::Repairing && request.adjustment.is_some() {
            self.execute_behavior_adjustment(
                dependencies.model,
                dependencies.resources,
                dependencies.behavior_adapters,
                &mut graph,
                graphs,
                run.id(),
                &request,
                &context,
                &resolved,
                &blueprint,
                &ephemeral_input,
                cancellation,
            )
            .await?;
        }

        let output_node_id = blueprint.prepare.output_node_id();
        let finalize_was_pending = graph.status() == ExecutionGraphStatus::Running
            && node_status(&graph, output_node_id)? != ExecutionNodeStatus::Succeeded;
        if finalize_was_pending {
            start_local_node(&mut graph, graphs, output_node_id, run.id())?;
        }
        let assembled = match &blueprint.prepare {
            CompiledPreparePipeline::ItemGeneration { .. } => {
                restore_rendered_assembly(&context, &resolved, &blueprint, &graph, runs)
            }
            CompiledPreparePipeline::DataJson { .. } => render_data_json(&resolved),
        };
        let (provenance, generated_writes, generated_artifact_files, finalize_checkpoint) =
            match assembled {
                Ok(value) => value,
                Err(error) => {
                    if finalize_was_pending {
                        pause_failed_node(&mut graph, graphs, output_node_id, run.id(), &error)?;
                    } else if graph.status() == ExecutionGraphStatus::CommitPrepared
                        && graph.active_run_id() == Some(run.id())
                    {
                        release_commit_claim(&mut graph, graphs)?;
                    }
                    return Err(error);
                }
            };
        if finalize_was_pending {
            complete_node(
                &mut graph,
                graphs,
                output_node_id,
                run.id(),
                VersionedPayload::from_typed(finalize_checkpoint_schema(), &finalize_checkpoint)?,
            )?;
        }
        let mut persisted_finalize: StagedFinalizeCheckpoint =
            decode_checkpoint(&graph, output_node_id, &finalize_checkpoint_schema())?;
        if persisted_finalize != finalize_checkpoint {
            let checkpoint =
                VersionedPayload::from_typed(finalize_checkpoint_schema(), &finalize_checkpoint)?;
            if graph.status() == ExecutionGraphStatus::Repairing {
                // A resumed campaign may contain completed targets newer than finalize.
                mutate_graph(&mut graph, graphs, |graph| {
                    graph.replace_checkpoint(output_node_id, run.id(), checkpoint, Utc::now())
                })?;
            } else if graph.status() == ExecutionGraphStatus::Running
                && graph
                    .nodes()
                    .values()
                    .any(|node| node.feedback_state.is_some())
            {
                mutate_graph(&mut graph, graphs, |graph| {
                    graph.replace_checkpoint_while_running(
                        output_node_id,
                        run.id(),
                        checkpoint,
                        Utc::now(),
                    )
                })?;
            } else {
                return Err(CompositionGenerateError::InvalidCheckpoint);
            }
            persisted_finalize = finalize_checkpoint.clone();
        }
        debug_assert_eq!(persisted_finalize, finalize_checkpoint);
        let published = self
            .publish_staged(
                dependencies,
                runs,
                &mut graph,
                graphs,
                run,
                &request,
                &context,
                &resolved,
                &blueprint.pipeline,
                provenance,
                generated_writes,
                generated_artifact_files,
                finalize_checkpoint,
                cancellation,
            )
            .await;
        if published.is_err()
            && graph.status() == ExecutionGraphStatus::CommitPrepared
            && graph.active_run_id() == Some(run.id())
        {
            release_commit_claim(&mut graph, graphs)?;
        }
        let result = published?;
        Ok(StagedCompositionGenerateExecution { result })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    async fn execute_behavior_adjustment<C, R, G>(
        &self,
        model: &C,
        resources: &R,
        behavior_adapters: &ats_game_context::BehaviorAdapterRegistry,
        graph: &mut ExecutionGraphRecord,
        graphs: &G,
        run_id: &RunId,
        request: &CompositionGenerateRequest,
        context: &CompositionGenerateContext<'_>,
        resolved: &ResolvedItemGraph,
        blueprint: &StagedCompositionGenerateBlueprint,
        ephemeral_input: &EphemeralCompositionInput,
        cancellation: &CancellationToken,
    ) -> Result<(), CompositionGenerateError>
    where
        C: ModelClient + ?Sized,
        R: ResourceRepository + ?Sized,
        G: ExecutionGraphRepository + ?Sized,
    {
        let adjustment = request
            .adjustment
            .as_ref()
            .ok_or(CompositionGenerateError::AdjustmentInvalid)?;
        ephemeral_input.validate_for(adjustment)?;
        let instruction = match ephemeral_input {
            EphemeralCompositionInput::HumanSemanticFeedback { instruction, .. } => {
                instruction.trim()
            }
            EphemeralCompositionInput::None => {
                return Err(CompositionGenerateError::FeedbackInputUnavailable);
            }
        };
        let campaign = graph
            .repair_campaign()
            .cloned()
            .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
        let target = campaign
            .targets
            .first()
            .filter(|_| campaign.targets.len() == 1)
            .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
        let persisted_adjustment: HumanSemanticFeedbackRef = target
            .feedback
            .payload
            .decode(&item_adjustment_schema())
            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
        let item_index = blueprint
            .items
            .iter()
            .position(|item| {
                item.item_id == target.item_id && item.behavior_node_id == target.node_id
            })
            .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
        let item = &blueprint.items[item_index];
        let definition = &resolved.nodes[item_index];
        if &persisted_adjustment != adjustment
            || adjustment.item_id != definition.definition.item_id
            || adjustment.expected_definition_hash != definition.definition_hash
            || !matches!(
                &campaign.cause,
                ats_runtime::RepairCause::HumanSemanticFeedback {
                    item_id,
                    expected_definition_hash,
                    instruction_sha256,
                    ..
                } if item_id == &adjustment.item_id
                    && expected_definition_hash == &adjustment.expected_definition_hash
                    && instruction_sha256 == &adjustment.instruction_sha256
            )
        {
            return Err(CompositionGenerateError::InvalidCheckpoint);
        }

        if target.status != ats_runtime::ExecutionRepairTargetStatus::Completed {
            let plan = decode_plan_checkpoint(graph, item, definition)?;
            let behavior_service = BehaviorGenerationService::built_in()?;
            if target.status == ats_runtime::ExecutionRepairTargetStatus::Pending {
                mutate_graph(graph, graphs, |graph| {
                    graph.activate_repair_target(run_id, Utc::now())
                })?;
            }
            let generated = loop {
                let feedback = graph.nodes()[&item.behavior_node_id]
                    .feedback_state
                    .as_ref()
                    .filter(|state| {
                        state.phase == ats_runtime::ExecutionFeedbackPhase::OutputContract
                            && state.checkpoint_hash.as_ref() == Some(&target.checkpoint_hash)
                    })
                    .map(|state| {
                        state
                            .feedback
                            .payload
                            .decode::<super::behavior::BehaviorFeedback>(
                                &behavior_feedback_schema(),
                            )
                    })
                    .transpose()
                    .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
                let behavior_context = BehaviorGenerationContext {
                    pack: context.pack,
                    truth_snapshot_id: context.truth.manifest().snapshot_id(),
                    definition,
                    plan: &plan.plan,
                    project_context: context.project_context,
                    custom_instructions: None,
                    model: context.model.clone(),
                    model_request_limits: blueprint.model_request_limits,
                    feedback: feedback.as_ref(),
                    human_semantic_feedback: Some(instruction),
                };
                let snapshot = behavior_service.prepare(&behavior_context)?;
                match behavior_service
                    .generate(model, behavior_context, snapshot, cancellation)
                    .await
                {
                    Ok(generated) => break generated,
                    Err(error) => {
                        if cancellation.is_cancelled() {
                            return handle_cancellation(graph, graphs, run_id, cancellation);
                        }
                        let Some(evidence) = error.feedback_evidence() else {
                            let error = CompositionGenerateError::Behavior(error);
                            pause_repair_after_error(graph, graphs, run_id, &error)?;
                            return Err(error);
                        };
                        let completed_rounds = graph.nodes()[&item.behavior_node_id]
                            .feedback_state
                            .as_ref()
                            .map_or(0, |state| state.round);
                        if !request.repair_policy.permits(completed_rounds) {
                            pause_repair_with_code(
                                graph,
                                graphs,
                                run_id,
                                "model.feedback_exhausted",
                            )?;
                            let error = CompositionGenerateError::Behavior(error);
                            return Err(error);
                        }
                        let fingerprint = hash_json(&evidence.feedback)
                            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
                        if graph.nodes()[&item.behavior_node_id]
                            .feedback_state
                            .as_ref()
                            .is_some_and(|state| {
                                state.diagnostic_fingerprint == fingerprint
                                    && state.candidate_sha256.as_ref()
                                        == Some(&evidence.candidate_sha256)
                                    && state.checkpoint_hash.as_ref()
                                        == Some(&target.checkpoint_hash)
                            })
                        {
                            pause_repair_with_code(
                                graph,
                                graphs,
                                run_id,
                                "model.feedback_no_progress",
                            )?;
                            let error = CompositionGenerateError::Behavior(error);
                            return Err(error);
                        }
                        mutate_graph(graph, graphs, |graph| {
                            graph.record_repair_output_feedback(
                                &item.behavior_node_id,
                                run_id,
                                target.checkpoint_hash.clone(),
                                ats_runtime::ExecutionOutputFeedback {
                                    diagnostic_fingerprint: fingerprint,
                                    candidate_sha256: evidence.candidate_sha256,
                                    feedback: VersionedPayload::from_typed(
                                        behavior_feedback_schema(),
                                        &evidence.feedback,
                                    )
                                    .map_err(|_| {
                                        ats_runtime::ExecutionGraphError::InvalidMetadata
                                    })?,
                                },
                                Utc::now(),
                            )
                        })?;
                    }
                }
            };
            let behavior_sha256 = generated
                .proposal
                .sha256()
                .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
            let current_behavior = decode_behavior_checkpoint(graph, item, definition, context)?;
            if current_behavior.behavior_sha256 == behavior_sha256 {
                let error = CompositionGenerateError::AdjustmentInvalid;
                pause_repair_after_error(graph, graphs, run_id, &error)?;
                return Err(error);
            }
            let checkpoint = StagedBehaviorCheckpoint {
                pack_id: context.pack.id().clone(),
                pack_sha256: context.pack.content_sha256().clone(),
                truth_snapshot_id: context.truth.manifest().snapshot_id().clone(),
                catalog: context.pack.capability_catalog_identity().clone(),
                adapter: context.pack.behavior_adapter().clone(),
                definition_hash: definition.definition_hash.clone(),
                request_commitment: generated.request_snapshot.commitment().clone(),
                response_model: generated.response_model,
                usage: generated.usage,
                behavior_sha256,
                proposal: generated.proposal,
            };
            let request_sha256 = checkpoint.request_commitment.request_sha256().clone();
            let payload = VersionedPayload::from_typed(behavior_checkpoint_schema(), &checkpoint)?;
            mutate_graph(graph, graphs, |graph| {
                graph.complete_repair_target_with_request(
                    &item.behavior_node_id,
                    run_id,
                    request_sha256,
                    payload,
                    Utc::now(),
                )
            })?;
        }

        let behavior = decode_behavior_checkpoint(graph, item, definition, context)?;
        let (bundle, _) = match render_item(
            behavior_adapters,
            context.pack,
            context.resource_contributions,
            resources,
            RenderItemRequest {
                graph: resolved,
                definition,
                mod_id: &request.mod_id,
                proposal: &behavior.proposal,
            },
        ) {
            Ok(rendered) => rendered,
            Err(error) => {
                let error = CompositionGenerateError::Render(error);
                pause_repair_after_error(graph, graphs, run_id, &error)?;
                return Err(error);
            }
        };
        let render_checkpoint = StagedRenderCheckpoint {
            pack_id: context.pack.id().clone(),
            pack_sha256: context.pack.content_sha256().clone(),
            truth_snapshot_id: context.truth.manifest().snapshot_id().clone(),
            catalog: context.pack.capability_catalog_identity().clone(),
            adapter: context.pack.behavior_adapter().clone(),
            behavior_sha256: behavior.behavior_sha256.clone(),
            rendered_bundle_sha256: hash_json(&bundle)
                .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?,
            bundle,
        };
        mutate_graph(graph, graphs, |graph| {
            graph.replace_checkpoint_with_request(
                &item.render_node_id,
                run_id,
                behavior.behavior_sha256,
                VersionedPayload::from_typed(render_checkpoint_schema(), &render_checkpoint)
                    .map_err(|_| ats_runtime::ExecutionGraphError::InvalidMetadata)?,
                Utc::now(),
            )
        })?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    async fn publish_staged<C, I, R, W, S, V, A, B, P, G, RR>(
        &self,
        dependencies: CompositionGenerateDependencies<'_, C, I, R, W, S, V, A, B, P>,
        runs: &RR,
        graph: &mut ExecutionGraphRecord,
        graphs: &G,
        run: &mut RunRecord,
        request: &CompositionGenerateRequest,
        context: &CompositionGenerateContext<'_>,
        resolved: &ResolvedItemGraph,
        pipeline: &ResolvedPipelineGraph,
        provenance: Vec<CompositionBehaviorProvenance>,
        generated_writes: Vec<ProjectFileWrite>,
        generated_artifact_files: Vec<ProposedArtifactFile>,
        finalize: StagedFinalizeCheckpoint,
        cancellation: &CancellationToken,
    ) -> Result<CompositionGenerateResult, CompositionGenerateError>
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
        G: ExecutionGraphRepository + ?Sized,
        RR: RunRepository + ?Sized,
    {
        let (_, _, delivery, _) = compile_generation_nodes(pipeline, resolved)?;
        if graph.status() == ExecutionGraphStatus::Repairing
            && graph.repair_campaign().is_some_and(|campaign| {
                usize::try_from(campaign.current_target)
                    .is_ok_and(|current| current < campaign.targets.len())
            })
        {
            return Err(CompositionGenerateError::InvalidCheckpoint);
        }
        if graph.status() == ExecutionGraphStatus::Running
            || graph.status() == ExecutionGraphStatus::Repairing
        {
            mutate_graph(graph, graphs, |graph| {
                graph.begin_validation(run.id(), Utc::now())
            })?;
        }
        let stage = dependencies.stager.stage(ProjectStageRequest {
            project_root: context.project_root.to_path_buf(),
            run_id: run.id().clone(),
        })?;
        let stage_root = stage.root().to_path_buf();
        let staged_writes =
            dependencies
                .writer
                .apply(&stage_root, run.id(), generated_writes.clone())?;
        staged_writes.commit()?;
        if let Some((_, validation_primitive)) = &delivery.validation {
            if finalize.validation_primitive.as_ref() != Some(validation_primitive) {
                stage.cleanup()?;
                return Err(CompositionGenerateError::ValidationPrimitiveMismatch);
            }
            match dependencies
                .validator
                .validate(
                    ValidationRequest {
                        primitive: validation_primitive.clone(),
                        project_root: stage_root.clone(),
                        run_id: run.id().clone(),
                    },
                    cancellation,
                )
                .await
            {
                Ok(_) => {}
                Err(ValidationError::Rejected(report)) => {
                    stage.cleanup()?;
                    pause_validation_graph(graph, graphs, run.id(), "game.adapter_invalid")?;
                    return Err(ValidationError::Rejected(report).into());
                }
                Err(error) => {
                    stage.cleanup()?;
                    return Err(error.into());
                }
            }
        } else if finalize.validation_primitive.is_some() {
            stage.cleanup()?;
            return Err(CompositionGenerateError::ValidationPrimitiveMismatch);
        }
        check_cancelled(cancellation)?;
        let publication = StagedPublicationIntent {
            execution_graph_id: graph.id().clone(),
            artifact_id: request.artifact_id.clone(),
            graph_digest: resolved.graph_digest.clone(),
            generated_file_count: finalize.generated_file_count,
        };
        let canonical = VersionedPayload::from_typed(commit_intent_schema(), &publication)?;
        let payload_sha256 = hash_json(canonical.payload())
            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
        let digest = validated_content_digest(graph, &finalize)?;
        let intent = ExecutionCommitIntent::publication(
            request.artifact_id.clone(),
            canonical,
            payload_sha256,
            digest,
        );
        mutate_graph(graph, graphs, |graph| {
            graph.prepare_commit(run.id(), intent, Utc::now())
        })?;
        validate_publication_intent(graph, request, resolved, &finalize)?;

        if delivery.package_node_id.is_some() != request.package.is_some() {
            stage.cleanup()?;
            release_commit_claim(graph, graphs)?;
            return Err(CompositionGenerateError::InvalidInput);
        }
        let (build_run_id, build) = if delivery.build_node_id.is_some() {
            let build_request = ProjectBuildRequest {
                output_relative_root: request
                    .package
                    .as_ref()
                    .map(|package| package.source_relative_root.clone()),
            };
            let mut build_run = running_run::<ProjectBuildFeature, _>(&build_request)?;
            let build = match self
                .build
                .execute(
                    dependencies.build_runner,
                    &mut build_run,
                    build_request,
                    ProjectBuildContext {
                        pack: context.pack,
                        contributions: context
                            .build_contributions
                            .ok_or(CompositionGenerateError::InvalidContribution)?,
                        project_root: &stage_root,
                    },
                    cancellation,
                )
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    finish_failed_child(&mut build_run, error.run_failure(), cancellation)?;
                    persist_child(runs, &build_run)?;
                    stage.cleanup()?;
                    release_commit_claim(graph, graphs)?;
                    return Err(error.into());
                }
            };
            if let Err(error) = persist_child(runs, &build_run) {
                stage.cleanup()?;
                release_commit_claim(graph, graphs)?;
                return Err(error);
            }
            (Some(build_run.id().clone()), Some(build))
        } else {
            (None, None)
        };

        let (package_run_id, package, mut prepared_package) =
            if let (Some(_), Some(package_request)) =
                (&delivery.package_node_id, request.package.as_ref())
            {
                let mut package_run = running_run::<ProjectPackageFeature, _>(package_request)?;
                let prepared = match self.package.prepare(
                    dependencies.package_writer,
                    &package_run,
                    package_request,
                    ProjectPackageContext {
                        pack: context.pack,
                        contributions: context
                            .package_contributions
                            .ok_or(CompositionGenerateError::InvalidContribution)?,
                        project_root: &stage_root,
                    },
                    cancellation,
                ) {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        finish_failed_child(&mut package_run, error.run_failure(), cancellation)?;
                        if let Err(storage_error) = persist_child(runs, &package_run) {
                            stage.cleanup()?;
                            release_commit_claim(graph, graphs)?;
                            return Err(storage_error);
                        }
                        stage.cleanup()?;
                        release_commit_claim(graph, graphs)?;
                        return Err(error.into());
                    }
                };
                let result = prepared.result().clone();
                succeed_child::<ProjectPackageFeature, _>(&mut package_run, &result)?;
                if let Err(error) = persist_child(runs, &package_run) {
                    prepared.rollback()?;
                    stage.cleanup()?;
                    release_commit_claim(graph, graphs)?;
                    return Err(error);
                }
                (Some(package_run.id().clone()), Some(result), Some(prepared))
            } else {
                (None, None, None)
            };

        let mut final_writes = generated_writes;
        if let (Some(package), Some(prepared)) = (&package, &prepared_package) {
            final_writes.push(ProjectFileWrite::from_source(
                package.output_relative_path.clone(),
                prepared.output_path().to_path_buf(),
            )?);
        }
        ats_runtime::validate_project_writes(&final_writes)?;
        let pending = match dependencies
            .writer
            .apply(context.project_root, run.id(), final_writes)
        {
            Ok(pending) => pending,
            Err(error) => {
                if let Some(prepared) = prepared_package.take() {
                    prepared.rollback()?;
                }
                stage.cleanup()?;
                release_commit_claim(graph, graphs)?;
                return Err(error.into());
            }
        };
        if let Some(prepared) = prepared_package.take()
            && let Err(error) = prepared.commit()
        {
            pending.rollback()?;
            stage.cleanup()?;
            release_commit_claim(graph, graphs)?;
            return Err(error.into());
        }
        if let Err(error) = stage.cleanup() {
            pending.rollback()?;
            release_commit_claim(graph, graphs)?;
            return Err(error.into());
        }

        let mut child_run_ids = Vec::with_capacity(finalize.items.len() + 2);
        for item in &finalize.items {
            child_run_ids.push(item.plan_run_id.clone());
        }
        child_run_ids.extend(build_run_id.iter().cloned());
        child_run_ids.extend(package_run_id.iter().cloned());
        let extension = CompositionGenerateArtifactExtension {
            graph_digest: resolved.graph_digest.clone(),
            root_item_id: resolved.root_item_id.clone(),
            root_definition_hash: resolved.root_definition_hash.clone(),
            draft: resolved.draft.clone(),
            composition_profile: resolved.composition_profile.clone(),
            node_count: u32::try_from(resolved.nodes.len())
                .map_err(|_| CompositionGenerateError::InvalidInput)?,
            generated_file_count: finalize.generated_file_count,
            child_run_ids,
            package_output_relative_path: package
                .as_ref()
                .map(|result| result.output_relative_path.clone()),
            package_report: package.as_ref().map(|result| result.report.clone()),
            execution_graph_id: Some(graph.id().clone()),
        };
        let artifact_request = composition_artifact_request(
            request,
            context,
            run,
            resolved,
            &provenance,
            &generated_artifact_files,
            &extension,
        )?;
        let published = match dependencies.artifacts.publish(artifact_request) {
            Ok(published) => published,
            Err(_) => {
                pending.rollback()?;
                release_commit_claim(graph, graphs)?;
                return Err(CompositionGenerateError::ArtifactPublication);
            }
        };
        if let Err(error) = check_cancelled(cancellation) {
            cleanup_artifact(dependencies.artifacts, &request.artifact_id, run.id())?;
            pending.rollback()?;
            release_commit_claim(graph, graphs)?;
            return Err(error);
        }
        let result = CompositionGenerateResult {
            artifact_manifest_ref: published.artifact_manifest_ref,
            manifest_sha256: published.manifest_sha256,
            graph_digest: resolved.graph_digest.clone(),
            node_count: extension.node_count,
            generated_file_count: extension.generated_file_count,
            items: finalize.items,
            build_run_id,
            build,
            package_run_id,
            package,
            execution_graph_id: Some(graph.id().clone()),
        };
        let result_payload =
            VersionedPayload::from_typed(CompositionGenerateFeature::result_schema(), &result)?;
        if let Err(error) = pending.commit() {
            cleanup_artifact(dependencies.artifacts, &request.artifact_id, run.id())?;
            return Err(error.into());
        }
        mutate_graph(graph, graphs, |graph| {
            graph.mark_succeeded(run.id(), result_payload, Utc::now())
        })?;
        succeed_parent(run, &result)?;
        Ok(result)
    }
}

impl StagedCompositionGenerateBlueprint {
    fn validate_identity(
        &self,
        pipelines: &ats_game_context::GamePipelineRegistry,
        context: &CompositionGenerateContext<'_>,
    ) -> Result<(), CompositionGenerateError> {
        let contribution: CompositionGenerateContribution = context
            .composition_contributions
            .decode(&generation_slot())?;
        pipelines.validate_resolved(&contribution.pipeline, &self.pipeline)?;
        if self.schema_version != 8
            || self.game_pack_id != *context.pack.id()
            || self.game_pack_sha256 != *context.pack.content_sha256()
            || self.truth_snapshot_id != *context.truth.manifest().snapshot_id()
            || self.pipeline.game_pack_id != self.game_pack_id
            || self.pipeline.game_pack_sha256 != self.game_pack_sha256
            || self.pipeline.truth_snapshot_id != self.truth_snapshot_id
            || self.pipeline.source_graph_digest != self.graph_digest
            || self.pipeline.owner_feature_id != CompositionGenerateFeature::id()
            || self.request.execution.is_some()
            || self.items.len() > 128
            || match self.prepare {
                CompiledPreparePipeline::ItemGeneration { .. } => self.items.is_empty(),
                CompiledPreparePipeline::DataJson { .. } => !self.items.is_empty(),
            }
        {
            Err(CompositionGenerateError::InvalidCheckpoint)
        } else {
            Ok(())
        }
    }

    fn validate(
        &self,
        pipelines: &ats_game_context::GamePipelineRegistry,
        context: &CompositionGenerateContext<'_>,
        resolved: &ResolvedItemGraph,
        request: &CompositionGenerateRequest,
    ) -> Result<(), CompositionGenerateError> {
        self.validate_identity(pipelines, context)?;
        let contribution: CompositionGenerateContribution = context
            .composition_contributions
            .decode(&generation_slot())?;
        let expected_pipeline = pipelines.resolve(
            &contribution.pipeline,
            &pipeline_request(resolved, &contribution.pipeline.profile_id),
        )?;
        let (expected_prepare, expected_items, delivery, _) =
            compile_generation_nodes(&expected_pipeline, resolved)?;
        let mut canonical_request = request.clone();
        canonical_request.execution = None;
        canonical_request.adjustment = None;
        if canonical_request != self.request
            || self.graph_digest != resolved.graph_digest
            || self.pipeline != expected_pipeline
            || self.prepare != expected_prepare
            || self.items != expected_items
            || delivery.package_node_id.is_some() != request.package.is_some()
        {
            return Err(CompositionGenerateError::InvalidCheckpoint);
        }
        Ok(())
    }
}

fn pipeline_request(
    resolved: &ResolvedItemGraph,
    profile_id: &ats_kernel::PipelineProfileId,
) -> PipelineResolveRequest {
    let work_items = resolved
        .nodes
        .iter()
        .map(|definition| {
            let mut depends_on = resolved
                .pinned_edges
                .iter()
                .filter(|edge| edge.source_item_id == definition.definition.item_id)
                .map(|edge| edge.target_item_id.clone())
                .collect::<Vec<_>>();
            depends_on.sort();
            depends_on.dedup();
            PipelineWorkItem {
                item_id: definition.definition.item_id.clone(),
                definition_hash: definition.definition_hash.clone(),
                depends_on,
            }
        })
        .collect();
    PipelineResolveRequest {
        owner_feature_id: CompositionGenerateFeature::id(),
        game_pack_id: resolved.game_pack_id.clone(),
        game_pack_sha256: resolved.game_pack_sha256.clone(),
        truth_snapshot_id: resolved.truth_snapshot_id.clone(),
        source_graph_digest: resolved.graph_digest.clone(),
        profile_id: profile_id.clone(),
        work_items,
    }
}

fn compile_generation_nodes(
    pipeline: &ResolvedPipelineGraph,
    resolved: &ResolvedItemGraph,
) -> Result<
    (
        CompiledPreparePipeline,
        Vec<StagedCompositionGenerateItem>,
        CompiledDeliveryPipeline,
        Vec<ExecutionNodeSpec>,
    ),
    CompositionGenerateError,
> {
    let validation_nodes = pipeline
        .nodes
        .iter()
        .filter(|node| node.phase == PipelineNodePhase::Validate)
        .collect::<Vec<_>>();
    if validation_nodes.len() > 1
        || validation_nodes
            .iter()
            .any(|node| !matches!(node.scope, PipelineNodeScope::Composition))
    {
        return Err(CompositionGenerateError::InvalidContribution);
    }
    let validation = validation_nodes
        .first()
        .map(|node| (node.node_id.clone(), node.primitive_id.clone()));

    let unique_delivery = |primitive_id: &str| {
        let mut matches = pipeline.nodes.iter().filter(|node| {
            node.phase == PipelineNodePhase::Deliver
                && matches!(node.scope, PipelineNodeScope::Composition)
                && node.primitive_id.as_str() == primitive_id
        });
        let node = matches.next();
        if matches.next().is_some() {
            None
        } else {
            Some(node)
        }
    };
    let build = unique_delivery("feature.project-build")
        .ok_or(CompositionGenerateError::InvalidContribution)?;
    let package = unique_delivery("feature.project-package")
        .ok_or(CompositionGenerateError::InvalidContribution)?;
    if pipeline.nodes.iter().any(|node| {
        node.phase == PipelineNodePhase::Deliver
            && node.primitive_id.as_str() != "feature.project-build"
            && node.primitive_id.as_str() != "feature.project-package"
    }) {
        return Err(CompositionGenerateError::InvalidContribution);
    }
    let mut publish = pipeline.nodes.iter().filter(|node| {
        node.phase == PipelineNodePhase::Publish
            && matches!(node.scope, PipelineNodeScope::Composition)
            && node.primitive_id.as_str() == "storage.atomic-publish"
    });
    let publish = publish
        .next()
        .filter(|_| publish.next().is_none())
        .ok_or(CompositionGenerateError::InvalidContribution)?;
    let delivery = CompiledDeliveryPipeline {
        validation,
        build_node_id: build.map(|node| node.node_id.clone()),
        package_node_id: package.map(|node| node.node_id.clone()),
        publish_node_id: publish.node_id.clone(),
    };

    let prepare_nodes = pipeline
        .nodes
        .iter()
        .filter(|node| node.phase == PipelineNodePhase::Prepare)
        .collect::<Vec<_>>();
    let (prepare, staged_items) =
        compile_item_generation_prepare(&prepare_nodes, &validation_nodes, &delivery, resolved)
            .or_else(|| {
                compile_data_json_prepare(&prepare_nodes).map(|prepare| (prepare, Vec::new()))
            })
            .ok_or(CompositionGenerateError::InvalidContribution)?;
    let specs = pipeline
        .nodes
        .iter()
        .filter(|node| node.phase == PipelineNodePhase::Prepare)
        .map(|node| {
            Ok(ExecutionNodeSpec {
                node_id: node.node_id.clone(),
                role_id: execution_role_id(node.primitive_id.as_str()).into(),
                depends_on: node.depends_on.clone(),
                request_snapshot_hash: zero_digest()?,
            })
        })
        .collect::<Result<Vec<_>, CompositionGenerateError>>()?;
    Ok((prepare, staged_items, delivery, specs))
}

fn execution_role_id(primitive_id: &str) -> &str {
    match primitive_id {
        "feature.mod-plan" => "mod.plan",
        "feature.composition-behavior" => "composition.behavior",
        "game.behavior-render" => "composition.render",
        "feature.composition-finalize" => "composition.finalize",
        other => other,
    }
}

fn compile_item_generation_prepare(
    prepare_nodes: &[&ats_game_context::PipelineNode],
    validation_nodes: &[&ats_game_context::PipelineNode],
    delivery: &CompiledDeliveryPipeline,
    resolved: &ResolvedItemGraph,
) -> Option<(CompiledPreparePipeline, Vec<StagedCompositionGenerateItem>)> {
    let mut staged_items = Vec::with_capacity(resolved.nodes.len());
    let mut claimed = BTreeSet::new();
    for definition in &resolved.nodes {
        let item_id = &definition.definition.item_id;
        let find = |primitive_id: &str| {
            let mut matches = prepare_nodes.iter().copied().filter(|node| {
                matches!(
                    &node.scope,
                    PipelineNodeScope::Item { item_id: candidate } if candidate == item_id
                ) && node.primitive_id.as_str() == primitive_id
            });
            let node = matches.next()?;
            matches.next().is_none().then_some(node)
        };
        let plan = find("feature.mod-plan")?;
        let behavior = find("feature.composition-behavior")?;
        let render = find("game.behavior-render")?;
        if plan.checkpoint_policy != PipelineCheckpointPolicy::OnSuccess
            || behavior.checkpoint_policy != PipelineCheckpointPolicy::OnSuccess
            || render.checkpoint_policy != PipelineCheckpointPolicy::OnSuccess
            || plan.produces.schema != plan_checkpoint_schema()
            || behavior.produces.schema != behavior_checkpoint_schema()
            || render.produces.schema != render_checkpoint_schema()
            || !claimed.insert(plan.node_id.clone())
            || !claimed.insert(behavior.node_id.clone())
            || !claimed.insert(render.node_id.clone())
        {
            return None;
        }
        let valid_binding = match (&delivery.validation, validation_nodes.first()) {
            (Some((_, primitive)), Some(validation)) => {
                behavior.validation.is_empty()
                    && render.validation.len() == 1
                    && render.validation[0].id == *primitive
                    && render.validation[0].version == validation.primitive_version
            }
            (None, None) => behavior.validation.is_empty() && render.validation.is_empty(),
            _ => false,
        };
        if !valid_binding {
            return None;
        }
        staged_items.push(StagedCompositionGenerateItem {
            item_id: item_id.clone(),
            definition_hash: definition.definition_hash.clone(),
            plan_node_id: plan.node_id.clone(),
            behavior_node_id: behavior.node_id.clone(),
            render_node_id: render.node_id.clone(),
        });
    }

    let mut finalize = prepare_nodes.iter().copied().filter(|node| {
        matches!(node.scope, PipelineNodeScope::Composition)
            && node.primitive_id.as_str() == "feature.composition-finalize"
    });
    let finalize = finalize.next().filter(|_| finalize.next().is_none())?;
    if finalize.checkpoint_policy != PipelineCheckpointPolicy::OnSuccess
        || finalize.produces.schema != finalize_checkpoint_schema()
        || !claimed.insert(finalize.node_id.clone())
        || claimed.len() != prepare_nodes.len()
    {
        return None;
    }
    Some((
        CompiledPreparePipeline::ItemGeneration {
            output_node_id: finalize.node_id.clone(),
        },
        staged_items,
    ))
}

fn compile_data_json_prepare(
    prepare_nodes: &[&ats_game_context::PipelineNode],
) -> Option<CompiledPreparePipeline> {
    let [render] = prepare_nodes else {
        return None;
    };
    (matches!(render.scope, PipelineNodeScope::Composition)
        && render.primitive_id.as_str() == "data.render-json"
        && render.depends_on.is_empty()
        && render.validation.is_empty())
    .then_some(())
    .filter(|_| {
        render.checkpoint_policy == PipelineCheckpointPolicy::OnSuccess
            && render.produces.schema == finalize_checkpoint_schema()
    })
    .map(|()| CompiledPreparePipeline::DataJson {
        output_node_id: render.node_id.clone(),
    })
}

fn render_data_json(resolved: &ResolvedItemGraph) -> PreparedCompositionAssembly {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct DataJsonDocument<'a> {
        schema_version: u32,
        graph_digest: &'a Sha256Digest,
        items: &'a [StoredItemDefinition],
    }

    let relative_path = "Generated/items.json";
    let bytes = serde_json::to_vec_pretty(&DataJsonDocument {
        schema_version: 1,
        graph_digest: &resolved.graph_digest,
        items: &resolved.nodes,
    })
    .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    let writes = vec![ProjectFileWrite::new(relative_path, bytes)?];
    let artifact_files = vec![ProposedArtifactFile {
        role: "data.items".into(),
        relative_path: relative_path.into(),
        composition_merge: None,
        composition_merge_key_policy: None,
    }];
    Ok((
        Vec::new(),
        writes,
        artifact_files,
        StagedFinalizeCheckpoint {
            graph_digest: resolved.graph_digest.clone(),
            validation_primitive: None,
            generated_file_count: 1,
            items: Vec::new(),
        },
    ))
}

fn restore_rendered_assembly<R: RunRepository + ?Sized>(
    context: &CompositionGenerateContext<'_>,
    resolved: &ResolvedItemGraph,
    blueprint: &StagedCompositionGenerateBlueprint,
    graph: &ExecutionGraphRecord,
    runs: &R,
) -> PreparedCompositionAssembly {
    let (_, _, delivery, _) = compile_generation_nodes(&blueprint.pipeline, resolved)?;
    let validation_primitive = delivery
        .validation
        .map(|(_, primitive)| primitive)
        .ok_or(CompositionGenerateError::ValidationPrimitiveMismatch)?;
    let mut provenance = Vec::with_capacity(blueprint.items.len());
    let mut assembled_files = BTreeMap::new();
    let mut item_results = Vec::with_capacity(blueprint.items.len());

    for (item, definition) in blueprint.items.iter().zip(&resolved.nodes) {
        let plan = decode_plan_checkpoint(graph, item, definition)?;
        persist_child(runs, &plan.child_run)?;
        let behavior = decode_behavior_checkpoint(graph, item, definition, context)?;
        let render = decode_render_checkpoint(graph, item, definition, &behavior, context)?;
        for file in &render.bundle.files {
            merge_rendered_file(&mut assembled_files, file)?;
        }
        provenance.push(CompositionBehaviorProvenance {
            item_id: definition.definition.item_id.clone(),
            definition_hash: definition.definition_hash.clone(),
            pack_id: behavior.pack_id.clone(),
            pack_sha256: behavior.pack_sha256.clone(),
            truth_snapshot_id: behavior.truth_snapshot_id.clone(),
            catalog: behavior.catalog.clone(),
            adapter: behavior.adapter.clone(),
            model_request_sha256: behavior.request_commitment.request_sha256().clone(),
            behavior_sha256: behavior.behavior_sha256.clone(),
            rendered_bundle_sha256: render.rendered_bundle_sha256.clone(),
            model: behavior.response_model.clone(),
            usage: behavior.usage.clone(),
        });
        item_results.push(CompositionItemRunResult {
            item_id: definition.definition.item_id.clone(),
            definition_hash: definition.definition_hash.clone(),
            plan_run_id: plan.child_run.id().clone(),
            behavior_request_sha256: behavior.request_commitment.request_sha256().clone(),
            behavior_sha256: behavior.behavior_sha256,
            rendered_bundle_sha256: render.rendered_bundle_sha256,
            adapter: behavior.adapter,
            generated_file_count: u32::try_from(render.bundle.files.len())
                .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?,
        });
    }
    let mut writes = Vec::with_capacity(assembled_files.len());
    let mut artifact_files = Vec::with_capacity(assembled_files.len());
    for file in assembled_files.values() {
        writes.push(ProjectFileWrite::new(
            file.relative_path.clone(),
            file.bytes.clone(),
        )?);
        artifact_files.push(ProposedArtifactFile {
            role: file.role.clone(),
            relative_path: file.relative_path.clone(),
            composition_merge: None,
            composition_merge_key_policy: None,
        });
    }
    ats_runtime::validate_project_writes(&writes)?;
    let finalize = StagedFinalizeCheckpoint {
        graph_digest: resolved.graph_digest.clone(),
        validation_primitive: Some(validation_primitive),
        generated_file_count: u32::try_from(artifact_files.len())
            .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?,
        items: item_results,
    };
    Ok((provenance, writes, artifact_files, finalize))
}

struct AssembledRenderedFile {
    role: String,
    relative_path: String,
    bytes: Vec<u8>,
    composition_merge: Option<RenderedFileMerge>,
    composition_merge_key_policy: Option<RenderedFileMergeKeyPolicy>,
}

fn merge_rendered_file(
    files: &mut BTreeMap<String, AssembledRenderedFile>,
    file: &RenderedFile,
) -> Result<(), CompositionGenerateError> {
    let Some(existing) = files.get_mut(&file.relative_path) else {
        files.insert(
            file.relative_path.clone(),
            AssembledRenderedFile {
                role: file.role.clone(),
                relative_path: file.relative_path.clone(),
                bytes: file.bytes.clone(),
                composition_merge: file.composition_merge,
                composition_merge_key_policy: file.composition_merge_key_policy,
            },
        );
        return Ok(());
    };

    if existing.role != file.role
        || existing.composition_merge != Some(RenderedFileMerge::JsonObject)
        || file.composition_merge != Some(RenderedFileMerge::JsonObject)
        || existing.composition_merge_key_policy != file.composition_merge_key_policy
        || file.composition_merge_key_policy != Some(RenderedFileMergeKeyPolicy::UniqueKeys)
    {
        return Err(CompositionGenerateError::RenderedFileConflict);
    }

    let mut merged =
        serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&existing.bytes)
            .map_err(|_| CompositionGenerateError::RenderedFileConflict)?;
    let incoming =
        serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&file.bytes)
            .map_err(|_| CompositionGenerateError::RenderedFileConflict)?;
    if incoming.keys().any(|key| merged.contains_key(key)) {
        return Err(CompositionGenerateError::RenderedFileConflict);
    }
    merged.extend(incoming);
    existing.bytes =
        serde_json::to_vec(&merged).map_err(|_| CompositionGenerateError::RenderedFileConflict)?;
    Ok(())
}

fn decode_behavior_checkpoint(
    graph: &ExecutionGraphRecord,
    item: &StagedCompositionGenerateItem,
    definition: &StoredItemDefinition,
    context: &CompositionGenerateContext<'_>,
) -> Result<StagedBehaviorCheckpoint, CompositionGenerateError> {
    let checkpoint: StagedBehaviorCheckpoint =
        decode_checkpoint(graph, &item.behavior_node_id, &behavior_checkpoint_schema())?;
    let node = graph
        .nodes()
        .get(&item.behavior_node_id)
        .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
    checkpoint
        .request_commitment
        .verify()
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    checkpoint
        .proposal
        .validate(context.pack.capability_catalog())
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    if checkpoint.pack_id != *context.pack.id()
        || checkpoint.pack_sha256 != *context.pack.content_sha256()
        || checkpoint.truth_snapshot_id != *context.truth.manifest().snapshot_id()
        || checkpoint.catalog != *context.pack.capability_catalog_identity()
        || checkpoint.adapter != *context.pack.behavior_adapter()
        || checkpoint.definition_hash != definition.definition_hash
        || checkpoint.proposal.item_id != definition.definition.item_id
        || checkpoint.proposal.item_type != definition.definition.item_type
        || checkpoint.proposal.definition_hash != definition.definition_hash
        || checkpoint.proposal.catalog != checkpoint.catalog
        || checkpoint.proposal.adapter != checkpoint.adapter
        || checkpoint.proposal.sha256().ok().as_ref() != Some(&checkpoint.behavior_sha256)
        || checkpoint.request_commitment.identity.feature_id != CompositionGenerateFeature::id()
        || checkpoint.request_commitment.identity.game_pack.id != checkpoint.pack_id
        || checkpoint.request_commitment.identity.game_pack.sha256 != checkpoint.pack_sha256
        || checkpoint
            .request_commitment
            .identity
            .truth_snapshot_id
            .as_ref()
            != Some(&checkpoint.truth_snapshot_id)
        || checkpoint.request_commitment.request_sha256() != &node.request_snapshot_hash
        || checkpoint.response_model.trim().is_empty()
        || checkpoint.response_model.chars().count() > 256
        || checkpoint.response_model.contains('\0')
    {
        return Err(CompositionGenerateError::InvalidCheckpoint);
    }
    Ok(checkpoint)
}

fn decode_render_checkpoint(
    graph: &ExecutionGraphRecord,
    item: &StagedCompositionGenerateItem,
    definition: &StoredItemDefinition,
    behavior: &StagedBehaviorCheckpoint,
    context: &CompositionGenerateContext<'_>,
) -> Result<StagedRenderCheckpoint, CompositionGenerateError> {
    let checkpoint: StagedRenderCheckpoint =
        decode_checkpoint(graph, &item.render_node_id, &render_checkpoint_schema())?;
    let node = graph
        .nodes()
        .get(&item.render_node_id)
        .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
    checkpoint
        .bundle
        .validate_for(&behavior.proposal)
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    if checkpoint.pack_id != *context.pack.id()
        || checkpoint.pack_sha256 != *context.pack.content_sha256()
        || checkpoint.truth_snapshot_id != *context.truth.manifest().snapshot_id()
        || checkpoint.catalog != *context.pack.capability_catalog_identity()
        || checkpoint.adapter != *context.pack.behavior_adapter()
        || checkpoint.bundle.item_id != definition.definition.item_id
        || checkpoint.bundle.definition_hash != definition.definition_hash
        || checkpoint.behavior_sha256 != behavior.behavior_sha256
        || checkpoint.bundle.behavior_sha256 != behavior.behavior_sha256
        || checkpoint.bundle.adapter != behavior.adapter
        || hash_json(&checkpoint.bundle).ok().as_ref() != Some(&checkpoint.rendered_bundle_sha256)
        || node.request_snapshot_hash != behavior.behavior_sha256
    {
        return Err(CompositionGenerateError::InvalidCheckpoint);
    }
    Ok(checkpoint)
}

fn decode_plan_checkpoint(
    graph: &ExecutionGraphRecord,
    item: &StagedCompositionGenerateItem,
    definition: &StoredItemDefinition,
) -> Result<StagedPlanCheckpoint, CompositionGenerateError> {
    let checkpoint: StagedPlanCheckpoint =
        decode_checkpoint(graph, &item.plan_node_id, &plan_checkpoint_schema())?;
    let result = checkpoint
        .child_run
        .result()
        .ok_or(CompositionGenerateError::InvalidCheckpoint)?
        .decode::<crate::mod_plan::PlanItem>(&ModPlanFeature::result_schema())
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    if checkpoint.child_run.feature_id() != &ModPlanFeature::id()
        || checkpoint.child_run.status() != RunStatus::Succeeded
        || result != checkpoint.plan
        || checkpoint.plan.item_id != definition.definition.item_id.as_str()
        || checkpoint.plan.item_type != definition.definition.item_type.as_str()
    {
        return Err(CompositionGenerateError::InvalidCheckpoint);
    }
    Ok(checkpoint)
}

fn validated_content_digest(
    graph: &ExecutionGraphRecord,
    finalized: &StagedFinalizeCheckpoint,
) -> Result<Sha256Digest, CompositionGenerateError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DigestNode<'a> {
        node_id: &'a ExecutionNodeId,
        role_id: &'a str,
        depends_on: &'a [ExecutionNodeId],
        checkpoint_schema: &'a SchemaRef,
        checkpoint_sha256: &'a Sha256Digest,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DigestDomain<'a> {
        owner_feature_id: &'a FeatureId,
        request_snapshot_hash: &'a Sha256Digest,
        blueprint_schema: &'a SchemaRef,
        blueprint_sha256: &'a Sha256Digest,
        nodes: Vec<DigestNode<'a>>,
        finalized: &'a StagedFinalizeCheckpoint,
    }
    let nodes = graph
        .nodes()
        .values()
        .map(|node| {
            let checkpoint = node
                .active_checkpoint
                .as_ref()
                .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
            Ok(DigestNode {
                node_id: &node.node_id,
                role_id: &node.role_id,
                depends_on: &node.depends_on,
                checkpoint_schema: checkpoint.payload.schema(),
                checkpoint_sha256: &checkpoint.sha256,
            })
        })
        .collect::<Result<Vec<_>, CompositionGenerateError>>()?;
    hash_json(&DigestDomain {
        owner_feature_id: graph.owner_feature_id(),
        request_snapshot_hash: graph.request_snapshot_hash(),
        blueprint_schema: graph.blueprint().payload.schema(),
        blueprint_sha256: &graph.blueprint().sha256,
        nodes,
        finalized,
    })
    .map_err(|_| CompositionGenerateError::InvalidCheckpoint)
}

fn validate_publication_intent(
    graph: &ExecutionGraphRecord,
    request: &CompositionGenerateRequest,
    resolved: &ResolvedItemGraph,
    finalize: &StagedFinalizeCheckpoint,
) -> Result<(), CompositionGenerateError> {
    let intent = graph
        .commit_intent()
        .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
    let publication = intent
        .publication
        .as_ref()
        .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
    let decoded: StagedPublicationIntent = publication
        .canonical_payload
        .decode(&commit_intent_schema())
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    if publication.target_id != request.artifact_id
        || decoded.execution_graph_id != *graph.id()
        || decoded.artifact_id != request.artifact_id
        || decoded.graph_digest != resolved.graph_digest
        || decoded.generated_file_count != finalize.generated_file_count
        || intent.validated_content_digest != validated_content_digest(graph, finalize)?
    {
        return Err(CompositionGenerateError::InvalidCheckpoint);
    }
    Ok(())
}

fn pause_validation_graph<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    graphs: &G,
    run_id: &RunId,
    code: &str,
) -> Result<(), CompositionGenerateError> {
    let failure = ExecutionFailure::new(
        FailureCode::parse(code).map_err(|_| CompositionGenerateError::InvalidCheckpoint)?,
        "composition.generate.validate",
    )
    .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    mutate_graph(graph, graphs, |graph| {
        graph.pause_after_graph_failure(run_id, failure, Utc::now())
    })
}

fn pause_repair_after_error<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    graphs: &G,
    run_id: &RunId,
    error: &CompositionGenerateError,
) -> Result<(), CompositionGenerateError> {
    let failure = error.run_failure();
    let safe = ExecutionFailure::new(failure.code, failure.stage)
        .map_err(|_| CompositionGenerateError::ExecutionGraphStorage)?;
    mutate_graph(graph, graphs, |graph| {
        graph.pause_after_graph_failure(run_id, safe, Utc::now())
    })
}

fn pause_repair_with_code<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    graphs: &G,
    run_id: &RunId,
    code: &str,
) -> Result<(), CompositionGenerateError> {
    let failure = ExecutionFailure::new(
        FailureCode::parse(code).map_err(|_| CompositionGenerateError::InvalidCheckpoint)?,
        "composition.behavior.feedback",
    )
    .map_err(|_| CompositionGenerateError::ExecutionGraphStorage)?;
    mutate_graph(graph, graphs, |graph| {
        graph.pause_after_graph_failure(run_id, failure, Utc::now())
    })
}

fn execution_graph_id(
    request: &CompositionGenerateRequest,
) -> Result<&ExecutionGraphId, CompositionGenerateError> {
    request
        .execution
        .as_ref()
        .map(|execution| match execution {
            CompositionGenerateExecutionRequest::Start { execution_graph_id }
            | CompositionGenerateExecutionRequest::Resume {
                execution_graph_id, ..
            } => execution_graph_id,
        })
        .ok_or(CompositionGenerateError::InvalidInput)
}

fn request_hash<F, T>(request: &T) -> Result<Sha256Digest, CompositionGenerateError>
where
    F: FeatureSpec,
    T: Serialize,
{
    hash_json(&VersionedPayload::from_typed(F::request_schema(), request)?)
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)
}

fn zero_digest() -> Result<Sha256Digest, CompositionGenerateError> {
    Sha256Digest::parse("0".repeat(64)).map_err(|_| CompositionGenerateError::InvalidInput)
}

fn persist_child<R: RunRepository + ?Sized>(
    repository: &R,
    child: &RunRecord,
) -> Result<(), CompositionGenerateError> {
    match repository.create(child) {
        Ok(()) => Ok(()),
        Err(RunRepositoryError::AlreadyExists) => repository
            .get(child.id())
            .map_err(|_| CompositionGenerateError::RunStorage)
            .and_then(|existing| {
                if existing == *child {
                    Ok(())
                } else {
                    Err(CompositionGenerateError::RunStorage)
                }
            }),
        Err(_) => Err(CompositionGenerateError::RunStorage),
    }
}

fn persist_completed_child<R, G>(
    runs: &R,
    child: &RunRecord,
    graph: &mut ExecutionGraphRecord,
    graphs: &G,
    parent_run_id: &RunId,
) -> Result<(), CompositionGenerateError>
where
    R: RunRepository + ?Sized,
    G: ExecutionGraphRepository + ?Sized,
{
    if let Err(error) = persist_child(runs, child) {
        let ready = graph
            .nodes()
            .values()
            .find(|node| {
                node.status == ExecutionNodeStatus::Pending
                    && node.depends_on.iter().all(|dependency| {
                        graph.nodes().get(dependency).is_some_and(|dependency| {
                            dependency.status == ExecutionNodeStatus::Succeeded
                        })
                    })
            })
            .map(|node| node.node_id.clone())
            .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
        let failure = error.run_failure();
        let safe = ExecutionFailure::new(failure.code, failure.stage)
            .map_err(|_| CompositionGenerateError::ExecutionGraphStorage)?;
        mutate_graph(graph, graphs, |graph| {
            graph.pause_pending_node(&ready, parent_run_id, safe, Utc::now())
        })?;
        return Err(error);
    }
    Ok(())
}

fn bind_and_start_node<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    node_id: &ExecutionNodeId,
    run_id: &RunId,
    request_snapshot_hash: Sha256Digest,
) -> Result<(), CompositionGenerateError> {
    mutate_graph(graph, repository, |graph| {
        graph.set_node_request_snapshot_hash(node_id, run_id, request_snapshot_hash, Utc::now())
    })?;
    start_local_node(graph, repository, node_id, run_id)
}

fn start_local_node<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    node_id: &ExecutionNodeId,
    run_id: &RunId,
) -> Result<(), CompositionGenerateError> {
    mutate_graph(graph, repository, |graph| {
        graph.start_node(node_id, run_id, Utc::now())
    })
}

fn complete_node<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    node_id: &ExecutionNodeId,
    run_id: &RunId,
    checkpoint: VersionedPayload,
) -> Result<(), CompositionGenerateError> {
    mutate_graph(graph, repository, |graph| {
        graph.complete_node(node_id, run_id, checkpoint, Utc::now())
    })
}

fn pause_failed_node<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    node_id: &ExecutionNodeId,
    run_id: &RunId,
    error: &CompositionGenerateError,
) -> Result<(), CompositionGenerateError> {
    let failure = error.run_failure();
    let safe = ExecutionFailure::new(failure.code, failure.stage)
        .map_err(|_| CompositionGenerateError::ExecutionGraphStorage)?;
    mutate_graph(graph, repository, |graph| {
        graph.pause_after_node_failure(node_id, run_id, safe, Utc::now())
    })
}

fn pause_running_node_with_code<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    node_id: &ExecutionNodeId,
    run_id: &RunId,
    code: &str,
) -> Result<(), CompositionGenerateError> {
    let failure = ExecutionFailure::new(
        FailureCode::parse(code).map_err(|_| CompositionGenerateError::InvalidCheckpoint)?,
        "composition.generate.feedback",
    )
    .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    mutate_graph(graph, repository, |graph| {
        graph.pause_after_node_failure(node_id, run_id, failure, Utc::now())
    })
}

fn handle_cancellation<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    run_id: &RunId,
    cancellation: &CancellationToken,
) -> Result<(), CompositionGenerateError> {
    let Some(reason) = cancellation.reason() else {
        return Ok(());
    };
    mutate_graph(graph, repository, |graph| {
        if reason == ats_runtime::CancellationReason::User {
            graph.cancel(Some(run_id), Utc::now())
        } else {
            graph.pause_interrupted(run_id, Utc::now())
        }
    })?;
    Err(CompositionGenerateError::Cancelled)
}

fn release_commit_claim<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
) -> Result<(), CompositionGenerateError> {
    mutate_graph(graph, repository, |graph| {
        graph.recover_stale_claim(Utc::now())
    })
}

fn mutate_graph<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    mutate: impl FnOnce(&mut ExecutionGraphRecord) -> Result<(), ats_runtime::ExecutionGraphError>,
) -> Result<(), CompositionGenerateError> {
    let expected_revision = graph.revision();
    mutate(graph).map_err(|_| CompositionGenerateError::ExecutionGraphConflict)?;
    repository
        .compare_and_set(expected_revision, graph)
        .map_err(map_graph_repository_error)
}

fn node_status(
    graph: &ExecutionGraphRecord,
    node_id: &ExecutionNodeId,
) -> Result<ExecutionNodeStatus, CompositionGenerateError> {
    graph
        .nodes()
        .get(node_id)
        .map(|node| node.status)
        .ok_or(CompositionGenerateError::InvalidCheckpoint)
}

fn decode_checkpoint<T: for<'de> Deserialize<'de>>(
    graph: &ExecutionGraphRecord,
    node_id: &ExecutionNodeId,
    expected_schema: &SchemaRef,
) -> Result<T, CompositionGenerateError> {
    graph
        .nodes()
        .get(node_id)
        .and_then(|node| node.active_checkpoint.as_ref())
        .ok_or(CompositionGenerateError::InvalidCheckpoint)?
        .payload
        .decode(expected_schema)
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)
}

fn map_graph_repository_error(error: ExecutionGraphRepositoryError) -> CompositionGenerateError {
    match error {
        ExecutionGraphRepositoryError::AlreadyExists | ExecutionGraphRepositoryError::Conflict => {
            CompositionGenerateError::ExecutionGraphConflict
        }
        ExecutionGraphRepositoryError::NotFound
        | ExecutionGraphRepositoryError::InvalidRecord
        | ExecutionGraphRepositoryError::Io(_)
        | ExecutionGraphRepositoryError::Json(_) => CompositionGenerateError::ExecutionGraphStorage,
    }
}

fn succeed_parent(
    run: &mut RunRecord,
    result: &CompositionGenerateResult,
) -> Result<(), CompositionGenerateError> {
    let payload =
        VersionedPayload::from_typed(CompositionGenerateFeature::result_schema(), result)?;
    run.apply_transition(RunTransition::Succeed { result: payload }, Utc::now())?;
    Ok(())
}

fn blueprint_schema() -> SchemaRef {
    schema_version(BLUEPRINT_SCHEMA_ID, 9)
}

fn item_adjustment_schema() -> SchemaRef {
    schema("feature.item-adjustment")
}

fn plan_checkpoint_schema() -> SchemaRef {
    schema(PLAN_CHECKPOINT_SCHEMA_ID)
}

fn behavior_checkpoint_schema() -> SchemaRef {
    schema_version(BEHAVIOR_CHECKPOINT_SCHEMA_ID, 2)
}

fn behavior_feedback_schema() -> SchemaRef {
    schema("feature.composition-behavior-feedback")
}

fn render_checkpoint_schema() -> SchemaRef {
    schema(RENDER_CHECKPOINT_SCHEMA_ID)
}

fn finalize_checkpoint_schema() -> SchemaRef {
    schema_version(FINALIZE_CHECKPOINT_SCHEMA_ID, 3)
}

fn commit_intent_schema() -> SchemaRef {
    schema(COMMIT_INTENT_SCHEMA_ID)
}

fn schema_version(id: &str, version: u32) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in staged schema ID is valid"),
        version: SchemaVersion::new(version).expect("built-in staged schema version is valid"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use ats_kernel::{ExecutionGraphId, FeatureId};
    use ats_runtime::{ExecutionGraphRecovery, ExecutionNodeSpec, RunRepositoryError, RunSummary};

    use super::*;

    struct RejectingRunRepository;

    impl RunRepository for RejectingRunRepository {
        fn create(&self, _: &RunRecord) -> Result<(), RunRepositoryError> {
            Err(RunRepositoryError::Io(std::io::Error::other(
                "fixture rejection",
            )))
        }

        fn get(&self, _: &RunId) -> Result<RunRecord, RunRepositoryError> {
            Err(RunRepositoryError::NotFound)
        }

        fn list(&self) -> Result<Vec<RunSummary>, RunRepositoryError> {
            Ok(Vec::new())
        }

        fn persist(&self, _: &RunRecord, _: RunStatus) -> Result<(), RunRepositoryError> {
            Err(RunRepositoryError::NotFound)
        }

        fn reconcile_interrupted(&self) -> Result<u32, RunRepositoryError> {
            Ok(0)
        }
    }

    struct MemoryGraphRepository(Mutex<ExecutionGraphRecord>);

    impl ExecutionGraphRepository for MemoryGraphRepository {
        fn create_claimed(
            &self,
            _: &ExecutionGraphRecord,
            _: &RunId,
        ) -> Result<(), ExecutionGraphRepositoryError> {
            Err(ExecutionGraphRepositoryError::AlreadyExists)
        }

        fn get(
            &self,
            _: &ExecutionGraphId,
        ) -> Result<ExecutionGraphRecord, ExecutionGraphRepositoryError> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn compare_and_set(
            &self,
            expected_revision: u64,
            next: &ExecutionGraphRecord,
        ) -> Result<(), ExecutionGraphRepositoryError> {
            let mut current = self.0.lock().unwrap();
            if current.revision() != expected_revision {
                return Err(ExecutionGraphRepositoryError::Conflict);
            }
            *current = next.clone();
            Ok(())
        }

        fn list(&self) -> Result<Vec<ExecutionGraphRecord>, ExecutionGraphRepositoryError> {
            Ok(vec![self.0.lock().unwrap().clone()])
        }

        fn recover_structure(
            &self,
            _: &dyn RunRepository,
        ) -> Result<ExecutionGraphRecovery, ExecutionGraphRepositoryError> {
            Ok(ExecutionGraphRecovery::default())
        }
    }

    #[test]
    fn child_run_storage_failure_pauses_the_next_ready_node() {
        let parent_run_id = RunId::parse("run-parent").unwrap();
        let first_node = ExecutionNodeId::parse("item.000.plan").unwrap();
        let next_node = ExecutionNodeId::parse("item.000.behavior").unwrap();
        let zero = zero_digest().unwrap();
        let mut graph = ExecutionGraphRecord::new_claimed(
            ExecutionGraphId::parse("graph-fixture").unwrap(),
            FeatureId::parse("composition.generate").unwrap(),
            zero.clone(),
            VersionedPayload::from_typed(blueprint_schema(), &serde_json::json!({"fixture": true}))
                .unwrap(),
            vec![
                ExecutionNodeSpec {
                    node_id: first_node.clone(),
                    role_id: "mod.plan".into(),
                    depends_on: Vec::new(),
                    request_snapshot_hash: zero.clone(),
                },
                ExecutionNodeSpec {
                    node_id: next_node.clone(),
                    role_id: "composition.behavior".into(),
                    depends_on: vec![first_node.clone()],
                    request_snapshot_hash: zero,
                },
            ],
            parent_run_id.clone(),
            Utc::now(),
        )
        .unwrap();
        graph
            .start_node(&first_node, &parent_run_id, Utc::now())
            .unwrap();
        graph
            .complete_node(
                &first_node,
                &parent_run_id,
                VersionedPayload::from_typed(
                    plan_checkpoint_schema(),
                    &serde_json::json!({"fixture": true}),
                )
                .unwrap(),
                Utc::now(),
            )
            .unwrap();
        let graphs = MemoryGraphRepository(Mutex::new(graph.clone()));
        let child = RunRecord::new(
            FeatureId::parse("mod.plan").unwrap(),
            VersionedPayload::from_typed(
                schema("fixture.request"),
                &serde_json::json!({"fixture": true}),
            )
            .unwrap(),
        );

        let error = persist_completed_child(
            &RejectingRunRepository,
            &child,
            &mut graph,
            &graphs,
            &parent_run_id,
        )
        .unwrap_err();

        assert!(matches!(error, CompositionGenerateError::RunStorage));
        assert_eq!(graph.status(), ExecutionGraphStatus::Paused);
        assert!(graph.active_run_id().is_none());
        assert_eq!(graph.previous_run_id(), Some(&parent_run_id));
        assert_eq!(
            graph.nodes().get(&first_node).unwrap().status,
            ExecutionNodeStatus::Succeeded
        );
        let next = graph.nodes().get(&next_node).unwrap();
        assert_eq!(next.status, ExecutionNodeStatus::Pending);
        assert_eq!(
            next.safe_failure.as_ref().unwrap().code.as_str(),
            "run.storage_failed"
        );
        assert_eq!(graphs.get(graph.id()).unwrap(), graph);
    }

    #[test]
    fn validation_feedback_cancellation_preserves_user_and_pause_semantics() {
        for (reason, suffix, expected_status) in [
            (
                ats_runtime::CancellationReason::User,
                "user",
                ExecutionGraphStatus::Cancelled,
            ),
            (
                ats_runtime::CancellationReason::Pause,
                "pause",
                ExecutionGraphStatus::Paused,
            ),
        ] {
            let parent_run_id = RunId::parse(format!("run-cancel-{suffix}")).unwrap();
            let node_id = ExecutionNodeId::parse("item.000.behavior").unwrap();
            let zero = zero_digest().unwrap();
            let mut graph = ExecutionGraphRecord::new_claimed(
                ExecutionGraphId::parse(format!("graph-cancel-{suffix}")).unwrap(),
                FeatureId::parse("composition.generate").unwrap(),
                zero.clone(),
                VersionedPayload::from_typed(
                    blueprint_schema(),
                    &serde_json::json!({"fixture": true}),
                )
                .unwrap(),
                vec![ExecutionNodeSpec {
                    node_id: node_id.clone(),
                    role_id: "composition.behavior".into(),
                    depends_on: Vec::new(),
                    request_snapshot_hash: zero,
                }],
                parent_run_id.clone(),
                Utc::now(),
            )
            .unwrap();
            graph
                .start_node(&node_id, &parent_run_id, Utc::now())
                .unwrap();
            graph
                .complete_node(
                    &node_id,
                    &parent_run_id,
                    VersionedPayload::from_typed(
                        behavior_checkpoint_schema(),
                        &serde_json::json!({"fixture": true}),
                    )
                    .unwrap(),
                    Utc::now(),
                )
                .unwrap();
            graph.begin_validation(&parent_run_id, Utc::now()).unwrap();
            let graphs = MemoryGraphRepository(Mutex::new(graph.clone()));
            let cancellation = CancellationToken::new();
            assert!(cancellation.cancel(reason));

            assert!(matches!(
                handle_cancellation(&mut graph, &graphs, &parent_run_id, &cancellation,),
                Err(CompositionGenerateError::Cancelled)
            ));
            assert_eq!(graph.status(), expected_status);
            assert!(graph.active_run_id().is_none());
            assert_eq!(graphs.get(graph.id()).unwrap(), graph);
        }
    }
}
