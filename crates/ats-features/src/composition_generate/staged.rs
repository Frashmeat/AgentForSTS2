use ats_kernel::{ExecutionNodeId, GamePackId, ItemId};
use ats_runtime::{
    ExecutionCommitIntent, ExecutionFailure, ExecutionGraphRecord, ExecutionGraphRepository,
    ExecutionGraphRepositoryError, ExecutionGraphStatus, ExecutionNodeSpec, ExecutionNodeStatus,
    RunRepository, RunRepositoryError, hash_json,
};

use super::*;
use crate::mod_generate_single::{
    SingleGenerateCompositionProposal, SingleGenerateProposalCheckpoint,
};

const BLUEPRINT_SCHEMA_ID: &str = "feature.composition-generate-blueprint";
const PLAN_CHECKPOINT_SCHEMA_ID: &str = "feature.composition-generate-plan-checkpoint";
const SINGLE_CHECKPOINT_SCHEMA_ID: &str = "feature.composition-generate-single-checkpoint";
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

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedCompositionGenerateBlueprint {
    schema_version: u32,
    game_pack_id: GamePackId,
    game_pack_sha256: Sha256Digest,
    truth_snapshot_id: Sha256Digest,
    request: CompositionGenerateRequest,
    graph_digest: Sha256Digest,
    items: Vec<StagedCompositionGenerateItem>,
    finalize_node_id: ExecutionNodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedCompositionGenerateItem {
    item_id: ItemId,
    definition_hash: Sha256Digest,
    plan_node_id: ExecutionNodeId,
    single_node_id: ExecutionNodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedPlanCheckpoint {
    plan: crate::mod_plan::PlanItem,
    child_run: RunRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedSingleCheckpoint {
    proposal: SingleGenerateProposalCheckpoint,
    child_run: RunRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StagedFinalizeCheckpoint {
    graph_digest: Sha256Digest,
    validation_primitive: ats_kernel::PrimitiveId,
    generated_file_count: u32,
    items: Vec<CompositionItemRunResult>,
}

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
        contribution.validate()?;
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
        let finalize_node_id = ExecutionNodeId::parse("composition.finalize")
            .map_err(|_| CompositionGenerateError::InvalidInput)?;
        let mut staged_items = Vec::with_capacity(resolved.nodes.len());
        let mut specs = Vec::with_capacity(resolved.nodes.len() * 2 + 1);
        let mut previous_single = None;
        for (index, definition) in resolved.nodes.iter().enumerate() {
            let plan_node_id = ExecutionNodeId::parse(format!("item.{index:03}.plan"))
                .map_err(|_| CompositionGenerateError::InvalidInput)?;
            let single_node_id = ExecutionNodeId::parse(format!("item.{index:03}.single"))
                .map_err(|_| CompositionGenerateError::InvalidInput)?;
            let plan_dependencies = previous_single.iter().cloned().collect::<Vec<_>>();
            specs.push(ExecutionNodeSpec {
                node_id: plan_node_id.clone(),
                role_id: "mod.plan".into(),
                depends_on: plan_dependencies,
                request_snapshot_hash: zero_digest()?,
            });
            specs.push(ExecutionNodeSpec {
                node_id: single_node_id.clone(),
                role_id: "mod.generate.single".into(),
                depends_on: vec![plan_node_id.clone()],
                request_snapshot_hash: zero_digest()?,
            });
            staged_items.push(StagedCompositionGenerateItem {
                item_id: definition.definition.item_id.clone(),
                definition_hash: definition.definition_hash.clone(),
                plan_node_id,
                single_node_id: single_node_id.clone(),
            });
            previous_single = Some(single_node_id);
        }
        let finalize_dependencies = previous_single.into_iter().collect::<Vec<_>>();
        if finalize_dependencies.is_empty() {
            return Err(CompositionGenerateError::InvalidInput);
        }
        specs.push(ExecutionNodeSpec {
            node_id: finalize_node_id.clone(),
            role_id: "composition.finalize".into(),
            depends_on: finalize_dependencies,
            request_snapshot_hash: zero_digest()?,
        });
        let blueprint = StagedCompositionGenerateBlueprint {
            schema_version: 1,
            game_pack_id: context.pack.id().clone(),
            game_pack_sha256: context.pack.content_sha256().clone(),
            truth_snapshot_id: context.truth.manifest().snapshot_id().clone(),
            request: blueprint_request,
            graph_digest: resolved.graph_digest.clone(),
            items: staged_items,
            finalize_node_id,
        };
        blueprint.validate(&context, &resolved, &request)?;
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
        blueprint.validate_identity(&context)?;
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

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
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
        blueprint.validate_identity(&context)?;
        let resolved = ResolvedItemGraph::resolve(
            context.pack,
            context.truth,
            context.resource_contributions,
            dependencies.items,
            dependencies.resources,
            request.root.clone(),
            request.draft.clone(),
        )?;
        blueprint.validate(&context, &resolved, &request)?;
        match request.execution.as_ref() {
            Some(CompositionGenerateExecutionRequest::Start { .. })
                if graph.previous_run_id().is_none() => {}
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
                if node_status(&graph, &item.single_node_id)? != ExecutionNodeStatus::Succeeded {
                    let single_request = SingleGenerateRequest {
                        artifact_id: definition.definition.item_id.to_string(),
                        mod_id: request.mod_id.clone(),
                        plan: plan_checkpoint.plan.clone(),
                        definition: definition.clone(),
                    };
                    bind_and_start_node(
                        &mut graph,
                        graphs,
                        &item.single_node_id,
                        run.id(),
                        request_hash::<SingleGenerateFeature, _>(&single_request)?,
                    )?;
                    let mut child_run = running_run::<SingleGenerateFeature, _>(&single_request)?;
                    let proposal = match self
                        .single
                        .propose(
                            SingleProposalDependencies {
                                model: dependencies.model,
                                resources: dependencies.resources,
                            },
                            &child_run,
                            &single_request,
                            &single_context(&context),
                            cancellation,
                        )
                        .await
                    {
                        Ok(proposal) => proposal,
                        Err(error) => {
                            finish_failed_child(&mut child_run, error.run_failure(), cancellation)?;
                            if let Err(storage_error) = persist_child(runs, &child_run) {
                                pause_failed_node(
                                    &mut graph,
                                    graphs,
                                    &item.single_node_id,
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
                            let error = CompositionGenerateError::Single(error);
                            pause_failed_node(
                                &mut graph,
                                graphs,
                                &item.single_node_id,
                                run.id(),
                                &error,
                            )?;
                            return Err(error);
                        }
                    };
                    succeed_child::<SingleGenerateFeature, _>(&mut child_run, &proposal.result)?;
                    let checkpoint = StagedSingleCheckpoint {
                        proposal: proposal.checkpoint(),
                        child_run,
                    };
                    complete_node(
                        &mut graph,
                        graphs,
                        &item.single_node_id,
                        run.id(),
                        VersionedPayload::from_typed(single_checkpoint_schema(), &checkpoint)?,
                    )?;
                    persist_completed_child(
                        runs,
                        &checkpoint.child_run,
                        &mut graph,
                        graphs,
                        run.id(),
                    )?;
                } else {
                    let checkpoint = decode_single_checkpoint(&graph, item, definition)?;
                    persist_completed_child(
                        runs,
                        &checkpoint.child_run,
                        &mut graph,
                        graphs,
                        run.id(),
                    )?;
                }
            }
        }

        let finalize_was_pending = graph.status() == ExecutionGraphStatus::Running
            && node_status(&graph, &blueprint.finalize_node_id)? != ExecutionNodeStatus::Succeeded;
        if finalize_was_pending {
            start_local_node(&mut graph, graphs, &blueprint.finalize_node_id, run.id())?;
        }
        let assembled = (|| {
            let (proposals, item_results) = restore_proposals(
                self,
                dependencies.resources,
                &request,
                &context,
                &resolved,
                &blueprint,
                &graph,
                runs,
            )?;
            let validation_primitive = proposals
                .first()
                .map(|proposal| proposal.result.validation_primitive.clone())
                .ok_or(CompositionGenerateError::InvalidCheckpoint)?;
            if proposals
                .iter()
                .any(|proposal| proposal.result.validation_primitive != validation_primitive)
            {
                return Err(CompositionGenerateError::ValidationPrimitiveMismatch);
            }
            let (generated_writes, generated_artifact_files) =
                consolidate_proposed_files(&proposals)?;
            ats_runtime::validate_project_writes(&generated_writes)?;
            let finalize_checkpoint = StagedFinalizeCheckpoint {
                graph_digest: resolved.graph_digest.clone(),
                validation_primitive,
                generated_file_count: u32::try_from(generated_artifact_files.len())
                    .map_err(|_| CompositionGenerateError::InvalidInput)?,
                items: item_results,
            };
            Ok((
                proposals,
                generated_writes,
                generated_artifact_files,
                finalize_checkpoint,
            ))
        })();
        let (proposals, generated_writes, generated_artifact_files, finalize_checkpoint) =
            match assembled {
                Ok(value) => value,
                Err(error) => {
                    if finalize_was_pending {
                        pause_failed_node(
                            &mut graph,
                            graphs,
                            &blueprint.finalize_node_id,
                            run.id(),
                            &error,
                        )?;
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
                &blueprint.finalize_node_id,
                run.id(),
                VersionedPayload::from_typed(finalize_checkpoint_schema(), &finalize_checkpoint)?,
            )?;
        }
        let persisted_finalize: StagedFinalizeCheckpoint = decode_checkpoint(
            &graph,
            &blueprint.finalize_node_id,
            &finalize_checkpoint_schema(),
        )?;
        if persisted_finalize != finalize_checkpoint {
            return Err(CompositionGenerateError::InvalidCheckpoint);
        }
        if graph.status() == ExecutionGraphStatus::Running {
            let publication = StagedPublicationIntent {
                execution_graph_id: graph.id().clone(),
                artifact_id: request.artifact_id.clone(),
                graph_digest: resolved.graph_digest.clone(),
                generated_file_count: finalize_checkpoint.generated_file_count,
            };
            let canonical = VersionedPayload::from_typed(commit_intent_schema(), &publication)?;
            let payload_sha256 = hash_json(canonical.payload())
                .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
            let digest = validated_content_digest(&graph, &finalize_checkpoint)?;
            let intent = ExecutionCommitIntent::publication(
                request.artifact_id.clone(),
                canonical,
                payload_sha256,
                digest,
            );
            mutate_graph(&mut graph, graphs, |graph| {
                graph.prepare_commit(run.id(), intent, Utc::now())
            })?;
        }
        validate_publication_intent(&graph, &request, &resolved, &finalize_checkpoint)?;

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
                proposals,
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
        proposals: Vec<SingleGenerateCompositionProposal>,
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
        dependencies
            .validator
            .validate(
                ValidationRequest {
                    primitive: finalize.validation_primitive.clone(),
                    project_root: stage_root.clone(),
                    run_id: run.id().clone(),
                },
                cancellation,
            )
            .await?;
        check_cancelled(cancellation)?;

        let build_request = ProjectBuildRequest {
            output_relative_root: Some(request.package.source_relative_root.clone()),
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
                    contributions: context.build_contributions,
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
        let build_run_id = build_run.id().clone();

        let mut package_run = running_run::<ProjectPackageFeature, _>(&request.package)?;
        let prepared_package = match self.package.prepare(
            dependencies.package_writer,
            &package_run,
            &request.package,
            ProjectPackageContext {
                pack: context.pack,
                contributions: context.package_contributions,
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
        let package = prepared_package.result().clone();
        succeed_child::<ProjectPackageFeature, _>(&mut package_run, &package)?;
        if let Err(error) = persist_child(runs, &package_run) {
            prepared_package.rollback()?;
            stage.cleanup()?;
            release_commit_claim(graph, graphs)?;
            return Err(error);
        }
        let package_run_id = package_run.id().clone();

        let mut final_writes = generated_writes;
        final_writes.push(ProjectFileWrite::from_source(
            package.output_relative_path.clone(),
            prepared_package.output_path().to_path_buf(),
        )?);
        ats_runtime::validate_project_writes(&final_writes)?;
        let pending = match dependencies
            .writer
            .apply(context.project_root, run.id(), final_writes)
        {
            Ok(pending) => pending,
            Err(error) => {
                prepared_package.rollback()?;
                stage.cleanup()?;
                release_commit_claim(graph, graphs)?;
                return Err(error.into());
            }
        };
        if let Err(error) = prepared_package.commit() {
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

        let mut child_run_ids = Vec::with_capacity(finalize.items.len() * 2 + 2);
        for item in &finalize.items {
            child_run_ids.push(item.plan_run_id.clone());
            child_run_ids.push(item.generation_run_id.clone());
        }
        child_run_ids.push(build_run_id.clone());
        child_run_ids.push(package_run_id.clone());
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
            package_output_relative_path: package.output_relative_path.clone(),
            package_report: package.report.clone(),
            execution_graph_id: Some(graph.id().clone()),
        };
        let artifact_request = composition_artifact_request(
            request,
            context,
            run,
            resolved,
            &proposals,
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
        context: &CompositionGenerateContext<'_>,
    ) -> Result<(), CompositionGenerateError> {
        if self.schema_version != 1
            || self.game_pack_id != *context.pack.id()
            || self.game_pack_sha256 != *context.pack.content_sha256()
            || self.truth_snapshot_id != *context.truth.manifest().snapshot_id()
            || self.request.execution.is_some()
            || self.items.is_empty()
            || self.items.len() > 128
        {
            Err(CompositionGenerateError::InvalidCheckpoint)
        } else {
            Ok(())
        }
    }

    fn validate(
        &self,
        context: &CompositionGenerateContext<'_>,
        resolved: &ResolvedItemGraph,
        request: &CompositionGenerateRequest,
    ) -> Result<(), CompositionGenerateError> {
        self.validate_identity(context)?;
        let mut canonical_request = request.clone();
        canonical_request.execution = None;
        if canonical_request != self.request
            || self.graph_digest != resolved.graph_digest
            || self.items.len() != resolved.nodes.len()
            || self
                .items
                .iter()
                .zip(&resolved.nodes)
                .any(|(item, definition)| {
                    item.item_id != definition.definition.item_id
                        || item.definition_hash != definition.definition_hash
                })
        {
            return Err(CompositionGenerateError::InvalidCheckpoint);
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn restore_proposals<R, RR>(
    service: &CompositionGenerateService<'_>,
    resources: &R,
    request: &CompositionGenerateRequest,
    context: &CompositionGenerateContext<'_>,
    resolved: &ResolvedItemGraph,
    blueprint: &StagedCompositionGenerateBlueprint,
    graph: &ExecutionGraphRecord,
    runs: &RR,
) -> Result<
    (
        Vec<SingleGenerateCompositionProposal>,
        Vec<CompositionItemRunResult>,
    ),
    CompositionGenerateError,
>
where
    R: ResourceRepository + ?Sized,
    RR: RunRepository + ?Sized,
{
    let mut proposals = Vec::with_capacity(blueprint.items.len());
    let mut results = Vec::with_capacity(blueprint.items.len());
    for (item, definition) in blueprint.items.iter().zip(&resolved.nodes) {
        let plan = decode_plan_checkpoint(graph, item, definition)?;
        let single = decode_single_checkpoint(graph, item, definition)?;
        persist_child(runs, &plan.child_run)?;
        persist_child(runs, &single.child_run)?;
        let single_request = SingleGenerateRequest {
            artifact_id: definition.definition.item_id.to_string(),
            mod_id: request.mod_id.clone(),
            plan: plan.plan,
            definition: definition.clone(),
        };
        let proposal = service.single.restore_composition_proposal(
            resources,
            &single_request,
            &single_context(context),
            &single.proposal,
        )?;
        results.push(CompositionItemRunResult {
            item_id: definition.definition.item_id.clone(),
            definition_hash: definition.definition_hash.clone(),
            plan_run_id: plan.child_run.id().clone(),
            generation_run_id: single.child_run.id().clone(),
            generation: proposal.result.clone(),
        });
        proposals.push(proposal);
    }
    Ok((proposals, results))
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

fn decode_single_checkpoint(
    graph: &ExecutionGraphRecord,
    item: &StagedCompositionGenerateItem,
    definition: &StoredItemDefinition,
) -> Result<StagedSingleCheckpoint, CompositionGenerateError> {
    let checkpoint: StagedSingleCheckpoint =
        decode_checkpoint(graph, &item.single_node_id, &single_checkpoint_schema())?;
    let result = checkpoint
        .child_run
        .result()
        .ok_or(CompositionGenerateError::InvalidCheckpoint)?
        .decode::<SingleGenerateResult>(&SingleGenerateFeature::result_schema())
        .map_err(|_| CompositionGenerateError::InvalidCheckpoint)?;
    if checkpoint.child_run.feature_id() != &SingleGenerateFeature::id()
        || checkpoint.child_run.status() != RunStatus::Succeeded
        || result != checkpoint.proposal.result
        || checkpoint.proposal.provenance.definition_hash != definition.definition_hash
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
                .checkpoint
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

fn single_context<'a>(context: &'a CompositionGenerateContext<'a>) -> SingleGenerateContext<'a> {
    SingleGenerateContext {
        pack: context.pack,
        contributions: context.single_contributions,
        resource_contributions: context.resource_contributions,
        truth: context.truth,
        project_root: context.project_root,
        project_context: context.project_context,
        custom_instructions: context.custom_instructions,
        model: context.model.clone(),
    }
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
        .and_then(|node| node.checkpoint.as_ref())
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
    schema(BLUEPRINT_SCHEMA_ID)
}

fn plan_checkpoint_schema() -> SchemaRef {
    schema(PLAN_CHECKPOINT_SCHEMA_ID)
}

fn single_checkpoint_schema() -> SchemaRef {
    schema(SINGLE_CHECKPOINT_SCHEMA_ID)
}

fn finalize_checkpoint_schema() -> SchemaRef {
    schema(FINALIZE_CHECKPOINT_SCHEMA_ID)
}

fn commit_intent_schema() -> SchemaRef {
    schema(COMMIT_INTENT_SCHEMA_ID)
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
        let next_node = ExecutionNodeId::parse("item.000.single").unwrap();
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
                    role_id: "mod.generate.single".into(),
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
}
