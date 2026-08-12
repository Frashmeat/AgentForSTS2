use ats_kernel::{ExecutionGraphId, ExecutionNodeId, GamePackId};
use ats_runtime::{
    ExecutionCommitIntent, ExecutionFailure, ExecutionGraphRecord, ExecutionGraphRepository,
    ExecutionGraphRepositoryError, ExecutionGraphStatus, ExecutionNodeSpec, ExecutionNodeStatus,
    ModelOutputContract, RunId, hash_json,
};
use ats_workspace::CompositionDraftCreateOrMatch;

use super::*;

const SUITE_BRIEF_RECIPE_BYTES: &[u8] =
    include_bytes!("../../recipes/composition-suite-brief.json");
const SUITE_BRIEF_RECIPE_SHA256: &str =
    "b6f6c6041d6ef196eebbcc0081a8932e8d0c38c8a97c99b201d23f33c4cbcd0b";
const NODE_RECIPE_BYTES: &[u8] = include_bytes!("../../recipes/composition-plan-node.json");
const NODE_RECIPE_SHA256: &str = "3b714cb029cfea71128953014fb1cacea9fe433644a84088b78eb67202bd37b1";

const BLUEPRINT_SCHEMA_ID: &str = "feature.composition-plan-blueprint";
const SUITE_BRIEF_SCHEMA_ID: &str = "feature.composition-suite-brief";
const NODE_CHECKPOINT_SCHEMA_ID: &str = "feature.composition-plan-node-checkpoint";

pub(super) struct StagedRecipes {
    suite_brief: FeatureRecipe,
    node: FeatureRecipe,
}

impl StagedRecipes {
    pub(super) fn built_in() -> Result<Self, CompositionPlanError> {
        let suite_hash = Sha256Digest::parse(SUITE_BRIEF_RECIPE_SHA256)
            .map_err(|_| CompositionPlanError::InvalidRecipeContract)?;
        let node_hash = Sha256Digest::parse(NODE_RECIPE_SHA256)
            .map_err(|_| CompositionPlanError::InvalidRecipeContract)?;
        let suite_brief = FeatureRecipeLoader::load(SUITE_BRIEF_RECIPE_BYTES, &suite_hash)?;
        let node = FeatureRecipeLoader::load(NODE_RECIPE_BYTES, &node_hash)?;
        if suite_brief.feature_id() != &CompositionPlanFeature::id()
            || suite_brief.output_contract().schema != suite_brief_schema()
            || node.feature_id() != &CompositionPlanFeature::id()
            || node.output_contract().schema != node_checkpoint_schema()
        {
            return Err(CompositionPlanError::InvalidRecipeContract);
        }
        Ok(Self { suite_brief, node })
    }
}

