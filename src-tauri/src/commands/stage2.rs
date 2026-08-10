use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ats_adapters::{
    CompositionDraftStoreError, FileCompositionDraftRepository, FileResourceRepository,
    FileTruthSnapshotRepository, ItemStoreError, Sts2TruthImporter,
};
use ats_features::composition::{
    CompositionConfirmation, CompositionConfirmationError, CompositionConfirmationService,
    CompositionGraphError, ResolvedItemGraph,
};
use ats_features::composition_generate::{CompositionGenerateFeature, CompositionGenerateRequest};
use ats_features::composition_plan::{
    CompositionPlanFeature, CompositionPlanRequest, CompositionRetryNodeFeature,
    CompositionRetryNodeRequest,
};
use ats_features::item_definition::{ItemDefinitionValidationMode, ItemDefinitionValidator};
use ats_features::mod_generate_batch::{
    BatchGenerateFeature, BatchGenerateRequest, validate_batch_generation_input,
};
use ats_features::mod_generate_complex::{ComplexGenerateFeature, ComplexGenerateRequest};
use ats_features::mod_generate_single::{
    SingleGenerateError, SingleGenerateFeature, SingleGenerateRequest,
    validate_definition_resources, validate_single_generation_readiness,
};
use ats_features::mod_plan::{ModPlanFeature, ModPlanRequest};
use ats_features::resource_prepare::{
    ResourceCatalog, ResourcePrepareContext, ResourcePrepareError, ResourcePrepareFeature,
    ResourcePrepareResult, ResourcePrepareService, ResourcePreview,
};
use ats_features::{FeatureContract, FeatureSpec, built_in_feature_contracts};
use ats_game_context::{ItemCapabilityCatalog, TruthSnapshotRepository};
use ats_kernel::{
    CompositionDraftId, ExecutionGraphId, ExecutionNodeId, FailureCode, FeatureId, ItemId,
    ItemTypeId, Sha256Digest,
};
use ats_runtime::{
    CancellationReason, CancellationToken, ExecutionGraphRecord, ExecutionGraphRepository,
    ExecutionGraphStatus, ExecutionNodeStatus, RunId, RunRecord, RunRepository, RunRepositoryError,
    RunStatus, RunSummary, VersionedPayload,
};
use ats_workspace::{
    CompositionDraft, CompositionDraftNode, CompositionDraftRepository, ItemDefinition,
    ItemRepository, ResourceAsset, StoredItemDefinition,
};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppConfig;
use crate::commands::failure::{CommandFailure, CommandResult};
use crate::composition::Stage2Composition;
use crate::project_session::{ActiveProject, SubmitError};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitFeatureRequest {
    pub feature_id: FeatureId,
    pub request: VersionedPayload,
    #[serde(default)]
    pub source_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruthStatus {
    pub ready: bool,
    pub snapshot_id: Option<Sha256Digest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionGraphView {
    pub execution_graph_id: ExecutionGraphId,
    pub revision: u64,
    pub status: ExecutionGraphStatus,
    pub active_run_id: Option<RunId>,
    pub previous_run_id: Option<RunId>,
    pub completed_nodes: u32,
    pub total_nodes: u32,
    pub current_node_id: Option<ExecutionNodeId>,
    pub current_role_id: Option<String>,
    pub failure_code: Option<FailureCode>,
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_cancel: bool,
}

#[tauri::command]
pub fn get_feature_catalog() -> Vec<FeatureContract> {
    built_in_feature_contracts()
}

#[tauri::command]
pub async fn submit_feature(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
    config: State<'_, Arc<AppConfig>>,
    submission: SubmitFeatureRequest,
) -> CommandResult<RunId> {
    let session = current_session(&active, "run.submit")?;
    let drafts = session.composition_draft_repository();
    ensure_submission_ready(
        composition.inner(),
        session.item_repository().as_ref(),
        drafts.as_ref(),
        session.resource_repository().as_ref(),
        &submission,
    )?;
    let candidate_run_id = RunId::new();
    let (run, execution_graph) = if submission.feature_id == CompositionPlanFeature::id() {
        let request = submission
            .request
            .decode::<CompositionPlanRequest>(&CompositionPlanFeature::request_schema())
            .map_err(|_| CommandFailure::composition_invalid("run.submit"))?;
        if request.execution.is_some() {
            return Err(CommandFailure::composition_invalid("run.submit"));
        }
        let staged = composition
            .prepare_composition_plan_start(request, candidate_run_id.clone())
            .map_err(map_plan_prepare_failure)?;
        let request =
            VersionedPayload::from_typed(CompositionPlanFeature::request_schema(), &staged.request)
                .map_err(|_| CommandFailure::composition_invalid("run.submit"))?;
        (
            RunRecord::new_with_id(candidate_run_id, CompositionPlanFeature::id(), request),
            Some(staged.graph),
        )
    } else {
        (
            RunRecord::new_with_id(candidate_run_id, submission.feature_id, submission.request),
            None,
        )
    };
    let root = session.path().to_path_buf();
    let meta = session.meta().clone();
    let composition = Arc::clone(composition.inner());
    let config = Arc::clone(config.inner());
    let resources = session.resource_repository();
    let items = session.item_repository();
    let graphs = session.execution_graph_repository();
    let source_path = submission.source_path.map(PathBuf::from);
    let worker = move |run: RunRecord,
                       cancellation: CancellationToken,
                       repository: Arc<dyn ats_runtime::RunRepository>| async move {
        composition
            .execute(
                &config,
                &root,
                &meta,
                run,
                repository.as_ref(),
                items.as_ref(),
                drafts.as_ref(),
                graphs.as_ref(),
                resources.as_ref(),
                source_path,
                &cancellation,
            )
            .await
    };
    let submitted: Result<RunId, SubmitError> = match execution_graph {
        Some(graph) => session.submit_claimed(run, graph, worker).await,
        None => session.submit(run, worker).await,
    };
    submitted.map_err(|error| match error {
        SubmitError::Closing => CommandFailure::project_closing("run.submit"),
        SubmitError::Repository => CommandFailure::storage("run.submit"),
    })
}

#[tauri::command]
pub fn list_execution_graphs(
    active: State<'_, ActiveProject>,
) -> CommandResult<Vec<ExecutionGraphView>> {
    let session = current_session(&active, "execution.list")?;
    let graphs = session
        .execution_graph_repository()
        .list()
        .map_err(|_| CommandFailure::storage("execution.list"))?;
    let runs = session.repository();
    graphs
        .iter()
        .map(|graph| {
            can_reconcile_succeeded_graph(graph, runs.as_ref())
                .map(|can_reconcile| execution_graph_view(graph, can_reconcile))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CommandFailure::storage("execution.list"))
}

#[tauri::command]
pub fn get_execution_graph(
    active: State<'_, ActiveProject>,
    execution_graph_id: String,
) -> CommandResult<ExecutionGraphView> {
    let id = ExecutionGraphId::parse(execution_graph_id)
        .map_err(|_| CommandFailure::composition_invalid("execution.get"))?;
    let session = current_session(&active, "execution.get")?;
    let graph = session
        .execution_graph_repository()
        .get(&id)
        .map_err(|_| CommandFailure::storage("execution.get"))?;
    let can_reconcile_succeeded =
        can_reconcile_succeeded_graph(&graph, session.repository().as_ref())
            .map_err(|_| CommandFailure::storage("execution.get"))?;
    Ok(execution_graph_view(&graph, can_reconcile_succeeded))
}

#[tauri::command]
pub async fn pause_execution_graph(
    active: State<'_, ActiveProject>,
    execution_graph_id: String,
) -> CommandResult<bool> {
    let id = ExecutionGraphId::parse(execution_graph_id)
        .map_err(|_| CommandFailure::composition_invalid("execution.pause"))?;
    let session = current_session(&active, "execution.pause")?;
    let graph = session
        .execution_graph_repository()
        .get(&id)
        .map_err(|_| CommandFailure::storage("execution.pause"))?;
    let run_id = graph
        .active_run_id()
        .ok_or_else(|| CommandFailure::composition_conflict("execution.pause"))?;
    Ok(session.cancel_run(run_id, CancellationReason::Pause).await)
}

#[tauri::command]
pub async fn cancel_execution_graph(
    active: State<'_, ActiveProject>,
    execution_graph_id: String,
) -> CommandResult<bool> {
    let id = ExecutionGraphId::parse(execution_graph_id)
        .map_err(|_| CommandFailure::composition_invalid("execution.cancel"))?;
    let session = current_session(&active, "execution.cancel")?;
    let repository = session.execution_graph_repository();
    let mut graph = repository
        .get(&id)
        .map_err(|_| CommandFailure::storage("execution.cancel"))?;
    if let Some(run_id) = graph.active_run_id() {
        return Ok(session.cancel_run(run_id, CancellationReason::User).await);
    }
    let revision = graph.revision();
    graph
        .cancel(None, chrono::Utc::now())
        .map_err(|_| CommandFailure::composition_conflict("execution.cancel"))?;
    repository
        .compare_and_set(revision, &graph)
        .map_err(|_| CommandFailure::composition_conflict("execution.cancel"))?;
    Ok(true)
}

#[tauri::command]
pub async fn resume_composition_plan(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
    config: State<'_, Arc<AppConfig>>,
    execution_graph_id: String,
    expected_revision: u64,
) -> CommandResult<RunId> {
    let id = ExecutionGraphId::parse(execution_graph_id)
        .map_err(|_| CommandFailure::composition_invalid("execution.resume"))?;
    let session = current_session(&active, "execution.resume")?;
    let graphs = session.execution_graph_repository();
    let graph = graphs
        .get(&id)
        .map_err(|_| CommandFailure::storage("execution.resume"))?;
    let run_id = RunId::new();
    let staged = composition
        .prepare_composition_plan_resume(graph, expected_revision, run_id.clone())
        .map_err(map_plan_prepare_failure)?;
    let request =
        VersionedPayload::from_typed(CompositionPlanFeature::request_schema(), &staged.request)
            .map_err(|_| CommandFailure::composition_invalid("execution.resume"))?;
    let run = RunRecord::new_with_id(run_id, CompositionPlanFeature::id(), request);
    let root = session.path().to_path_buf();
    let meta = session.meta().clone();
    let composition = Arc::clone(composition.inner());
    let config = Arc::clone(config.inner());
    let resources = session.resource_repository();
    let items = session.item_repository();
    let drafts = session.composition_draft_repository();
    let worker_graphs = Arc::clone(&graphs);
    let already_succeeded = staged.graph.status() == ExecutionGraphStatus::Succeeded;
    let submitted = if already_succeeded {
        session
            .submit(run, move |run, cancellation, repository| async move {
                composition
                    .execute(
                        &config,
                        &root,
                        &meta,
                        run,
                        repository.as_ref(),
                        items.as_ref(),
                        drafts.as_ref(),
                        worker_graphs.as_ref(),
                        resources.as_ref(),
                        None,
                        &cancellation,
                    )
                    .await
            })
            .await
    } else {
        session
            .submit_resumed(
                run,
                expected_revision,
                staged.graph,
                move |run, cancellation, repository| async move {
                    composition
                        .execute(
                            &config,
                            &root,
                            &meta,
                            run,
                            repository.as_ref(),
                            items.as_ref(),
                            drafts.as_ref(),
                            worker_graphs.as_ref(),
                            resources.as_ref(),
                            None,
                            &cancellation,
                        )
                        .await
                },
            )
            .await
    };
    submitted.map_err(|error| match error {
        SubmitError::Closing => CommandFailure::project_closing("execution.resume"),
        SubmitError::Repository => CommandFailure::composition_conflict("execution.resume"),
    })
}

#[tauri::command]
pub fn get_item_capabilities(
    composition: State<'_, Arc<Stage2Composition>>,
) -> CommandResult<ItemCapabilityCatalog> {
    item_capabilities(composition.inner())
}

#[tauri::command]
pub fn list_item_definitions(
    active: State<'_, ActiveProject>,
) -> CommandResult<Vec<StoredItemDefinition>> {
    let session = current_session(&active, "item.list")?;
    session
        .item_repository()
        .list_current()
        .map_err(|error| map_item_store_error(error, "item.list"))
}

#[tauri::command]
pub fn get_item_definition(
    active: State<'_, ActiveProject>,
    item_id: String,
    definition_hash: Option<String>,
) -> CommandResult<StoredItemDefinition> {
    let session = current_session(&active, "item.get")?;
    let item_id = ItemId::parse(item_id).map_err(|_| CommandFailure::item_invalid("item.get"))?;
    let repository = session.item_repository();
    match definition_hash {
        Some(value) => {
            let hash =
                Sha256Digest::parse(value).map_err(|_| CommandFailure::item_invalid("item.get"))?;
            repository.load_version(&item_id, &hash)
        }
        None => repository.load_current(&item_id),
    }
    .map_err(|error| map_item_store_error(error, "item.get"))
}

#[tauri::command]
pub fn save_item_definition(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
    definition: ItemDefinition,
) -> CommandResult<StoredItemDefinition> {
    let session = current_session(&active, "item.save")?;
    ItemDefinitionValidator::validate(
        composition.pack(),
        &definition,
        ItemDefinitionValidationMode::Draft,
    )
    .map_err(|_| CommandFailure::item_invalid("item.save"))?;
    let capabilities = item_capabilities(composition.inner())?;
    let ready = capabilities
        .item_types
        .iter()
        .any(|item| item.descriptor.id() == &definition.item_type && item.ready);
    if !ready {
        return Err(CommandFailure::truth_evidence_missing("item.save"));
    }
    session
        .item_repository()
        .save(&definition)
        .map_err(|error| map_item_store_error(error, "item.save"))
}

#[tauri::command]
pub fn list_composition_drafts(
    active: State<'_, ActiveProject>,
) -> CommandResult<Vec<CompositionDraft>> {
    current_session(&active, "composition.draft.list")?
        .composition_draft_repository()
        .list()
        .map_err(|error| map_draft_store_error(error, "composition.draft.list"))
}

#[tauri::command]
pub fn get_composition_draft(
    active: State<'_, ActiveProject>,
    draft_id: String,
) -> CommandResult<CompositionDraft> {
    let id = CompositionDraftId::parse(draft_id)
        .map_err(|_| CommandFailure::composition_invalid("composition.draft.get"))?;
    current_session(&active, "composition.draft.get")?
        .composition_draft_repository()
        .load(&id)
        .map_err(|error| map_draft_store_error(error, "composition.draft.get"))
}

#[tauri::command]
pub fn update_composition_draft(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
    draft_id: String,
    expected_revision: u64,
    nodes: BTreeMap<ItemId, CompositionDraftNode>,
) -> CommandResult<CompositionDraft> {
    let id = CompositionDraftId::parse(draft_id)
        .map_err(|_| CommandFailure::composition_invalid("composition.draft.update"))?;
    let session = current_session(&active, "composition.draft.update")?;
    let repository = session.composition_draft_repository();
    let current = repository
        .load(&id)
        .map_err(|error| map_draft_store_error(error, "composition.draft.update"))?;
    if current.revision != expected_revision
        || current.game_pack_id != *composition.pack().id()
        || current.game_pack_sha256 != *composition.pack().content_sha256()
    {
        return Err(CommandFailure::composition_conflict(
            "composition.draft.update",
        ));
    }
    for node in nodes.values() {
        ItemDefinitionValidator::validate(
            composition.pack(),
            &node.definition,
            ItemDefinitionValidationMode::Draft,
        )
        .map_err(|_| CommandFailure::composition_invalid("composition.draft.update"))?;
    }
    let next = current
        .revised(nodes, chrono::Utc::now())
        .map_err(|_| CommandFailure::composition_invalid("composition.draft.update"))?;
    repository
        .compare_and_set(expected_revision, &next)
        .map_err(|error| map_draft_store_error(error, "composition.draft.update"))?;
    Ok(next)
}

#[tauri::command]
pub fn delete_composition_draft(
    active: State<'_, ActiveProject>,
    draft_id: String,
    expected_revision: u64,
) -> CommandResult<()> {
    let id = CompositionDraftId::parse(draft_id)
        .map_err(|_| CommandFailure::composition_invalid("composition.draft.delete"))?;
    current_session(&active, "composition.draft.delete")?
        .composition_draft_repository()
        .delete(&id, expected_revision)
        .map_err(|error| map_draft_store_error(error, "composition.draft.delete"))
}

#[tauri::command]
pub fn confirm_composition_draft(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
    draft_id: String,
    expected_revision: u64,
    selected_item_ids: Vec<String>,
) -> CommandResult<CompositionConfirmation> {
    let id = CompositionDraftId::parse(draft_id)
        .map_err(|_| CommandFailure::composition_invalid("composition.draft.confirm"))?;
    let selected = selected_item_ids
        .into_iter()
        .map(ItemId::parse)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| CommandFailure::composition_invalid("composition.draft.confirm"))?;
    let session = current_session(&active, "composition.draft.confirm")?;
    let draft = session
        .composition_draft_repository()
        .load(&id)
        .map_err(|error| map_draft_store_error(error, "composition.draft.confirm"))?;
    if draft.revision != expected_revision {
        return Err(CommandFailure::composition_conflict(
            "composition.draft.confirm",
        ));
    }
    CompositionConfirmationService::confirm(
        composition.pack(),
        &draft,
        &selected,
        session.item_repository().as_ref(),
    )
    .map_err(map_confirmation_error)
}

#[tauri::command]
pub fn get_resource_catalog(
    composition: State<'_, Arc<Stage2Composition>>,
) -> CommandResult<ResourceCatalog> {
    let contributions = resource_contributions(composition.inner(), "resource.catalog")?;
    ResourcePrepareService
        .catalog(ResourcePrepareContext {
            pack: composition.pack(),
            contributions: &contributions,
        })
        .map_err(|error| map_resource_error(error, "resource.catalog"))
}

#[tauri::command]
pub fn list_resource_assets(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
) -> CommandResult<Vec<ResourceAsset>> {
    let session = current_session(&active, "resource.list")?;
    let contributions = resource_contributions(composition.inner(), "resource.list")?;
    ResourcePrepareService
        .list(
            session.resource_repository().as_ref(),
            ResourcePrepareContext {
                pack: composition.pack(),
                contributions: &contributions,
            },
        )
        .map_err(|error| map_resource_error(error, "resource.list"))
}

#[tauri::command]
pub fn get_resource_preview(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
    resource_id: String,
    version: String,
) -> CommandResult<ResourcePreview> {
    let session = current_session(&active, "resource.preview")?;
    let resource_id = ats_kernel::ResourceId::parse(resource_id)
        .map_err(|_| CommandFailure::resource_invalid("resource.preview"))?;
    let version = Sha256Digest::parse(version)
        .map_err(|_| CommandFailure::resource_invalid("resource.preview"))?;
    let contributions = resource_contributions(composition.inner(), "resource.preview")?;
    ResourcePrepareService
        .preview(
            session.resource_repository().as_ref(),
            &resource_id,
            &version,
            ResourcePrepareContext {
                pack: composition.pack(),
                contributions: &contributions,
            },
        )
        .map_err(|error| map_resource_error(error, "resource.preview"))
}

#[tauri::command]
pub fn select_resource(
    active: State<'_, ActiveProject>,
    composition: State<'_, Arc<Stage2Composition>>,
    resource_id: String,
    version: String,
) -> CommandResult<ResourcePrepareResult> {
    let session = current_session(&active, "resource.select")?;
    let resource_id = ats_kernel::ResourceId::parse(resource_id)
        .map_err(|_| CommandFailure::resource_invalid("resource.select"))?;
    let version = Sha256Digest::parse(version)
        .map_err(|_| CommandFailure::resource_invalid("resource.select"))?;
    let contributions = resource_contributions(composition.inner(), "resource.select")?;
    ResourcePrepareService
        .select(
            session.resource_repository().as_ref(),
            &resource_id,
            &version,
            ResourcePrepareContext {
                pack: composition.pack(),
                contributions: &contributions,
            },
        )
        .map_err(|error| map_resource_error(error, "resource.select"))
}

#[tauri::command]
pub fn get_run(active: State<'_, ActiveProject>, run_id: String) -> CommandResult<RunRecord> {
    let session = current_session(&active, "run.get")?;
    let id = RunId::parse(run_id).map_err(|_| CommandFailure::invalid_input("run.get"))?;
    session
        .repository()
        .get(&id)
        .map_err(|_| CommandFailure::storage("run.get"))
}

#[tauri::command]
pub fn list_runs(active: State<'_, ActiveProject>) -> CommandResult<Vec<RunSummary>> {
    current_session(&active, "run.list")?
        .repository()
        .list()
        .map_err(|_| CommandFailure::storage("run.list"))
}

#[tauri::command]
pub async fn cancel_run(active: State<'_, ActiveProject>, run_id: String) -> CommandResult<bool> {
    let session = current_session(&active, "run.cancel")?;
    let id = RunId::parse(run_id).map_err(|_| CommandFailure::invalid_input("run.cancel"))?;
    Ok(session.cancel_run(&id, CancellationReason::User).await)
}

#[tauri::command]
pub fn get_truth_status(
    composition: State<'_, Arc<Stage2Composition>>,
) -> CommandResult<TruthStatus> {
    let repository = FileTruthSnapshotRepository::new(composition.runtime_root().to_path_buf());
    let snapshot = repository
        .open_current(composition.pack())
        .map_err(|_| CommandFailure::truth_missing("truth.status"))?;
    Ok(TruthStatus {
        ready: snapshot.is_some(),
        snapshot_id: snapshot.map(|value| value.manifest().snapshot_id().clone()),
    })
}

#[tauri::command]
pub async fn import_truth(
    composition: State<'_, Arc<Stage2Composition>>,
) -> CommandResult<TruthStatus> {
    let composition = Arc::clone(composition.inner());
    tauri::async_runtime::spawn_blocking(move || {
        Sts2TruthImporter::import_current(
            composition.runtime_root(),
            composition.pack(),
            &CancellationToken::new(),
        )
        .map_err(|_| CommandFailure::truth_missing("truth.import"))?;
        let repository = FileTruthSnapshotRepository::new(composition.runtime_root().to_path_buf());
        let snapshot = repository
            .open_current(composition.pack())
            .map_err(|_| CommandFailure::truth_missing("truth.import"))?
            .ok_or_else(|| CommandFailure::truth_missing("truth.import"))?;
        Ok(TruthStatus {
            ready: true,
            snapshot_id: Some(snapshot.manifest().snapshot_id().clone()),
        })
    })
    .await
    .map_err(|_| CommandFailure::unclassified("truth.import_join"))?
}

fn current_session(
    active: &State<'_, ActiveProject>,
    stage: &str,
) -> CommandResult<Arc<crate::project_session::ProjectSession>> {
    active
        .current()
        .map_err(|_| CommandFailure::unclassified(stage))?
        .ok_or_else(|| CommandFailure::project_not_open(stage))
}

fn item_capabilities(composition: &Stage2Composition) -> CommandResult<ItemCapabilityCatalog> {
    let repository = FileTruthSnapshotRepository::new(composition.runtime_root().to_path_buf());
    let snapshot = repository
        .open_current(composition.pack())
        .map_err(|_| CommandFailure::truth_missing("item.capabilities"))?;
    ItemCapabilityCatalog::evaluate(composition.pack(), snapshot.as_ref())
        .map_err(|_| CommandFailure::truth_missing("item.capabilities"))
}

fn ensure_submission_ready(
    composition: &Stage2Composition,
    items: &ats_adapters::FileItemRepository,
    drafts: &FileCompositionDraftRepository,
    resources: &FileResourceRepository,
    submission: &SubmitFeatureRequest,
) -> CommandResult<()> {
    let requested = requested_item_types(composition, drafts, submission)?;
    if requested.is_empty() {
        return Ok(());
    }
    let capabilities = item_capabilities(composition)?;
    for requested in requested {
        let capability = capabilities
            .item_types
            .iter()
            .find(|item| item.descriptor.id() == &requested)
            .ok_or_else(|| CommandFailure::item_invalid("run.submit.readiness"))?;
        if !capability.ready {
            return Err(CommandFailure::truth_evidence_missing(
                "run.submit.readiness",
            ));
        }
    }
    validate_generation_submission(composition, items, resources, submission)?;
    Ok(())
}

fn validate_generation_submission(
    composition: &Stage2Composition,
    items: &ats_adapters::FileItemRepository,
    resources: &FileResourceRepository,
    submission: &SubmitFeatureRequest,
) -> CommandResult<()> {
    let validate_single = |request: &SingleGenerateRequest| {
        let contributions = resource_contributions(composition, "run.submit.resources")?;
        validate_single_generation_readiness(composition.pack(), &contributions, resources, request)
            .map_err(map_generation_readiness)
    };
    match submission.feature_id.as_str() {
        "composition.generate" => {
            let request = submission
                .request
                .decode::<CompositionGenerateRequest>(&CompositionGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::composition_invalid("run.submit.readiness"))?;
            let truth = composition
                .current_truth()
                .map_err(|_| CommandFailure::truth_missing("run.submit.readiness"))?;
            let contributions = resource_contributions(composition, "run.submit.resources")?;
            ResolvedItemGraph::resolve(
                composition.pack(),
                &truth,
                &contributions,
                items,
                resources,
                request.root,
                request.draft,
            )
            .map(|_| ())
            .map_err(map_composition_graph)
        }
        "mod.generate.single" => {
            let request = submission
                .request
                .decode::<SingleGenerateRequest>(&SingleGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            validate_single(&request)
        }
        "mod.generate.batch" => {
            let request = submission
                .request
                .decode::<BatchGenerateRequest>(&BatchGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            validate_batch_generation_input(&request)
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            let contributions = resource_contributions(composition, "run.submit.resources")?;
            for item in &request.items {
                validate_definition_resources(
                    composition.pack(),
                    &contributions,
                    resources,
                    &item.definition,
                )
                .map_err(map_generation_readiness)?;
            }
            Ok(())
        }
        "mod.generate.complex" => {
            let request = submission
                .request
                .decode::<ComplexGenerateRequest>(&ComplexGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            validate_batch_generation_input(&request.batch)
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            if request.package.mod_id != request.batch.mod_id {
                return Err(CommandFailure::item_invalid("run.submit.readiness"));
            }
            let contributions = resource_contributions(composition, "run.submit.resources")?;
            for item in &request.batch.items {
                validate_definition_resources(
                    composition.pack(),
                    &contributions,
                    resources,
                    &item.definition,
                )
                .map_err(map_generation_readiness)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn requested_item_types(
    composition: &Stage2Composition,
    drafts: &FileCompositionDraftRepository,
    submission: &SubmitFeatureRequest,
) -> CommandResult<Vec<ItemTypeId>> {
    let all = || composition.pack().item_types().keys().cloned().collect();
    let parse = |value: &str| {
        ItemTypeId::parse(value).map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))
    };
    match submission.feature_id.as_str() {
        "composition.generate" => {
            let request = submission
                .request
                .decode::<CompositionGenerateRequest>(&CompositionGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::composition_invalid("run.submit.readiness"))?;
            Ok(vec![request.root.definition.item_type])
        }
        "composition.plan" => {
            let request = submission
                .request
                .decode::<CompositionPlanRequest>(&CompositionPlanFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            composition
                .pack()
                .composition_profile(&request.composition_id)
                .map(|profile| vec![profile.root_item_type().clone()])
                .ok_or_else(|| CommandFailure::item_invalid("run.submit.readiness"))
        }
        "composition.retry-node" => {
            let request = submission
                .request
                .decode::<CompositionRetryNodeRequest>(
                    &CompositionRetryNodeFeature::request_schema(),
                )
                .map_err(|_| CommandFailure::composition_invalid("run.submit.readiness"))?;
            let draft = drafts
                .load(&request.draft_id)
                .map_err(|error| map_draft_store_error(error, "run.submit.readiness"))?;
            if draft.revision != request.expected_revision {
                return Err(CommandFailure::composition_conflict("run.submit.readiness"));
            }
            draft
                .nodes
                .get(&request.item_id)
                .map(|node| vec![node.definition.item_type.clone()])
                .ok_or_else(|| CommandFailure::composition_invalid("run.submit.readiness"))
        }
        "mod.plan" => {
            let request = submission
                .request
                .decode::<ModPlanRequest>(&ModPlanFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            request
                .item_type
                .as_deref()
                .map(parse)
                .transpose()
                .map(|value| value.map_or_else(all, |item_type| vec![item_type]))
        }
        "mod.generate.single" => {
            let request = submission
                .request
                .decode::<SingleGenerateRequest>(&SingleGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            Ok(vec![request.definition.definition.item_type])
        }
        "mod.generate.batch" => {
            let request = submission
                .request
                .decode::<BatchGenerateRequest>(&BatchGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            request
                .items
                .iter()
                .map(|item| Ok(item.definition.definition.item_type.clone()))
                .collect()
        }
        "mod.generate.complex" => {
            let request = submission
                .request
                .decode::<ComplexGenerateRequest>(&ComplexGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            Ok(request
                .batch
                .items
                .into_iter()
                .map(|item| item.definition.definition.item_type)
                .collect())
        }
        _ => Ok(Vec::new()),
    }
}

fn map_composition_graph(error: CompositionGraphError) -> CommandFailure {
    match error {
        CompositionGraphError::Storage => {
            CommandFailure::composition_storage("run.submit.readiness")
        }
        CompositionGraphError::ItemMissing => {
            CommandFailure::composition_not_found("run.submit.readiness")
        }
        CompositionGraphError::Readiness => CommandFailure::item_invalid("run.submit.readiness"),
        _ => CommandFailure::composition_invalid("run.submit.readiness"),
    }
}

fn map_item_store_error(error: ItemStoreError, stage: &str) -> CommandFailure {
    match error {
        ItemStoreError::NotFound => CommandFailure::item_not_found(stage),
        ItemStoreError::Contract(_)
        | ItemStoreError::TypeConflict
        | ItemStoreError::PathInvalid
        | ItemStoreError::Json(_) => CommandFailure::item_invalid(stage),
        ItemStoreError::Conflict
        | ItemStoreError::Io { .. }
        | ItemStoreError::LockUnavailable
        | ItemStoreError::TransactionInvalid => CommandFailure::item_storage(stage),
    }
}

fn map_draft_store_error(error: CompositionDraftStoreError, stage: &str) -> CommandFailure {
    match error {
        CompositionDraftStoreError::NotFound => CommandFailure::composition_not_found(stage),
        CompositionDraftStoreError::Conflict => CommandFailure::composition_conflict(stage),
        CompositionDraftStoreError::Contract(_)
        | CompositionDraftStoreError::PathInvalid
        | CompositionDraftStoreError::Json(_) => CommandFailure::composition_invalid(stage),
        CompositionDraftStoreError::Io { .. } | CompositionDraftStoreError::LockUnavailable => {
            CommandFailure::composition_storage(stage)
        }
    }
}

fn map_confirmation_error(error: CompositionConfirmationError) -> CommandFailure {
    match error {
        CompositionConfirmationError::NotReady
        | CompositionConfirmationError::Graph(CompositionGraphError::Readiness) => {
            CommandFailure::composition_not_ready("composition.draft.confirm")
        }
        CompositionConfirmationError::Conflict => {
            CommandFailure::composition_conflict("composition.draft.confirm")
        }
        CompositionConfirmationError::Storage => {
            CommandFailure::composition_storage("composition.draft.confirm")
        }
        _ => CommandFailure::composition_invalid("composition.draft.confirm"),
    }
}

fn resource_contributions(
    composition: &Stage2Composition,
    stage: &str,
) -> CommandResult<ats_game_context::VerifiedContributionSet> {
    composition
        .resolve(
            &ResourcePrepareFeature::id(),
            &[ResourcePrepareFeature::contribution_requirement()],
        )
        .map_err(|_| CommandFailure::pack_invalid(stage))
}

fn map_resource_error(error: ResourcePrepareError, stage: &str) -> CommandFailure {
    match error {
        ResourcePrepareError::InvalidMedia | ResourcePrepareError::InvalidMediaResponse => {
            CommandFailure::resource_media_invalid(stage)
        }
        ResourcePrepareError::Repository => CommandFailure::resource_storage(stage),
        ResourcePrepareError::ContextIdentityMismatch
        | ResourcePrepareError::InvalidPackSpecs
        | ResourcePrepareError::Contribution(_) => CommandFailure::pack_invalid(stage),
        ResourcePrepareError::InvalidInput
        | ResourcePrepareError::UnsupportedResource
        | ResourcePrepareError::WrongSourceMode
        | ResourcePrepareError::PackAssetUnavailable
        | ResourcePrepareError::PackAssetInvalid
        | ResourcePrepareError::Cancelled
        | ResourcePrepareError::Media(_) => CommandFailure::resource_invalid(stage),
    }
}

fn map_generation_readiness(error: SingleGenerateError) -> CommandFailure {
    let failure = error.run_failure();
    match failure.code.as_str() {
        "item.definition_invalid" | "run.input_invalid" | "feature.item_type_unsupported" => {
            CommandFailure::item_invalid("run.submit.readiness")
        }
        "pack.contribution_invalid" | "resource.spec_invalid" | "truth.context_mismatch" => {
            CommandFailure::pack_invalid("run.submit.readiness")
        }
        "resource.storage_failed" => CommandFailure::resource_storage("run.submit.readiness"),
        _ => CommandFailure::resource_invalid("run.submit.readiness"),
    }
}

fn map_plan_prepare_failure(failure: ats_runtime::RunFailure) -> CommandFailure {
    match failure.code.as_str() {
        "truth.missing" | "truth.evidence_missing" => {
            CommandFailure::truth_missing("run.submit.plan")
        }
        "pack.contribution_invalid" | "truth.context_mismatch" => {
            CommandFailure::pack_invalid("run.submit.plan")
        }
        "run.input_invalid" | "composition.profile.invalid" | "composition.profile.unsupported" => {
            CommandFailure::composition_invalid("run.submit.plan")
        }
        _ => CommandFailure::unclassified("run.submit.plan"),
    }
}

fn execution_graph_view(
    graph: &ExecutionGraphRecord,
    can_reconcile_succeeded: bool,
) -> ExecutionGraphView {
    let completed_nodes = graph
        .nodes()
        .values()
        .filter(|node| node.status == ExecutionNodeStatus::Succeeded)
        .count();
    let current = graph
        .nodes()
        .values()
        .find(|node| node.status == ExecutionNodeStatus::Running)
        .or_else(|| {
            graph.nodes().values().find(|node| {
                node.status == ExecutionNodeStatus::Pending
                    && node.depends_on.iter().all(|dependency| {
                        graph
                            .nodes()
                            .get(dependency)
                            .is_some_and(|value| value.status == ExecutionNodeStatus::Succeeded)
                    })
            })
        });
    let failure_code = graph
        .nodes()
        .values()
        .filter_map(|node| node.safe_failure.as_ref())
        .next_back()
        .map(|failure| failure.code.clone());
    ExecutionGraphView {
        execution_graph_id: graph.id().clone(),
        revision: graph.revision(),
        status: graph.status(),
        active_run_id: graph.active_run_id().cloned(),
        previous_run_id: graph.previous_run_id().cloned(),
        completed_nodes: u32::try_from(completed_nodes).unwrap_or(u32::MAX),
        total_nodes: u32::try_from(graph.nodes().len()).unwrap_or(u32::MAX),
        current_node_id: current.map(|node| node.node_id.clone()),
        current_role_id: current.map(|node| node.role_id.clone()),
        failure_code,
        can_pause: graph.status() == ExecutionGraphStatus::Running,
        can_resume: matches!(
            graph.status(),
            ExecutionGraphStatus::Paused | ExecutionGraphStatus::CommitPrepared
        ) && graph.active_run_id().is_none()
            || can_reconcile_succeeded,
        can_cancel: !matches!(
            graph.status(),
            ExecutionGraphStatus::Succeeded | ExecutionGraphStatus::Cancelled
        ) && graph.status() != ExecutionGraphStatus::CommitPrepared,
    }
}

fn can_reconcile_succeeded_graph(
    graph: &ExecutionGraphRecord,
    runs: &dyn RunRepository,
) -> Result<bool, RunRepositoryError> {
    if graph.status() != ExecutionGraphStatus::Succeeded {
        return Ok(false);
    }
    let Some(run_id) = graph.previous_run_id() else {
        return Ok(false);
    };
    match runs.get(run_id) {
        Ok(run) => Ok(run.status() == RunStatus::Failed
            && run
                .failure()
                .is_some_and(|failure| failure.code.as_str() == "run.interrupted")),
        Err(RunRepositoryError::NotFound) => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use ats_features::mod_generate_batch::BatchDefinitionItem;
    use ats_features::mod_plan::PlanItem;
    use ats_features::project_package::ProjectPackageRequest;
    use ats_kernel::{CompositionId, CompositionProfileId, ItemId};
    use ats_workspace::{ItemCompositionProfile, ItemCompositionSource};

    use super::*;

    #[test]
    fn confirmation_readiness_maps_to_a_resource_action() {
        for error in [
            CompositionConfirmationError::NotReady,
            CompositionConfirmationError::Graph(CompositionGraphError::Readiness),
        ] {
            let failure = map_confirmation_error(error);
            assert_eq!(failure.0.code.as_str(), "composition.confirm.not_ready");
            assert_eq!(failure.0.stage, "composition.draft.confirm");
            assert_eq!(
                failure.0.action,
                ats_kernel::RecoveryAction::ReplaceResource
            );
            assert!(!failure.0.retryable);
        }
    }

    fn stored_definition(item_type: &ItemTypeId) -> StoredItemDefinition {
        let mut definition =
            ItemDefinition::new(ItemId::parse("fixture-item").unwrap(), item_type.clone());
        definition.behavior_intent = vec!["Create one fixture item.".into()];
        StoredItemDefinition {
            definition_hash: definition.definition_hash().unwrap(),
            definition,
        }
    }

    fn submission<T: Serialize>(
        feature_id: &str,
        schema: ats_kernel::SchemaRef,
        request: &T,
    ) -> SubmitFeatureRequest {
        SubmitFeatureRequest {
            feature_id: FeatureId::parse(feature_id).unwrap(),
            request: VersionedPayload::from_typed(schema, request).unwrap(),
            source_path: None,
        }
    }

    fn plan(item_type: &ItemTypeId) -> PlanItem {
        PlanItem {
            item_id: "fixture-item".into(),
            item_type: item_type.as_str().into(),
            name: "Fixture item".into(),
            summary: "Fixture summary".into(),
            behavior_intent: vec!["Exercise the readiness gate.".into()],
            implementation_constraints: Vec::new(),
            evidence_requirements: Vec::new(),
            required_resource_roles: Vec::new(),
            acceptance_criteria: vec!["The request is classified.".into()],
        }
    }

    fn single(item_type: &ItemTypeId) -> SingleGenerateRequest {
        SingleGenerateRequest {
            artifact_id: "fixture-artifact".into(),
            mod_id: "FixtureMod".into(),
            plan: plan(item_type),
            definition: stored_definition(item_type),
        }
    }

    #[test]
    fn requested_item_types_are_decoded_from_each_generation_shape() {
        let temp = tempfile::tempdir().unwrap();
        let composition = Stage2Composition::built_in(temp.path().join("runtime")).unwrap();
        let drafts = FileCompositionDraftRepository::new(temp.path().join("project"));
        let all = composition
            .pack()
            .item_types()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let selected = all.first().unwrap().clone();

        let plan_all = submission(
            "mod.plan",
            ModPlanFeature::request_schema(),
            &ModPlanRequest {
                requirements: "Create one fixture item.".into(),
                item_type: None,
            },
        );
        assert_eq!(
            requested_item_types(&composition, &drafts, &plan_all).unwrap(),
            all
        );

        let single_request = submission(
            "mod.generate.single",
            SingleGenerateFeature::request_schema(),
            &single(&selected),
        );
        assert_eq!(
            requested_item_types(&composition, &drafts, &single_request).unwrap(),
            vec![selected.clone()]
        );

        let batch_request = submission(
            "mod.generate.batch",
            BatchGenerateFeature::request_schema(),
            &BatchGenerateRequest {
                mod_id: "FixtureMod".into(),
                items: vec![
                    BatchDefinitionItem {
                        artifact_id: "fixture-one".into(),
                        definition: stored_definition(&selected),
                    },
                    BatchDefinitionItem {
                        artifact_id: "fixture-two".into(),
                        definition: stored_definition(&selected),
                    },
                ],
                fail_fast: false,
            },
        );
        assert_eq!(
            requested_item_types(&composition, &drafts, &batch_request).unwrap(),
            vec![selected.clone(), selected.clone()]
        );

        let complex_request = submission(
            "mod.generate.complex",
            ComplexGenerateFeature::request_schema(),
            &ComplexGenerateRequest {
                batch: BatchGenerateRequest {
                    mod_id: "FixtureMod".into(),
                    items: vec![BatchDefinitionItem {
                        artifact_id: "fixture-artifact".into(),
                        definition: stored_definition(&selected),
                    }],
                    fail_fast: false,
                },
                package: ProjectPackageRequest {
                    artifact_id: "fixture-package".into(),
                    mod_id: "FixtureMod".into(),
                    source_relative_root: ".".into(),
                    output_relative_path: "dist/fixture.zip".into(),
                    compression_level: None,
                },
            },
        );
        assert_eq!(
            requested_item_types(&composition, &drafts, &complex_request).unwrap(),
            vec![selected]
        );

        std::fs::create_dir_all(temp.path().join("project")).unwrap();
        let root_id = ItemId::parse("fixture-character").unwrap();
        let composition_id = CompositionId::parse("character_suite").unwrap();
        let profile_set = composition
            .pack()
            .composition_profile(&composition_id)
            .unwrap();
        let standard = profile_set
            .profiles()
            .iter()
            .find(|profile| profile.id().as_str() == "standard")
            .unwrap();
        let profile = ItemCompositionProfile {
            composition_id,
            source: ItemCompositionSource::Preset {
                profile_id: CompositionProfileId::parse("standard").unwrap(),
            },
            parameters: standard.values().clone(),
        };
        let mut definition =
            ItemDefinition::new(root_id.clone(), profile_set.root_item_type().clone());
        definition.composition_profile = Some(profile.clone());
        let draft = CompositionDraft::new(
            CompositionDraftId::parse("fixture-retry-draft").unwrap(),
            composition.pack().id().clone(),
            composition.pack().content_sha256().clone(),
            root_id.clone(),
            profile,
            BTreeMap::from([(
                root_id.clone(),
                CompositionDraftNode {
                    definition,
                    expected_current_definition_hash: None,
                },
            )]),
            chrono::Utc::now(),
        )
        .unwrap();
        drafts.create(&draft).unwrap();
        let retry = submission(
            "composition.retry-node",
            CompositionRetryNodeFeature::request_schema(),
            &CompositionRetryNodeRequest {
                draft_id: draft.draft_id,
                expected_revision: draft.revision,
                item_id: root_id,
                instructions: "Revise this node.".into(),
            },
        );
        assert_eq!(
            requested_item_types(&composition, &drafts, &retry).unwrap(),
            vec![profile_set.root_item_type().clone()]
        );
    }

    #[test]
    fn readiness_rejects_item_runs_before_run_record_creation() {
        let temp = tempfile::tempdir().unwrap();
        let composition = Stage2Composition::built_in(temp.path().join("runtime")).unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let resources = FileResourceRepository::new(project.clone());
        let drafts = FileCompositionDraftRepository::new(project.clone());
        let items = ats_adapters::FileItemRepository::new(project);
        let request = submission(
            "mod.plan",
            ModPlanFeature::request_schema(),
            &ModPlanRequest {
                requirements: "Create one fixture item.".into(),
                item_type: None,
            },
        );

        let error = ensure_submission_ready(&composition, &items, &drafts, &resources, &request)
            .unwrap_err();
        let encoded = serde_json::to_value(error).unwrap();
        assert_eq!(encoded["code"], "truth.evidence_missing");
        assert_eq!(encoded["stage"], "run.submit.readiness");

        let undeclared = submission(
            "mod.plan",
            ModPlanFeature::request_schema(),
            &ModPlanRequest {
                requirements: "Create one unsupported item.".into(),
                item_type: Some("undeclared".into()),
            },
        );
        let encoded = serde_json::to_value(
            ensure_submission_ready(&composition, &items, &drafts, &resources, &undeclared)
                .unwrap_err(),
        )
        .unwrap();
        assert_eq!(encoded["code"], "item.definition_invalid");

        let unrelated = SubmitFeatureRequest {
            feature_id: FeatureId::parse("project.build").unwrap(),
            request: request.request,
            source_path: None,
        };
        ensure_submission_ready(&composition, &items, &drafts, &resources, &unrelated).unwrap();
    }
}
