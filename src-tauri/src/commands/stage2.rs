use std::path::PathBuf;
use std::sync::Arc;

use ats_adapters::{FileTruthSnapshotRepository, ItemStoreError, Sts2TruthImporter};
use ats_features::item_definition::{ItemDefinitionValidationMode, ItemDefinitionValidator};
use ats_features::mod_generate_batch::{BatchGenerateFeature, BatchGenerateRequest};
use ats_features::mod_generate_complex::{ComplexGenerateFeature, ComplexGenerateRequest};
use ats_features::mod_generate_single::{SingleGenerateFeature, SingleGenerateRequest};
use ats_features::mod_plan::{ModPlanFeature, ModPlanRequest};
use ats_features::{FeatureContract, FeatureSpec, built_in_feature_contracts};
use ats_game_context::{ItemCapabilityCatalog, TruthSnapshotRepository};
use ats_kernel::{FeatureId, ItemId, ItemTypeId, Sha256Digest};
use ats_runtime::{
    CancellationReason, CancellationToken, RunId, RunRecord, RunSummary, VersionedPayload,
};
use ats_workspace::{ItemDefinition, ItemRepository, StoredItemDefinition};
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
    ensure_submission_ready(composition.inner(), &submission)?;
    let run = RunRecord::new(submission.feature_id, submission.request);
    let root = session.path().to_path_buf();
    let meta = session.meta().clone();
    let composition = Arc::clone(composition.inner());
    let config = Arc::clone(config.inner());
    let resources = session.resource_repository();
    let source_path = submission.source_path.map(PathBuf::from);
    session
        .submit(run, move |run, cancellation, repository| async move {
            composition
                .execute(
                    &config,
                    &root,
                    &meta,
                    run,
                    repository.as_ref(),
                    resources.as_ref(),
                    source_path,
                    &cancellation,
                )
                .await
        })
        .await
        .map_err(|error| match error {
            SubmitError::Closing => CommandFailure::project_closing("run.submit"),
            SubmitError::Repository => CommandFailure::storage("run.submit"),
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
    submission: &SubmitFeatureRequest,
) -> CommandResult<()> {
    let requested = requested_item_types(composition, submission)?;
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
    Ok(())
}

fn requested_item_types(
    composition: &Stage2Composition,
    submission: &SubmitFeatureRequest,
) -> CommandResult<Vec<ItemTypeId>> {
    let all = || composition.pack().item_types().keys().cloned().collect();
    let parse = |value: &str| {
        ItemTypeId::parse(value).map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))
    };
    match submission.feature_id.as_str() {
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
            Ok(vec![parse(&request.plan.item_type)?])
        }
        "mod.generate.batch" => {
            let request = submission
                .request
                .decode::<BatchGenerateRequest>(&BatchGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            request
                .items
                .iter()
                .map(|item| parse(&item.plan.item_type))
                .collect()
        }
        "mod.generate.complex" => {
            let request = submission
                .request
                .decode::<ComplexGenerateRequest>(&ComplexGenerateFeature::request_schema())
                .map_err(|_| CommandFailure::item_invalid("run.submit.readiness"))?;
            let mut result = Vec::new();
            for item in request.planning_items {
                if let Some(item_type) = item.request.item_type {
                    result.push(parse(&item_type)?);
                } else {
                    result.extend(all());
                }
            }
            Ok(result)
        }
        _ => Ok(Vec::new()),
    }
}

fn map_item_store_error(error: ItemStoreError, stage: &str) -> CommandFailure {
    match error {
        ItemStoreError::NotFound => CommandFailure::item_not_found(stage),
        ItemStoreError::Contract(_)
        | ItemStoreError::TypeConflict
        | ItemStoreError::PathInvalid
        | ItemStoreError::Json(_) => CommandFailure::item_invalid(stage),
        ItemStoreError::Io { .. } | ItemStoreError::LockUnavailable => {
            CommandFailure::item_storage(stage)
        }
    }
}

#[cfg(test)]
mod tests {
    use ats_features::mod_generate_complex::ComplexPlanningItem;
    use ats_features::mod_generate_single::SelectedResource;
    use ats_features::mod_plan::PlanItem;
    use ats_features::project_package::ProjectPackageRequest;

    use super::*;

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
            selected_resources: Vec::<SelectedResource>::new(),
        }
    }

    #[test]
    fn requested_item_types_are_decoded_from_each_generation_shape() {
        let temp = tempfile::tempdir().unwrap();
        let composition = Stage2Composition::built_in(temp.path().join("runtime")).unwrap();
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
        assert_eq!(requested_item_types(&composition, &plan_all).unwrap(), all);

        let single_request = submission(
            "mod.generate.single",
            SingleGenerateFeature::request_schema(),
            &single(&selected),
        );
        assert_eq!(
            requested_item_types(&composition, &single_request).unwrap(),
            vec![selected.clone()]
        );

        let batch_request = submission(
            "mod.generate.batch",
            BatchGenerateFeature::request_schema(),
            &BatchGenerateRequest {
                items: vec![single(&selected), single(&selected)],
                fail_fast: false,
            },
        );
        assert_eq!(
            requested_item_types(&composition, &batch_request).unwrap(),
            vec![selected.clone(), selected.clone()]
        );

        let complex_request = submission(
            "mod.generate.complex",
            ComplexGenerateFeature::request_schema(),
            &ComplexGenerateRequest {
                mod_id: "FixtureMod".into(),
                planning_items: vec![ComplexPlanningItem {
                    request: ModPlanRequest {
                        requirements: "Create one fixture item.".into(),
                        item_type: Some(selected.as_str().into()),
                    },
                    artifact_id: "fixture-artifact".into(),
                    selected_resources: Vec::new(),
                }],
                fail_fast: false,
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
            requested_item_types(&composition, &complex_request).unwrap(),
            vec![selected]
        );
    }

    #[test]
    fn readiness_rejects_item_runs_before_run_record_creation() {
        let temp = tempfile::tempdir().unwrap();
        let composition = Stage2Composition::built_in(temp.path().join("runtime")).unwrap();
        let request = submission(
            "mod.plan",
            ModPlanFeature::request_schema(),
            &ModPlanRequest {
                requirements: "Create one fixture item.".into(),
                item_type: None,
            },
        );

        let error = ensure_submission_ready(&composition, &request).unwrap_err();
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
        let encoded =
            serde_json::to_value(ensure_submission_ready(&composition, &undeclared).unwrap_err())
                .unwrap();
        assert_eq!(encoded["code"], "item.definition_invalid");

        let unrelated = SubmitFeatureRequest {
            feature_id: FeatureId::parse("project.build").unwrap(),
            request: request.request,
            source_path: None,
        };
        ensure_submission_ready(&composition, &unrelated).unwrap();
    }
}