#[derive(Debug, Clone)]
pub struct StagedCompositionStart {
    pub request: CompositionPlanRequest,
    pub graph: ExecutionGraphRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedCompositionBlueprint {
    schema_version: u32,
    game_pack_id: GamePackId,
    game_pack_sha256: Sha256Digest,
    draft_id: CompositionDraftId,
    composition_id: CompositionId,
    concept: String,
    profile: ItemCompositionProfile,
    root_node_id: ExecutionNodeId,
    suite_brief_node_id: Option<ExecutionNodeId>,
    item_nodes: Vec<StagedItemNode>,
    binding_rules: Vec<CompositionBindingRule>,
    guidance: Vec<String>,
    suite_brief_guidance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedItemNode {
    execution_node_id: ExecutionNodeId,
    group_id: String,
    ordinal: u32,
    item_id: ItemId,
    item_type: ItemTypeId,
    depends_on: Vec<ExecutionNodeId>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedNodeIdentity<'a> {
    execution_node_id: &'a ExecutionNodeId,
    group_id: &'a str,
    ordinal: u32,
    item_id: &'a ItemId,
    item_type: &'a ItemTypeId,
    depends_on: &'a [ExecutionNodeId],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedSuiteBrief {
    theme: String,
    node_responsibilities: BTreeMap<ExecutionNodeId, String>,
    quantity_distributions: BTreeMap<String, BTreeMap<ExecutionNodeId, u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedItemCheckpoint {
    canonical_fields: BTreeMap<ItemFieldId, ItemFieldValue>,
    behavior_intent: Vec<String>,
    localizations: BTreeMap<LocaleId, BTreeMap<LocalizationFieldId, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedFinalizedGraph {
    root_item_id: ItemId,
    definitions: Vec<StoredItemDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedValidationCheckpoint {
    definition_hashes: BTreeMap<ItemId, Sha256Digest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedCommitCheckpoint {
    draft: CompositionDraft,
    draft_payload_sha256: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct StagedCompositionPlanExecution {
    pub result: CompositionPlanResult,
    pub draft: CompositionDraft,
    pub request_snapshots: Vec<ModelRequestSnapshot>,
    pub response_models: Vec<String>,
    pub usage: TokenUsage,
}

impl CompositionPlanService {
    pub fn prepare_staged_start(
        &self,
        request: CompositionPlanRequest,
        context: CompositionPlanContext<'_>,
        run_id: RunId,
    ) -> Result<StagedCompositionStart, CompositionPlanError> {
        validate_context(&context)?;
        if request.execution.is_some() {
            return Err(CompositionPlanError::InvalidInput);
        }
        let profile_set = validate_request(context.pack, &request)?;
        let contribution: CompositionPlanContribution =
            context.contributions.decode(&contribution_slot())?;
        contribution.validate(context.pack)?;
        let guidance = contribution
            .compositions
            .iter()
            .find(|value| value.composition_id == request.composition_id)
            .ok_or(CompositionPlanError::UnsupportedComposition)?;
        let profile = ItemCompositionProfile {
            composition_id: request.composition_id.clone(),
            source: request.source.clone(),
            parameters: request.parameters.clone(),
        };
        let resolved = resolve_composition_profile(profile_set, guidance, &profile)?;
        if resolved.expected_node_count > profile_set.max_nodes() {
            return Err(CompositionPlanError::InvalidProfile);
        }

        let mut item_nodes = Vec::new();
        let mut nodes_by_group = BTreeMap::<String, Vec<ExecutionNodeId>>::new();
        for group in &guidance.node_groups {
            let count = resolve_group_count(group, &profile)?;
            let mut ids = Vec::new();
            for ordinal in 0..count {
                let execution_node_id =
                    ExecutionNodeId::parse(format!("item.generate:{}:{ordinal:03}", group.id))
                        .map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
                ids.push(execution_node_id.clone());
                item_nodes.push(StagedItemNode {
                    execution_node_id,
                    group_id: group.id.clone(),
                    ordinal,
                    item_id: staged_item_id(&request.draft_id, &group.id, ordinal)?,
                    item_type: group.item_type.clone(),
                    depends_on: Vec::new(),
                });
            }
            nodes_by_group.insert(group.id.clone(), ids);
        }
        if item_nodes.len() != usize::try_from(resolved.expected_node_count).unwrap_or(usize::MAX) {
            return Err(CompositionPlanError::InvalidPackGuidance);
        }
        let suite_brief_node_id = guidance
            .coordination
            .suite_brief
            .enabled
            .then(|| ExecutionNodeId::parse("coordination.brief"))
            .transpose()
            .map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
        for node in &mut item_nodes {
            let group = guidance
                .node_groups
                .iter()
                .find(|group| group.id == node.group_id)
                .ok_or(CompositionPlanError::InvalidPackGuidance)?;
            let mut dependencies = Vec::new();
            if let Some(brief_id) = &suite_brief_node_id {
                dependencies.push(brief_id.clone());
            }
            for group_id in &group.depends_on_group_ids {
                dependencies.extend(
                    nodes_by_group
                        .get(group_id)
                        .ok_or(CompositionPlanError::InvalidPackGuidance)?
                        .iter()
                        .cloned(),
                );
            }
            dependencies.sort();
            dependencies.dedup();
            node.depends_on = dependencies;
        }
        item_nodes = topologically_order_item_nodes(item_nodes, suite_brief_node_id.as_ref())?;
        let root_nodes = item_nodes
            .iter()
            .filter(|node| &node.item_type == profile_set.root_item_type())
            .collect::<Vec<_>>();
        if root_nodes.len() != 1 {
            return Err(CompositionPlanError::InvalidPackGuidance);
        }
        let root_node_id = root_nodes[0].execution_node_id.clone();
        let blueprint = StagedCompositionBlueprint {
            schema_version: 1,
            game_pack_id: context.pack.id().clone(),
            game_pack_sha256: context.pack.content_sha256().clone(),
            draft_id: request.draft_id.clone(),
            composition_id: request.composition_id.clone(),
            concept: request.concept.clone(),
            profile,
            root_node_id,
            suite_brief_node_id: suite_brief_node_id.clone(),
            item_nodes,
            binding_rules: guidance.binding_rules.clone(),
            guidance: guidance.guidance.clone(),
            suite_brief_guidance: guidance.coordination.suite_brief.guidance.clone(),
        };
        blueprint.validate(context.pack)?;

        let execution_graph_id = ExecutionGraphId::new();
        let mut enriched_request = request;
        enriched_request.execution = Some(CompositionExecutionRequest::Start {
            execution_graph_id: execution_graph_id.clone(),
        });
        let request_payload = VersionedPayload::from_typed(
            CompositionPlanFeature::request_schema(),
            &enriched_request,
        )
        .map_err(|_| CompositionPlanError::InvalidInput)?;
        let request_snapshot_hash =
            hash_json(&request_payload).map_err(|_| CompositionPlanError::InvalidInput)?;
        let blueprint_payload = VersionedPayload::from_typed(blueprint_schema(), &blueprint)
            .map_err(|_| CompositionPlanError::InvalidInput)?;
        let specs = blueprint.execution_specs()?;
        let graph = ExecutionGraphRecord::new_claimed(
            execution_graph_id,
            CompositionPlanFeature::id(),
            request_snapshot_hash,
            blueprint_payload,
            specs,
            run_id,
            Utc::now(),
        )
        .map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
        Ok(StagedCompositionStart {
            request: enriched_request,
            graph,
        })
    }

    pub fn prepare_staged_resume(
        &self,
        mut graph: ExecutionGraphRecord,
        expected_revision: u64,
        run_id: RunId,
        context: CompositionPlanContext<'_>,
    ) -> Result<StagedCompositionStart, CompositionPlanError> {
        validate_context(&context)?;
        if graph.owner_feature_id() != &CompositionPlanFeature::id()
            || graph.revision() != expected_revision
        {
            return Err(CompositionPlanError::ExecutionGraphConflict);
        }
        let blueprint: StagedCompositionBlueprint = graph
            .blueprint()
            .payload
            .decode(&blueprint_schema())
            .map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
        blueprint.validate(context.pack)?;
        let previous_run_id = graph
            .previous_run_id()
            .cloned()
            .ok_or(CompositionPlanError::ExecutionGraphConflict)?;
        if graph.status() != ExecutionGraphStatus::Succeeded {
            graph
                .claim(
                    expected_revision,
                    run_id,
                    previous_run_id.clone(),
                    Utc::now(),
                )
                .map_err(|_| CompositionPlanError::ExecutionGraphConflict)?;
        }
        Ok(StagedCompositionStart {
            request: CompositionPlanRequest {
                draft_id: blueprint.draft_id,
                composition_id: blueprint.composition_id,
                concept: blueprint.concept,
                source: blueprint.profile.source,
                parameters: blueprint.profile.parameters,
                execution: Some(CompositionExecutionRequest::Resume {
                    execution_graph_id: graph.id().clone(),
                    expected_revision,
                    previous_run_id,
                }),
            },
            graph,
        })
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    pub async fn execute_staged<C, I, D, G>(
        &self,
        client: &C,
        items: &I,
        drafts: &D,
        graphs: &G,
        run_id: &RunId,
        request: CompositionPlanRequest,
        context: CompositionPlanContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<StagedCompositionPlanExecution, CompositionPlanError>
    where
        C: ModelClient + ?Sized,
        I: ItemRepository + ?Sized,
        D: CompositionDraftRepository + ?Sized,
        G: ExecutionGraphRepository + ?Sized,
    {
        validate_context(&context)?;
        let graph_id = request
            .execution
            .as_ref()
            .map(|execution| match execution {
                CompositionExecutionRequest::Start { execution_graph_id }
                | CompositionExecutionRequest::Resume {
                    execution_graph_id, ..
                } => execution_graph_id,
            })
            .ok_or(CompositionPlanError::InvalidInput)?;
        let mut graph = graphs.get(graph_id).map_err(map_graph_repository_error)?;
        if graph.owner_feature_id() != &CompositionPlanFeature::id() {
            return Err(CompositionPlanError::ExecutionGraphConflict);
        }
        let blueprint: StagedCompositionBlueprint = graph
            .blueprint()
            .payload
            .decode(&blueprint_schema())
            .map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
        blueprint.validate(context.pack)?;
        if blueprint.draft_id != request.draft_id
            || blueprint.composition_id != request.composition_id
            || blueprint.concept != request.concept
            || blueprint.profile.source != request.source
            || blueprint.profile.parameters != request.parameters
        {
            return Err(CompositionPlanError::ExecutionGraphConflict);
        }
        if graph.status() == ExecutionGraphStatus::Succeeded {
            if graph.active_run_id().is_some() {
                return Err(CompositionPlanError::ExecutionGraphConflict);
            }
            let result: CompositionPlanResult = graph
                .final_result_ref()
                .ok_or_else(invalid_staged_output)?
                .payload
                .decode(&CompositionPlanFeature::result_schema())
                .map_err(|_| invalid_staged_output())?;
            let draft =
                drafts
                    .load(&result.draft_id)
                    .map_err(|error| match D::classify_error(&error) {
                        CompositionDraftRepositoryErrorKind::NotFound => {
                            CompositionPlanError::CommitConflict
                        }
                        CompositionDraftRepositoryErrorKind::Conflict
                        | CompositionDraftRepositoryErrorKind::Storage => {
                            CompositionPlanError::DraftStorage
                        }
                    })?;
            if draft.source_execution_graph_id.as_ref() != Some(graph.id())
                || draft.validated_content_digest != result.validated_content_digest
            {
                return Err(CompositionPlanError::CommitConflict);
            }
            return Ok(StagedCompositionPlanExecution {
                result,
                draft,
                request_snapshots: Vec::new(),
                response_models: Vec::new(),
                usage: TokenUsage::default(),
            });
        }
        if graph.active_run_id() != Some(run_id) {
            return Err(CompositionPlanError::ExecutionGraphConflict);
        }

        let evidence = query_evidence(
            context.truth,
            context.pack,
            &blueprint
                .item_nodes
                .iter()
                .map(|node| node.item_type.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>(),
        )?;
        let mut snapshots = Vec::new();
        let mut response_models = Vec::new();
        let mut usage = TokenUsage::default();

        if let Some(brief_node_id) = &blueprint.suite_brief_node_id
            && node_status(&graph, brief_node_id)? != ExecutionNodeStatus::Succeeded
        {
            handle_staged_cancellation(&mut graph, graphs, run_id, cancellation)?;
            let snapshot = self.render_suite_brief_snapshot(&blueprint, &context)?;
            bind_and_start_node(
                &mut graph,
                graphs,
                brief_node_id,
                run_id,
                snapshot.request_sha256().clone(),
            )?;
            let response = match client.complete(snapshot.clone(), cancellation).await {
                Ok(response) => response,
                Err(error) => {
                    if cancellation.is_cancelled() {
                        return handle_staged_cancellation(
                            &mut graph,
                            graphs,
                            run_id,
                            cancellation,
                        )
                        .and(Err(CompositionPlanError::Cancelled));
                    }
                    let error = CompositionPlanError::Model(error);
                    pause_failed_node(&mut graph, graphs, brief_node_id, run_id, &error)?;
                    return Err(error);
                }
            };
            handle_staged_cancellation(&mut graph, graphs, run_id, cancellation)?;
            if response.finish_reason == FinishReason::MaxTokens {
                let error = CompositionPlanError::TruncatedModelOutput;
                pause_failed_node(&mut graph, graphs, brief_node_id, run_id, &error)?;
                return Err(error);
            }
            let brief: StagedSuiteBrief = match serde_json::from_str(&response.content) {
                Ok(value) => value,
                Err(_) => {
                    let error = invalid_staged_output();
                    pause_failed_node(&mut graph, graphs, brief_node_id, run_id, &error)?;
                    return Err(error);
                }
            };
            if let Err(error) = validate_suite_brief(&blueprint, &brief) {
                pause_failed_node(&mut graph, graphs, brief_node_id, run_id, &error)?;
                return Err(error);
            }
            let checkpoint =
                VersionedPayload::from_typed(suite_brief_schema(), &brief).map_err(|_| {
                    CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
                        CompositionPlanFailureReason::DraftGraphInvalid,
                    ))
                })?;
            complete_node(&mut graph, graphs, brief_node_id, run_id, checkpoint)?;
            accumulate_usage(&mut usage, &response.usage);
            response_models.push(response.model);
            snapshots.push(snapshot);
        }

        for node in &blueprint.item_nodes {
            if node_status(&graph, &node.execution_node_id)? == ExecutionNodeStatus::Succeeded {
                continue;
            }
            handle_staged_cancellation(&mut graph, graphs, run_id, cancellation)?;
            let brief = decode_suite_brief(&graph, &blueprint)?;
            let snapshot =
                self.render_item_snapshot(&blueprint, node, brief.as_ref(), &evidence, &context)?;
            bind_and_start_node(
                &mut graph,
                graphs,
                &node.execution_node_id,
                run_id,
                snapshot.request_sha256().clone(),
            )?;
            let response = match client.complete(snapshot.clone(), cancellation).await {
                Ok(response) => response,
                Err(error) => {
                    if cancellation.is_cancelled() {
                        return handle_staged_cancellation(
                            &mut graph,
                            graphs,
                            run_id,
                            cancellation,
                        )
                        .and(Err(CompositionPlanError::Cancelled));
                    }
                    let error = CompositionPlanError::Model(error);
                    pause_failed_node(&mut graph, graphs, &node.execution_node_id, run_id, &error)?;
                    return Err(error);
                }
            };
            handle_staged_cancellation(&mut graph, graphs, run_id, cancellation)?;
            if response.finish_reason == FinishReason::MaxTokens {
                let error = CompositionPlanError::TruncatedModelOutput;
                pause_failed_node(&mut graph, graphs, &node.execution_node_id, run_id, &error)?;
                return Err(error);
            }
            let checkpoint: StagedItemCheckpoint = match serde_json::from_str(&response.content) {
                Ok(value) => value,
                Err(_) => {
                    let error = invalid_staged_output();
                    pause_failed_node(&mut graph, graphs, &node.execution_node_id, run_id, &error)?;
                    return Err(error);
                }
            };
            let brief = decode_suite_brief(&graph, &blueprint)?;
            if let Err(error) = validate_item_checkpoint(
                context.pack,
                &blueprint,
                node,
                brief.as_ref(),
                &checkpoint,
            ) {
                pause_failed_node(&mut graph, graphs, &node.execution_node_id, run_id, &error)?;
                return Err(error);
            }
            complete_node(
                &mut graph,
                graphs,
                &node.execution_node_id,
                run_id,
                VersionedPayload::from_typed(node_checkpoint_schema(), &checkpoint)
                    .map_err(|_| invalid_staged_output())?,
            )?;
            accumulate_usage(&mut usage, &response.usage);
            response_models.push(response.model);
            snapshots.push(snapshot);
        }

        let finalize_id = ExecutionNodeId::parse("graph.finalize")
            .map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
        let finalized = if node_status(&graph, &finalize_id)? == ExecutionNodeStatus::Succeeded {
            decode_checkpoint(&graph, &finalize_id, &finalized_graph_schema())?
        } else {
            let planned = assemble_model_plan(context.pack, &blueprint, &graph)?;
            let profile_set = context
                .pack
                .composition_profile(&blueprint.composition_id)
                .ok_or(CompositionPlanError::UnsupportedComposition)?;
            let contribution: CompositionPlanContribution =
                context.contributions.decode(&contribution_slot())?;
            let guidance = contribution
                .compositions
                .iter()
                .find(|value| value.composition_id == blueprint.composition_id)
                .ok_or(CompositionPlanError::UnsupportedComposition)?;
            let definitions = build_definitions(
                context.pack,
                profile_set,
                guidance,
                &blueprint.profile,
                planned,
                &BTreeMap::new(),
            )?;
            let value = StagedFinalizedGraph {
                root_item_id: blueprint
                    .item_nodes
                    .iter()
                    .find(|node| node.execution_node_id == blueprint.root_node_id)
                    .map(|node| node.item_id.clone())
                    .ok_or(CompositionPlanError::InvalidPackGuidance)?,
                definitions,
            };
            start_local_node(&mut graph, graphs, &finalize_id, run_id)?;
            complete_node(
                &mut graph,
                graphs,
                &finalize_id,
                run_id,
                VersionedPayload::from_typed(finalized_graph_schema(), &value)
                    .map_err(|_| invalid_staged_output())?,
            )?;
            value
        };

        let validate_id = ExecutionNodeId::parse("graph.validate")
            .map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
        if node_status(&graph, &validate_id)? != ExecutionNodeStatus::Succeeded {
            let definition_hashes = finalized
                .definitions
                .iter()
                .map(|stored| {
                    if stored.definition.definition_hash().ok().as_ref()
                        != Some(&stored.definition_hash)
                    {
                        return Err(invalid_staged_output());
                    }
                    Ok((
                        stored.definition.item_id.clone(),
                        stored.definition_hash.clone(),
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            start_local_node(&mut graph, graphs, &validate_id, run_id)?;
            complete_node(
                &mut graph,
                graphs,
                &validate_id,
                run_id,
                VersionedPayload::from_typed(
                    validation_checkpoint_schema(),
                    &StagedValidationCheckpoint { definition_hashes },
                )
                .map_err(|_| invalid_staged_output())?,
            )?;
        }

        let commit_id = ExecutionNodeId::parse("draft.commit")
            .map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
        if node_status(&graph, &commit_id)? != ExecutionNodeStatus::Succeeded {
            let validated_content_digest = validated_content_digest(&graph, &commit_id)?;
            let draft = build_staged_draft(
                items,
                context.pack,
                &blueprint,
                &finalized,
                graph.id().clone(),
                validated_content_digest,
            )?;
            let checkpoint = StagedCommitCheckpoint {
                draft_payload_sha256: draft
                    .payload_sha256()
                    .map_err(|_| invalid_staged_output())?,
                draft,
            };
            start_local_node(&mut graph, graphs, &commit_id, run_id)?;
            complete_node(
                &mut graph,
                graphs,
                &commit_id,
                run_id,
                VersionedPayload::from_typed(commit_checkpoint_schema(), &checkpoint)
                    .map_err(|_| invalid_staged_output())?,
            )?;
        }

        if graph.status() == ExecutionGraphStatus::Running {
            mutate_graph(&mut graph, graphs, |graph| {
                graph.begin_validation(run_id, Utc::now())
            })?;
        }

        if graph.status() == ExecutionGraphStatus::Validating {
            let checkpoint: StagedCommitCheckpoint =
                decode_checkpoint(&graph, &commit_id, &commit_checkpoint_schema())?;
            let canonical_draft =
                VersionedPayload::from_typed(composition_draft_payload_schema(), &checkpoint.draft)
                    .map_err(|_| invalid_staged_output())?;
            let intent = ExecutionCommitIntent::draft(
                checkpoint.draft.draft_id.clone(),
                canonical_draft,
                checkpoint.draft_payload_sha256,
                checkpoint
                    .draft
                    .validated_content_digest
                    .clone()
                    .ok_or_else(invalid_staged_output)?,
            );
            mutate_graph(&mut graph, graphs, |graph| {
                graph.prepare_commit(run_id, intent, Utc::now())
            })?;
        }

        let intent = graph
            .commit_intent()
            .cloned()
            .ok_or_else(invalid_staged_output)?;
        let canonical_draft = intent
            .canonical_draft
            .as_ref()
            .ok_or_else(invalid_staged_output)?;
        let draft_payload_sha256 = intent
            .draft_payload_sha256
            .as_ref()
            .ok_or_else(invalid_staged_output)?;
        let draft: CompositionDraft = canonical_draft
            .decode(&composition_draft_payload_schema())
            .map_err(|_| invalid_staged_output())?;
        match drafts.create_or_match(&draft, draft_payload_sha256) {
            Ok(CompositionDraftCreateOrMatch::Created | CompositionDraftCreateOrMatch::Matched) => {
            }
            Err(error)
                if D::classify_error(&error) == CompositionDraftRepositoryErrorKind::Conflict =>
            {
                mutate_graph(&mut graph, graphs, |graph| {
                    graph.block_commit(run_id, Utc::now())
                })?;
                return Err(CompositionPlanError::CommitConflict);
            }
            Err(_) => return Err(CompositionPlanError::DraftStorage),
        }

        let model_request_sha256 = snapshots
            .last()
            .map(|snapshot| snapshot.request_sha256().clone())
            .or_else(|| {
                blueprint.item_nodes.last().and_then(|node| {
                    graph
                        .nodes()
                        .get(&node.execution_node_id)
                        .map(|record| record.request_snapshot_hash.clone())
                })
            })
            .ok_or_else(invalid_staged_output)?;
        let result = CompositionPlanResult {
            draft_id: draft.draft_id.clone(),
            revision: draft.revision,
            root_item_id: draft.root_item_id.clone(),
            node_count: u32::try_from(draft.nodes.len()).map_err(|_| invalid_staged_output())?,
            model_request_sha256,
            execution_graph_id: Some(graph.id().clone()),
            validated_content_digest: draft.validated_content_digest.clone(),
        };
        if graph.status() == ExecutionGraphStatus::CommitPrepared {
            let result_payload =
                VersionedPayload::from_typed(CompositionPlanFeature::result_schema(), &result)
                    .map_err(|_| invalid_staged_output())?;
            mutate_graph(&mut graph, graphs, |graph| {
                graph.mark_succeeded(run_id, result_payload, Utc::now())
            })?;
        }
        Ok(StagedCompositionPlanExecution {
            result,
            draft,
            request_snapshots: snapshots,
            response_models,
            usage,
        })
    }

    fn render_suite_brief_snapshot(
        &self,
        blueprint: &StagedCompositionBlueprint,
        context: &CompositionPlanContext<'_>,
    ) -> Result<ModelRequestSnapshot, CompositionPlanError> {
        let output_contract = suite_brief_output_contract(blueprint)?;
        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&output_contract.json_schema)?,
            ),
            (
                "pack.guidance".into(),
                serialize(&blueprint.suite_brief_guidance)?,
            ),
            ("composition.blueprint".into(), serialize(blueprint)?),
            ("request.concept".into(), blueprint.concept.clone()),
            (
                "project.context".into(),
                bounded_optional(context.project_context, 8_000)?,
            ),
            (
                "runtime.custom_instructions".into(),
                bounded_optional(context.custom_instructions, 4_000)?,
            ),
        ]);
        let request = self
            .staged_recipes
            .suite_brief
            .render_with_output_contract(&slots, context.model.clone(), output_contract)?;
        ModelRequestSnapshot::new(
            CompositionPlanFeature::id(),
            self.staged_recipes.suite_brief.recipe_ref(),
            ModelGamePackRef {
                id: context.pack.id().clone(),
                sha256: context.pack.content_sha256().clone(),
            },
            Some(context.truth.manifest().snapshot_id().clone()),
            Vec::new(),
            request,
        )
        .map_err(CompositionPlanError::Request)
    }

    fn render_item_snapshot(
        &self,
        blueprint: &StagedCompositionBlueprint,
        node: &StagedItemNode,
        brief: Option<&StagedSuiteBrief>,
        evidence: &[TruthEvidenceRecord],
        context: &CompositionPlanContext<'_>,
    ) -> Result<ModelRequestSnapshot, CompositionPlanError> {
        let descriptor = context
            .pack
            .item_type(&node.item_type)
            .ok_or(CompositionPlanError::InvalidPackGuidance)?;
        let output_contract = node_output_contract(descriptor)?;
        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&output_contract.json_schema)?,
            ),
            ("pack.guidance".into(), serialize(&blueprint.guidance)?),
            ("item.descriptor".into(), serialize(descriptor)?),
            (
                "node.identity".into(),
                serialize(&StagedNodeIdentity::from(node))?,
            ),
            (
                "suite.brief".into(),
                brief.map_or_else(|| Ok(String::new()), serialize)?,
            ),
            ("truth.evidence".into(), serialize(evidence)?),
            ("request.concept".into(), blueprint.concept.clone()),
            (
                "project.context".into(),
                bounded_optional(context.project_context, 8_000)?,
            ),
            (
                "runtime.custom_instructions".into(),
                bounded_optional(context.custom_instructions, 4_000)?,
            ),
        ]);
        let request = self.staged_recipes.node.render_with_output_contract(
            &slots,
            context.model.clone(),
            output_contract,
        )?;
        ModelRequestSnapshot::new(
            CompositionPlanFeature::id(),
            self.staged_recipes.node.recipe_ref(),
            ModelGamePackRef {
                id: context.pack.id().clone(),
                sha256: context.pack.content_sha256().clone(),
            },
            Some(context.truth.manifest().snapshot_id().clone()),
            Vec::new(),
            request,
        )
        .map_err(CompositionPlanError::Request)
    }
}

impl StagedCompositionBlueprint {
    fn validate(&self, pack: &LoadedGamePack) -> Result<(), CompositionPlanError> {
        if self.schema_version != 1
            || &self.game_pack_id != pack.id()
            || &self.game_pack_sha256 != pack.content_sha256()
            || self.item_nodes.is_empty()
            || self.item_nodes.len() > 128
            || self
                .item_nodes
                .iter()
                .filter(|node| node.execution_node_id == self.root_node_id)
                .count()
                != 1
            || self.item_nodes.iter().any(|node| {
                pack.item_type(&node.item_type).is_none()
                    || !valid_group_id(&node.group_id)
                    || node.depends_on.contains(&node.execution_node_id)
            })
        {
            return Err(CompositionPlanError::InvalidPackGuidance);
        }
        let ids = self
            .item_nodes
            .iter()
            .map(|node| &node.execution_node_id)
            .collect::<BTreeSet<_>>();
        if ids.len() != self.item_nodes.len()
            || self.item_nodes.iter().any(|node| {
                node.depends_on.iter().any(|dependency| {
                    self.suite_brief_node_id.as_ref() != Some(dependency)
                        && !ids.contains(dependency)
                })
            })
        {
            return Err(CompositionPlanError::InvalidPackGuidance);
        }
        Ok(())
    }

    fn execution_specs(&self) -> Result<Vec<ExecutionNodeSpec>, CompositionPlanError> {
        let mut specs = Vec::new();
        if let Some(node_id) = &self.suite_brief_node_id {
            specs.push(ExecutionNodeSpec {
                node_id: node_id.clone(),
                role_id: "coordination.brief".into(),
                depends_on: Vec::new(),
                request_snapshot_hash: hash_json(self)
                    .map_err(|_| CompositionPlanError::InvalidInput)?,
            });
        }
        for node in &self.item_nodes {
            specs.push(ExecutionNodeSpec {
                node_id: node.execution_node_id.clone(),
                role_id: "item.generate".into(),
                depends_on: node.depends_on.clone(),
                request_snapshot_hash: hash_json(&StagedNodeIdentity::from(node))
                    .map_err(|_| CompositionPlanError::InvalidInput)?,
            });
        }
        let item_ids = self
            .item_nodes
            .iter()
            .map(|node| node.execution_node_id.clone())
            .collect::<Vec<_>>();
        specs.push(local_spec("graph.finalize", item_ids)?);
        specs.push(local_spec(
            "graph.validate",
            vec![
                ExecutionNodeId::parse("graph.finalize")
                    .map_err(|_| CompositionPlanError::InvalidPackGuidance)?,
            ],
        )?);
        specs.push(local_spec(
            "draft.commit",
            vec![
                ExecutionNodeId::parse("graph.validate")
                    .map_err(|_| CompositionPlanError::InvalidPackGuidance)?,
            ],
        )?);
        Ok(specs)
    }
}

impl<'a> From<&'a StagedItemNode> for StagedNodeIdentity<'a> {
    fn from(node: &'a StagedItemNode) -> Self {
        Self {
            execution_node_id: &node.execution_node_id,
            group_id: &node.group_id,
            ordinal: node.ordinal,
            item_id: &node.item_id,
            item_type: &node.item_type,
            depends_on: &node.depends_on,
        }
    }
}

fn local_spec(
    id: &str,
    depends_on: Vec<ExecutionNodeId>,
) -> Result<ExecutionNodeSpec, CompositionPlanError> {
    let node_id =
        ExecutionNodeId::parse(id).map_err(|_| CompositionPlanError::InvalidPackGuidance)?;
    Ok(ExecutionNodeSpec {
        request_snapshot_hash: hash_json(&serde_json::json!({
            "nodeId": node_id,
            "dependsOn": depends_on,
        }))
        .map_err(|_| CompositionPlanError::InvalidInput)?,
        node_id,
        role_id: id.into(),
        depends_on,
    })
}

fn topologically_order_item_nodes(
    nodes: Vec<StagedItemNode>,
    brief_id: Option<&ExecutionNodeId>,
) -> Result<Vec<StagedItemNode>, CompositionPlanError> {
    let mut pending = nodes
        .into_iter()
        .map(|node| (node.execution_node_id.clone(), node))
        .collect::<BTreeMap<_, _>>();
    let mut completed = BTreeSet::new();
    let mut ordered = Vec::with_capacity(pending.len());
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .find(|(_, node)| {
                node.depends_on.iter().all(|dependency| {
                    brief_id == Some(dependency) || completed.contains(dependency)
                })
            })
            .map(|(id, _)| id.clone())
            .ok_or(CompositionPlanError::InvalidPackGuidance)?;
        let node = pending
            .remove(&ready)
            .ok_or(CompositionPlanError::InvalidPackGuidance)?;
        completed.insert(ready);
        ordered.push(node);
    }
    Ok(ordered)
}

fn staged_item_id(
    draft_id: &CompositionDraftId,
    group_id: &str,
    ordinal: u32,
) -> Result<ItemId, CompositionPlanError> {
    let candidate = format!("{draft_id}-{group_id}-{:03}", ordinal + 1);
    if candidate.len() <= 128 {
        return ItemId::parse(candidate).map_err(|_| CompositionPlanError::InvalidPackGuidance);
    }
    let digest = hash_json(&serde_json::json!({
        "draftId": draft_id,
        "groupId": group_id,
        "ordinal": ordinal,
    }))
    .map_err(|_| CompositionPlanError::InvalidInput)?;
    ItemId::parse(format!(
        "{}-{}",
        &group_id[..group_id.len().min(48)],
        &digest.as_str()[..32]
    ))
    .map_err(|_| CompositionPlanError::InvalidPackGuidance)
}

fn blueprint_schema() -> SchemaRef {
    schema(BLUEPRINT_SCHEMA_ID)
}

fn suite_brief_schema() -> SchemaRef {
    schema(SUITE_BRIEF_SCHEMA_ID)
}

fn node_checkpoint_schema() -> SchemaRef {
    schema(NODE_CHECKPOINT_SCHEMA_ID)
}

fn suite_brief_output_contract(
    blueprint: &StagedCompositionBlueprint,
) -> Result<ModelOutputContract, CompositionPlanError> {
    let responsibilities = blueprint
        .item_nodes
        .iter()
        .map(|node| {
            (
                node.execution_node_id.as_str().into(),
                serde_json::json!({"type":"string","minLength":1,"maxLength":1000}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let responsibility_ids = responsibilities.keys().cloned().collect::<Vec<_>>();
    let mut distribution_properties = serde_json::Map::new();
    for rule in blueprint.binding_rules.iter().filter(|rule| {
        matches!(
            rule.quantity_policy,
            CompositionQuantityPolicy::BriefDistribution { .. }
        )
    }) {
        let CompositionQuantityPolicy::BriefDistribution {
            min_per_target,
            max_per_target,
            ..
        } = &rule.quantity_policy
        else {
            unreachable!();
        };
        let targets = blueprint
            .item_nodes
            .iter()
            .filter(|node| rule.target_group_ids.contains(&node.group_id))
            .map(|node| {
                (
                    node.execution_node_id.as_str().into(),
                    serde_json::json!({
                        "type":"integer",
                        "minimum": min_per_target,
                        "maximum": max_per_target,
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let required = targets.keys().cloned().collect::<Vec<_>>();
        distribution_properties.insert(
            binding_key(rule),
            serde_json::json!({
                "type":"object",
                "additionalProperties":false,
                "required":required,
                "properties":targets,
            }),
        );
    }
    let distribution_ids = distribution_properties.keys().cloned().collect::<Vec<_>>();
    Ok(ModelOutputContract {
        schema: suite_brief_schema(),
        json_schema: serde_json::json!({
            "type":"object",
            "additionalProperties":false,
            "required":["theme","nodeResponsibilities","quantityDistributions"],
            "properties":{
                "theme":{"type":"string","minLength":1,"maxLength":2000},
                "nodeResponsibilities":{
                    "type":"object","additionalProperties":false,
                    "required":responsibility_ids,"properties":responsibilities,
                },
                "quantityDistributions":{
                    "type":"object","additionalProperties":false,
                    "required":distribution_ids,"properties":distribution_properties,
                }
            }
        }),
    })
}

fn node_output_contract(
    descriptor: &ItemTypeDescriptor,
) -> Result<ModelOutputContract, CompositionPlanError> {
    let mut value = composition_node_schema(descriptor, &BTreeMap::new())?;
    let object = value
        .as_object_mut()
        .ok_or(CompositionPlanError::InvalidPackGuidance)?;
    let properties = object
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or(CompositionPlanError::InvalidPackGuidance)?;
    properties.remove("itemId");
    properties.remove("itemType");
    properties.remove("referenceBindings");
    object.insert(
        "required".into(),
        serde_json::json!(["canonicalFields", "behaviorIntent", "localizations"]),
    );
    Ok(ModelOutputContract {
        schema: node_checkpoint_schema(),
        json_schema: value,
    })
}

fn binding_key(rule: &CompositionBindingRule) -> String {
    format!("{}.{}", rule.source_group_id, rule.slot_id)
}

fn validate_suite_brief(
    blueprint: &StagedCompositionBlueprint,
    brief: &StagedSuiteBrief,
) -> Result<(), CompositionPlanError> {
    if !valid_text(&brief.theme, 2_000)
        || brief.node_responsibilities.len() != blueprint.item_nodes.len()
        || blueprint.item_nodes.iter().any(|node| {
            brief
                .node_responsibilities
                .get(&node.execution_node_id)
                .is_none_or(|value| !valid_text(value, 1_000))
        })
    {
        return Err(CompositionPlanError::InvalidModelOutput(
            CompositionPlanFailureDetails::reason(CompositionPlanFailureReason::DraftGraphInvalid),
        ));
    }
    let distribution_rules = blueprint
        .binding_rules
        .iter()
        .filter(|rule| {
            matches!(
                rule.quantity_policy,
                CompositionQuantityPolicy::BriefDistribution { .. }
            )
        })
        .collect::<Vec<_>>();
    if brief.quantity_distributions.len() != distribution_rules.len() {
        return Err(CompositionPlanError::InvalidModelOutput(
            CompositionPlanFailureDetails::reason(
                CompositionPlanFailureReason::ReferenceTotalQuantity,
            ),
        ));
    }
    for rule in distribution_rules {
        let CompositionQuantityPolicy::BriefDistribution {
            total_parameter_id,
            min_per_target,
            max_per_target,
        } = &rule.quantity_policy
        else {
            unreachable!();
        };
        let distribution = brief
            .quantity_distributions
            .get(&binding_key(rule))
            .ok_or_else(|| {
                CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
                    CompositionPlanFailureReason::ReferenceTotalQuantity,
                ))
            })?;
        let targets = blueprint
            .item_nodes
            .iter()
            .filter(|node| rule.target_group_ids.contains(&node.group_id))
            .map(|node| &node.execution_node_id)
            .collect::<BTreeSet<_>>();
        if distribution.keys().collect::<BTreeSet<_>>() != targets
            || distribution
                .values()
                .any(|value| value < min_per_target || value > max_per_target)
            || distribution.values().copied().sum::<u32>()
                != blueprint
                    .profile
                    .parameters
                    .get(total_parameter_id)
                    .copied()
                    .ok_or(CompositionPlanError::InvalidProfile)?
        {
            return Err(CompositionPlanError::InvalidModelOutput(
                CompositionPlanFailureDetails::reason(
                    CompositionPlanFailureReason::ReferenceTotalQuantity,
                ),
            ));
        }
    }
    Ok(())
}

fn validate_item_checkpoint(
    pack: &LoadedGamePack,
    blueprint: &StagedCompositionBlueprint,
    node: &StagedItemNode,
    brief: Option<&StagedSuiteBrief>,
    checkpoint: &StagedItemCheckpoint,
) -> Result<(), CompositionPlanError> {
    if checkpoint.behavior_intent.is_empty()
        || checkpoint.behavior_intent.len() > 32
        || checkpoint
            .behavior_intent
            .iter()
            .any(|value| !valid_text(value, 1_000))
    {
        return Err(invalid_staged_output());
    }
    let mut definition = ItemDefinition::new(node.item_id.clone(), node.item_type.clone());
    definition.canonical_fields = checkpoint.canonical_fields.clone();
    definition.behavior_intent = checkpoint.behavior_intent.clone();
    definition.localizations = checkpoint
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
    if node.execution_node_id == blueprint.root_node_id {
        definition.composition_profile = Some(blueprint.profile.clone());
    }
    for (slot_id, planned) in planned_bindings_for_node(pack, blueprint, node, brief)? {
        let values = planned
            .into_iter()
            .map(|binding| match binding {
                PlannedReference::Identity {
                    item_id,
                    expected_item_type,
                } => ItemReferenceBinding::Identity {
                    item_id,
                    expected_item_type,
                },
                PlannedReference::Pinned { item_id, quantity } => ItemReferenceBinding::Pinned {
                    item_id,
                    definition_hash: Sha256Digest::parse("0".repeat(64))
                        .expect("built-in placeholder hash is valid"),
                    quantity,
                },
            })
            .collect();
        definition.reference_bindings.insert(slot_id, values);
    }
    ItemDefinitionValidator::validate(pack, &definition, ItemDefinitionValidationMode::Draft)
        .map_err(|_| invalid_staged_output())
}

fn assemble_model_plan(
    pack: &LoadedGamePack,
    blueprint: &StagedCompositionBlueprint,
    graph: &ExecutionGraphRecord,
) -> Result<ModelCompositionPlan, CompositionPlanError> {
    let brief = decode_suite_brief(graph, blueprint)?;
    let mut nodes = Vec::with_capacity(blueprint.item_nodes.len());
    for node in &blueprint.item_nodes {
        let checkpoint: StagedItemCheckpoint =
            decode_checkpoint(graph, &node.execution_node_id, &node_checkpoint_schema())?;
        nodes.push(ModelCompositionNode {
            item_id: node.item_id.clone(),
            item_type: node.item_type.clone(),
            canonical_fields: checkpoint.canonical_fields,
            behavior_intent: checkpoint.behavior_intent,
            localizations: checkpoint.localizations,
            reference_bindings: planned_bindings_for_node(pack, blueprint, node, brief.as_ref())?,
        });
    }
    let root_item_id = blueprint
        .item_nodes
        .iter()
        .find(|node| node.execution_node_id == blueprint.root_node_id)
        .map(|node| node.item_id.clone())
        .ok_or(CompositionPlanError::InvalidPackGuidance)?;
    Ok(ModelCompositionPlan {
        root_item_id,
        nodes,
    })
}

fn planned_bindings_for_node(
    pack: &LoadedGamePack,
    blueprint: &StagedCompositionBlueprint,
    source_node: &StagedItemNode,
    brief: Option<&StagedSuiteBrief>,
) -> Result<BTreeMap<ItemReferenceSlotId, Vec<PlannedReference>>, CompositionPlanError> {
    let descriptor = pack
        .item_type(&source_node.item_type)
        .ok_or(CompositionPlanError::InvalidPackGuidance)?;
    let mut result = BTreeMap::new();
    for rule in blueprint.binding_rules.iter().filter(|rule| {
        rule.source_group_id == source_node.group_id
            && rule.measure == ReferenceBindingMeasure::Bindings
    }) {
        let slot = descriptor
            .reference_slots()
            .iter()
            .find(|slot| slot.id() == &rule.slot_id)
            .ok_or(CompositionPlanError::InvalidPackGuidance)?;
        let total_rule = blueprint.binding_rules.iter().find(|candidate| {
            candidate.source_group_id == rule.source_group_id
                && candidate.slot_id == rule.slot_id
                && candidate.measure == ReferenceBindingMeasure::TotalQuantity
        });
        let mut targets = blueprint
            .item_nodes
            .iter()
            .filter(|node| rule.target_group_ids.contains(&node.group_id))
            .collect::<Vec<_>>();
        targets.sort_by(|left, right| {
            left.group_id
                .cmp(&right.group_id)
                .then(left.ordinal.cmp(&right.ordinal))
        });
        let values = targets
            .into_iter()
            .map(|target| match slot.kind() {
                ItemReferenceKind::Identity => Ok(PlannedReference::Identity {
                    item_id: target.item_id.clone(),
                    expected_item_type: target.item_type.clone(),
                }),
                ItemReferenceKind::Pinned => Ok(PlannedReference::Pinned {
                    item_id: target.item_id.clone(),
                    quantity: binding_quantity(rule, total_rule, target, brief)?,
                }),
            })
            .collect::<Result<Vec<_>, CompositionPlanError>>()?;
        if !values.is_empty() {
            result.insert(rule.slot_id.clone(), values);
        }
    }
    Ok(result)
}

fn binding_quantity(
    binding_rule: &CompositionBindingRule,
    total_rule: Option<&CompositionBindingRule>,
    target: &StagedItemNode,
    brief: Option<&StagedSuiteBrief>,
) -> Result<u32, CompositionPlanError> {
    let policy = total_rule
        .map(|rule| &rule.quantity_policy)
        .unwrap_or(&binding_rule.quantity_policy);
    match policy {
        CompositionQuantityPolicy::Constant { value } => Ok(*value),
        CompositionQuantityPolicy::BriefDistribution { .. } => {
            let total_rule = total_rule.ok_or(CompositionPlanError::InvalidPackGuidance)?;
            brief
                .and_then(|brief| brief.quantity_distributions.get(&binding_key(total_rule)))
                .and_then(|distribution| distribution.get(&target.execution_node_id))
                .copied()
                .ok_or_else(invalid_staged_output)
        }
    }
}

fn build_staged_draft<I: ItemRepository + ?Sized>(
    items: &I,
    pack: &LoadedGamePack,
    blueprint: &StagedCompositionBlueprint,
    finalized: &StagedFinalizedGraph,
    graph_id: ExecutionGraphId,
    validated_content_digest: Sha256Digest,
) -> Result<CompositionDraft, CompositionPlanError> {
    let mut nodes = BTreeMap::new();
    for stored in &finalized.definitions {
        let expected_current_definition_hash = match items.load_current(&stored.definition.item_id)
        {
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
                definition: stored.definition.clone(),
                expected_current_definition_hash,
            },
        );
    }
    CompositionDraft::new_staged(
        blueprint.draft_id.clone(),
        pack.id().clone(),
        pack.content_sha256().clone(),
        finalized.root_item_id.clone(),
        blueprint.profile.clone(),
        nodes,
        graph_id,
        validated_content_digest,
        Utc::now(),
    )
    .map_err(|_| invalid_staged_output())
}

fn validated_content_digest(
    graph: &ExecutionGraphRecord,
    excluded_node: &ExecutionNodeId,
) -> Result<Sha256Digest, CompositionPlanError> {
    let nodes = graph
        .nodes()
        .iter()
        .filter(|(id, node)| *id != excluded_node && node.status == ExecutionNodeStatus::Succeeded)
        .map(|(id, node)| {
            let checkpoint = node
                .active_checkpoint
                .as_ref()
                .ok_or_else(invalid_staged_output)?;
            Ok(serde_json::json!({
                "nodeId": id,
                "roleId": node.role_id,
                "dependsOn": node.depends_on,
                "checkpointSchema": checkpoint.payload.schema(),
                "checkpointSha256": checkpoint.sha256,
            }))
        })
        .collect::<Result<Vec<_>, CompositionPlanError>>()?;
    hash_json(&serde_json::json!({
        "ownerFeatureId": graph.owner_feature_id(),
        "requestSnapshotHash": graph.request_snapshot_hash(),
        "blueprintSchema": graph.blueprint().payload.schema(),
        "blueprintSha256": graph.blueprint().sha256,
        "nodes": nodes,
    }))
    .map_err(|_| invalid_staged_output())
}

fn decode_suite_brief(
    graph: &ExecutionGraphRecord,
    blueprint: &StagedCompositionBlueprint,
) -> Result<Option<StagedSuiteBrief>, CompositionPlanError> {
    blueprint
        .suite_brief_node_id
        .as_ref()
        .map(|id| decode_checkpoint(graph, id, &suite_brief_schema()))
        .transpose()
}

fn decode_checkpoint<T: for<'de> Deserialize<'de>>(
    graph: &ExecutionGraphRecord,
    node_id: &ExecutionNodeId,
    schema: &SchemaRef,
) -> Result<T, CompositionPlanError> {
    graph
        .nodes()
        .get(node_id)
        .and_then(|node| node.active_checkpoint.as_ref())
        .ok_or_else(invalid_staged_output)?
        .payload
        .decode(schema)
        .map_err(|_| invalid_staged_output())
}

fn bind_and_start_node<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    node_id: &ExecutionNodeId,
    run_id: &RunId,
    request_snapshot_hash: Sha256Digest,
) -> Result<(), CompositionPlanError> {
    mutate_graph(graph, repository, |graph| {
        graph.set_node_request_snapshot_hash(node_id, run_id, request_snapshot_hash, Utc::now())
    })?;
    mutate_graph(graph, repository, |graph| {
        graph.start_node(node_id, run_id, Utc::now())
    })
}

fn start_local_node<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    node_id: &ExecutionNodeId,
    run_id: &RunId,
) -> Result<(), CompositionPlanError> {
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
) -> Result<(), CompositionPlanError> {
    mutate_graph(graph, repository, |graph| {
        graph.complete_node(node_id, run_id, checkpoint, Utc::now())
    })
}

fn pause_failed_node<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    node_id: &ExecutionNodeId,
    run_id: &RunId,
    error: &CompositionPlanError,
) -> Result<(), CompositionPlanError> {
    let failure = error.run_failure();
    let safe = ExecutionFailure::new(failure.code, "composition.node")
        .map_err(|_| CompositionPlanError::ExecutionGraphStorage)?;
    mutate_graph(graph, repository, |graph| {
        graph.pause_after_node_failure(node_id, run_id, safe, Utc::now())
    })
}

fn handle_staged_cancellation<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    run_id: &RunId,
    cancellation: &CancellationToken,
) -> Result<(), CompositionPlanError> {
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
    Err(CompositionPlanError::Cancelled)
}

fn mutate_graph<G: ExecutionGraphRepository + ?Sized>(
    graph: &mut ExecutionGraphRecord,
    repository: &G,
    mutate: impl FnOnce(&mut ExecutionGraphRecord) -> Result<(), ats_runtime::ExecutionGraphError>,
) -> Result<(), CompositionPlanError> {
    let expected_revision = graph.revision();
    mutate(graph).map_err(|_| CompositionPlanError::ExecutionGraphConflict)?;
    repository
        .compare_and_set(expected_revision, graph)
        .map_err(map_graph_repository_error)
}

fn node_status(
    graph: &ExecutionGraphRecord,
    node_id: &ExecutionNodeId,
) -> Result<ExecutionNodeStatus, CompositionPlanError> {
    graph
        .nodes()
        .get(node_id)
        .map(|node| node.status)
        .ok_or(CompositionPlanError::InvalidPackGuidance)
}

fn map_graph_repository_error(error: ExecutionGraphRepositoryError) -> CompositionPlanError {
    match error {
        ExecutionGraphRepositoryError::AlreadyExists | ExecutionGraphRepositoryError::Conflict => {
            CompositionPlanError::ExecutionGraphConflict
        }
        ExecutionGraphRepositoryError::NotFound
        | ExecutionGraphRepositoryError::InvalidRecord
        | ExecutionGraphRepositoryError::Io(_)
        | ExecutionGraphRepositoryError::Json(_) => CompositionPlanError::ExecutionGraphStorage,
    }
}

fn accumulate_usage(total: &mut TokenUsage, next: &TokenUsage) {
    total.input_tokens = total.input_tokens.saturating_add(next.input_tokens);
    total.output_tokens = total.output_tokens.saturating_add(next.output_tokens);
}

fn invalid_staged_output() -> CompositionPlanError {
    CompositionPlanError::InvalidModelOutput(CompositionPlanFailureDetails::reason(
        CompositionPlanFailureReason::DraftGraphInvalid,
    ))
}

fn finalized_graph_schema() -> SchemaRef {
    schema("feature.composition-finalized-graph")
}

fn validation_checkpoint_schema() -> SchemaRef {
    schema("feature.composition-validation-checkpoint")
}

fn commit_checkpoint_schema() -> SchemaRef {
    schema("feature.composition-commit-checkpoint")
}

fn composition_draft_payload_schema() -> SchemaRef {
    schema_version("workspace.composition-draft", 2)
}

#[cfg(test)]
mod tests {
    use ats_game_context::GamePackLoader;
    use ats_kernel::{CompositionProfileId, ItemReferenceSlotId};

    use super::*;

    #[test]
    fn zero_target_binding_rule_does_not_author_an_empty_reference_slot() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let composition_id = CompositionId::parse("character_suite").unwrap();
        let profile_set = pack.composition_profile(&composition_id).unwrap();
        let prototype = profile_set
            .profiles()
            .iter()
            .find(|profile| profile.id().as_str() == "prototype")
            .unwrap();
        let character_node_id = ExecutionNodeId::parse("item.generate:character:000").unwrap();
        let character = StagedItemNode {
            execution_node_id: character_node_id.clone(),
            group_id: "character".into(),
            ordinal: 0,
            item_id: ItemId::parse("zero-target-character").unwrap(),
            item_type: ItemTypeId::parse("character").unwrap(),
            depends_on: Vec::new(),
        };
        let blueprint = StagedCompositionBlueprint {
            schema_version: 1,
            game_pack_id: pack.id().clone(),
            game_pack_sha256: pack.content_sha256().clone(),
            draft_id: CompositionDraftId::parse("zero-target-draft").unwrap(),
            composition_id: composition_id.clone(),
            concept: "Verify an optional zero-count group.".into(),
            profile: ItemCompositionProfile {
                composition_id,
                source: ItemCompositionSource::Preset {
                    profile_id: CompositionProfileId::parse("prototype").unwrap(),
                },
                parameters: prototype.values().clone(),
            },
            root_node_id: character_node_id,
            suite_brief_node_id: None,
            item_nodes: vec![character.clone()],
            binding_rules: vec![CompositionBindingRule {
                source_group_id: "character".into(),
                slot_id: ItemReferenceSlotId::parse("potions").unwrap(),
                target_group_ids: vec!["potions".into()],
                measure: ReferenceBindingMeasure::Bindings,
                quantity_policy: CompositionQuantityPolicy::Constant { value: 1 },
            }],
            guidance: Vec::new(),
            suite_brief_guidance: Vec::new(),
        };

        let bindings = planned_bindings_for_node(&pack, &blueprint, &character, None).unwrap();
        assert!(bindings.is_empty());
    }
}
