use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use ats_adapters::{
    FileArtifactStore, FileCompositionDraftRepository, FileExecutionGraphRepository,
    FileItemRepository, FileProjectStager, FileProjectWriter, FileResourceRepository,
    FileRunRepository, PngResourceMediaProcessor, RegisteredBuildRunner,
    RegisteredValidationRunner, ZipPackageWriter,
};
use ats_features::FeatureSpec;
use ats_features::composition::{CompositionConfirmationService, CompositionDraftRef};
use ats_features::composition_generate::{
    CompositionGenerateContext, CompositionGenerateDependencies, CompositionGenerateFeature,
    CompositionGenerateRequest, CompositionGenerateService,
};
use ats_features::composition_plan::{
    CompositionPlanContext, CompositionPlanFeature, CompositionPlanRequest, CompositionPlanService,
};
use ats_features::mod_generate_single::{
    SingleGenerateError, SingleGenerateFeature, SingleGenerateService,
    validate_definition_resources,
};
use ats_features::mod_plan::{ModPlanFeature, ModPlanService};
use ats_features::project_build::{ProjectBuildFeature, ProjectBuildService};
use ats_features::project_package::{
    ProjectPackageFeature, ProjectPackageRequest, ProjectPackageService,
};
use ats_features::resource_prepare::{
    ResourcePrepareContext, ResourcePrepareFeature, ResourcePrepareRequest, ResourcePrepareService,
    ResourcePrepareSource,
};
use ats_game_context::{
    ContributionResolver, GamePackLoader, LoadedGamePack, TruthEvidenceRecord, TruthSnapshotIndex,
    TruthSnapshotManifest, TruthSnapshotSource, VerifiedContributionSet, VerifiedTruthSnapshot,
    built_in_game_pack_asset,
};
use ats_kernel::{
    CompositionDraftId, CompositionId, CompositionProfileId, ItemId, PrimitiveId, ResourceId,
    Sha256Digest,
};
use ats_runtime::{
    ArtifactManifest, CancellationToken, ExecutionGraphRepository, FinishReason, ModelClient,
    ModelError, ModelRequestSnapshot, ModelResponse, ModelStream, ModelStreamEvent, RunRecord,
    RunRepository, RunStatus, RunTransition, TokenUsage, VersionedPayload,
};
use ats_workspace::{
    CompositionDraftRepository, ItemCompositionSource, ItemResourceBinding, PreparedResourceMedia,
    ResourceBytesIngestRequest, ResourceOrigin, ResourceRepository, ResourceVersionProvenance,
    StoredItemDefinition,
};
use chrono::{Duration, Utc};
use futures_util::stream;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MOD_ID: &str = "PrototypeCharacterGate";
const ROOT_ID: &str = "prototype-character";
const RELIC_ID: &str = "prototype-relic";

struct QueueModel {
    responses: Mutex<VecDeque<String>>,
    snapshots: Mutex<Vec<ModelRequestSnapshot>>,
}

#[async_trait]
impl ModelClient for QueueModel {
    async fn complete(
        &self,
        request: ModelRequestSnapshot,
        _: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        self.snapshots.lock().unwrap().push(request);
        Ok(ModelResponse {
            model: "fixture-model".into(),
            content: self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(ModelError::InvalidResponse)?,
            finish_reason: FinishReason::EndTurn,
            usage: TokenUsage::default(),
        })
    }

    async fn stream(
        &self,
        _: ModelRequestSnapshot,
        _: &CancellationToken,
    ) -> Result<ModelStream, ModelError> {
        Ok(Box::pin(stream::empty::<
            Result<ModelStreamEvent, ModelError>,
        >()))
    }
}

#[tokio::test]
async fn sts2_branded_placeholder_prototype_prepares_resources_and_publishes_one_closure() {
    let Some(machine) = MachinePaths::from_environment() else {
        eprintln!(
            "skipped STS2 machine gate: ATS_TEST_STS2_ASSEMBLY_PATH and ATS_TEST_GODOT_PATH are required"
        );
        return;
    };

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    create_project(&project, &machine);
    let pack = GamePackLoader::load_built_in_sts2().unwrap();
    let truth = truth(&pack);
    let items = FileItemRepository::new(project.clone());
    let drafts = FileCompositionDraftRepository::new(project.clone());
    let resources = FileResourceRepository::new(project.clone());
    let artifacts = FileArtifactStore::new(project.clone());
    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.dotnet-validate").unwrap(),
        PrimitiveId::parse("image.role-transform").unwrap(),
        PrimitiveId::parse("process.dotnet-publish").unwrap(),
    ]);
    let composition_plan = resolve::<CompositionPlanFeature>(
        &resolver,
        &pack,
        CompositionPlanFeature::contribution_requirement(),
    );
    let resource = resolve::<ResourcePrepareFeature>(
        &resolver,
        &pack,
        ResourcePrepareFeature::contribution_requirement(),
    );

    let mut queued = VecDeque::from([prototype_plan_response()]);
    for item_id in prototype_item_ids() {
        let item_type = item_type(item_id);
        queued.push_back(plan_response(item_id, item_type));
        queued.push_back(bundle_response(item_id, item_type));
    }
    let model = QueueModel {
        responses: Mutex::new(queued),
        snapshots: Mutex::new(Vec::new()),
    };
    let profile_set = pack
        .composition_profile(&CompositionId::parse("character_suite").unwrap())
        .unwrap();
    let prototype = profile_set
        .profiles()
        .iter()
        .find(|profile| profile.id().as_str() == "prototype")
        .unwrap();
    let parameters = prototype.values().clone();
    let plan_request = CompositionPlanRequest {
        draft_id: CompositionDraftId::parse("prototype-character-draft").unwrap(),
        composition_id: CompositionId::parse("character_suite").unwrap(),
        concept: "A small deterministic branded placeholder Character suite.".into(),
        source: ItemCompositionSource::Preset {
            profile_id: CompositionProfileId::parse("prototype").unwrap(),
        },
        parameters,
        execution: None,
    };
    let planned = CompositionPlanService::built_in()
        .unwrap()
        .execute(
            &model,
            &items,
            &drafts,
            plan_request,
            CompositionPlanContext {
                pack: &pack,
                contributions: &composition_plan,
                truth: &truth,
                project_context: Some("Fresh STS2 Character machine-gate project"),
                custom_instructions: None,
                model: None,
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(planned.result.node_count, 11);
    let plan_prompt = snapshot_text(&planned.request_snapshot);
    assert!(plan_prompt.contains("CustomContentDictionary.AddCharacter"));
    assert!(plan_prompt.contains("starting_deck_size"));
    assert!(plan_prompt.contains("bindingRules"));

    let prepared = ResourcePrepareService
        .prepare_default(
            &PngResourceMediaProcessor,
            &resources,
            ResourcePrepareRequest {
                logical_role: "character.identity_master".into(),
                media_type: "image/png".into(),
                source: ResourcePrepareSource::PackDefault,
            },
            |pack, asset_id| built_in_game_pack_asset(pack, asset_id).ok(),
            ResourcePrepareContext {
                pack: &pack,
                contributions: &resource,
            },
        )
        .unwrap();
    assert_eq!(prepared.candidates.len(), 6);
    let mut character_bindings = BTreeMap::new();
    for candidate in prepared
        .candidates
        .into_iter()
        .filter(|candidate| candidate.logical_role != "character.identity_master")
    {
        ResourcePrepareService
            .select(
                &resources,
                &candidate.resource_id,
                &candidate.candidate_version,
                ResourcePrepareContext {
                    pack: &pack,
                    contributions: &resource,
                },
            )
            .unwrap();
        character_bindings.insert(
            ResourceId::parse(&candidate.logical_role).unwrap(),
            ItemResourceBinding {
                resource_id: candidate.resource_id,
                selected_version: candidate.candidate_version,
            },
        );
    }
    assert_eq!(character_bindings.len(), 5);

    let mut nodes = planned.draft.nodes.clone();
    for (item_id, node) in &mut nodes {
        match node.definition.item_type.as_str() {
            "card" => {
                bind_resource(
                    &resources,
                    &mut node.definition,
                    item_id,
                    "card.portrait",
                    250,
                    190,
                    false,
                );
                bind_resource(
                    &resources,
                    &mut node.definition,
                    item_id,
                    "card.big",
                    1000,
                    760,
                    false,
                );
            }
            "relic" => {
                for (role, width, height) in [
                    ("relic.normal", 128, 128),
                    ("relic.outline", 128, 128),
                    ("relic.big", 256, 256),
                ] {
                    bind_resource(
                        &resources,
                        &mut node.definition,
                        item_id,
                        role,
                        width,
                        height,
                        true,
                    );
                }
            }
            "character" => {
                node.definition.resource_bindings = character_bindings.clone();
            }
            _ => {}
        }
    }
    let revised = planned
        .draft
        .revised(nodes, planned.draft.updated_at + Duration::seconds(1))
        .unwrap();
    drafts
        .compare_and_set(planned.draft.revision, &revised)
        .unwrap();
    let selected = revised.nodes.keys().cloned().collect::<Vec<_>>();
    let confirmation =
        CompositionConfirmationService::confirm(&pack, &revised, &selected, &items).unwrap();
    assert_eq!(confirmation.definitions.len(), 11);
    let root = confirmation
        .definitions
        .iter()
        .find(|definition| definition.definition.item_id.as_str() == ROOT_ID)
        .unwrap()
        .clone();

    let required_character_roles = [
        "character.top_panel_icon",
        "character.top_panel_icon_outline",
        "character.select_icon",
        "character.select_locked_icon",
        "character.map_marker",
    ];
    let repin = |definition: ats_workspace::ItemDefinition| StoredItemDefinition {
        definition_hash: definition.definition_hash().unwrap(),
        definition,
    };
    let mut missing = root.definition.clone();
    missing
        .resource_bindings
        .remove(&ResourceId::parse(required_character_roles[0]).unwrap());
    assert!(matches!(
        validate_definition_resources(&pack, &resource, &resources, &repin(missing)),
        Err(SingleGenerateError::InvalidItemDefinition)
    ));

    let mut unselected = root.definition.clone();
    bind_unselected_resource(
        &resources,
        &mut unselected,
        &ItemId::parse(ROOT_ID).unwrap(),
        required_character_roles[0],
        85,
        85,
    );
    assert!(matches!(
        validate_definition_resources(&pack, &resource, &resources, &repin(unselected)),
        Err(SingleGenerateError::InvalidSelectedResource)
    ));

    let mut stale = root.definition.clone();
    stale
        .resource_bindings
        .get_mut(&ResourceId::parse(required_character_roles[0]).unwrap())
        .unwrap()
        .selected_version = sha256(b"stale-character-resource");
    assert!(matches!(
        validate_definition_resources(&pack, &resource, &resources, &repin(stale)),
        Err(SingleGenerateError::InvalidSelectedResource)
    ));

    let mut wrong_shape = root.definition.clone();
    bind_resource(
        &resources,
        &mut wrong_shape,
        &ItemId::parse(ROOT_ID).unwrap(),
        required_character_roles[0],
        84,
        85,
        true,
    );
    assert!(matches!(
        validate_definition_resources(&pack, &resource, &resources, &repin(wrong_shape)),
        Err(SingleGenerateError::InvalidSelectedResource)
    ));
    assert_eq!(model.snapshots.lock().unwrap().len(), 1);

    let composition = resolve::<CompositionGenerateFeature>(
        &resolver,
        &pack,
        CompositionGenerateFeature::contribution_requirement(),
    );
    let plan =
        resolve::<ModPlanFeature>(&resolver, &pack, ModPlanFeature::contribution_requirement());
    let single = resolve::<SingleGenerateFeature>(
        &resolver,
        &pack,
        SingleGenerateFeature::contribution_requirement(),
    );
    let build = resolve::<ProjectBuildFeature>(
        &resolver,
        &pack,
        ProjectBuildFeature::contribution_requirement(),
    );
    let package = resolve::<ProjectPackageFeature>(
        &resolver,
        &pack,
        ProjectPackageFeature::contribution_requirement(),
    );
    let request = CompositionGenerateRequest {
        artifact_id: "prototype-character-composition".into(),
        mod_id: MOD_ID.into(),
        root,
        draft: Some(CompositionDraftRef {
            draft_id: revised.draft_id.clone(),
            revision: revised.revision,
        }),
        package: ProjectPackageRequest {
            artifact_id: "prototype-character-composition".into(),
            mod_id: MOD_ID.into(),
            source_relative_root: "delivery".into(),
            output_relative_path: format!("packages/{MOD_ID}.zip"),
            compression_level: Some(6),
        },
        repair_policy: ats_features::composition_generate::RepairPolicy::MaxRounds {
            max_rounds: 3,
        },
        execution: None,
    };
    let run_id = ats_runtime::RunId::new();
    let context = || CompositionGenerateContext {
        pack: &pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        single_contributions: &single,
        resource_contributions: &resource,
        build_contributions: &build,
        package_contributions: &package,
        truth: &truth,
        project_root: &project,
        project_context: "Fresh STS2 Character machine-gate project",
        custom_instructions: None,
        model: None,
    };
    let plan_service = ModPlanService::built_in().unwrap();
    let single_service = SingleGenerateService::built_in().unwrap();
    let service = CompositionGenerateService::new(
        &plan_service,
        &single_service,
        &ProjectBuildService,
        &ProjectPackageService,
    );
    let start = service
        .prepare_staged_start(request, context(), &items, &resources, run_id.clone())
        .unwrap();
    let graphs = FileExecutionGraphRepository::new(project.clone());
    graphs.create_claimed(&start.graph, &run_id).unwrap();
    let runs = FileRunRepository::new(project.clone()).unwrap();
    let mut run = RunRecord::new_with_id(
        run_id.clone(),
        CompositionGenerateFeature::id(),
        VersionedPayload::from_typed(CompositionGenerateFeature::request_schema(), &start.request)
            .unwrap(),
    );
    run.apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let result = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &model,
                items: &items,
                resources: &resources,
                writer: &FileProjectWriter,
                stager: &FileProjectStager,
                validator: &RegisteredValidationRunner,
                artifacts: &artifacts,
                build_runner: &RegisteredBuildRunner,
                package_writer: &ZipPackageWriter,
            },
            &runs,
            &graphs,
            &mut run,
            start.request,
            context(),
            &CancellationToken::new(),
        )
        .await;
    let execution = match result {
        Ok(execution) => execution,
        Err(error) => panic!(
            "Character composition generation failed: {} ({:?})",
            error.run_failure().code,
            error
        ),
    };

    assert_eq!(run.status(), RunStatus::Succeeded);
    assert_eq!(execution.result.node_count, 11);
    let child_runs = runs.list().unwrap();
    assert_eq!(child_runs.len(), 24);
    assert!(
        child_runs
            .iter()
            .all(|child| child.status == RunStatus::Succeeded)
    );
    let graph = graphs
        .get(execution.result.execution_graph_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(graph.status(), ats_runtime::ExecutionGraphStatus::Succeeded);
    assert!(
        graph
            .nodes()
            .values()
            .all(|node| node.feedback_state.is_none())
    );
    assert_eq!(model.snapshots.lock().unwrap().len(), 23);
    let model_character_resource_roles = {
        let snapshots = model.snapshots.lock().unwrap();
        snapshots
            .iter()
            .filter(|snapshot| snapshot.feature_id() == &SingleGenerateFeature::id())
            .map(ModelRequestSnapshot::selected_resources)
            .find(|resources| resources.len() == required_character_roles.len())
            .unwrap()
            .iter()
            .map(|resource| resource.logical_role.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        model_character_resource_roles
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        required_character_roles
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    assert!(model.responses.lock().unwrap().is_empty());
    assert_eq!(execution.result.build.steps.len(), 1);
    assert_eq!(execution.result.package.report.file_count, 6);

    let cards: BTreeMap<String, Value> = serde_json::from_slice(
        &fs::read(project.join(format!("{MOD_ID}/localization/eng/cards.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(cards.len(), 18);
    let characters: BTreeMap<String, Value> = serde_json::from_slice(
        &fs::read(project.join(format!("{MOD_ID}/localization/eng/characters.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(characters.len(), 14);
    for path in [
        "top_panel.png",
        "top_panel_outline.png",
        "select.png",
        "select_locked.png",
        "map_marker.png",
    ] {
        assert!(
            project
                .join(format!("{MOD_ID}/images/characters/{ROOT_ID}/{path}"))
                .is_file()
        );
    }
    assert!(project.join(format!("packages/{MOD_ID}.zip")).is_file());
    assert!(!project.join(".ats/composition-staging").exists());
    assert!(!has_staging(&project));

    let manifest_path = project.join(&execution.result.artifact_manifest_ref);
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    assert_eq!(sha256(&manifest_bytes), execution.result.manifest_sha256);
    let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest.artifact_kind, "composition");
    assert_eq!(manifest.producing_run_id, run_id);
    assert_eq!(
        manifest.files.len(),
        execution.result.generated_file_count as usize + 1
    );
    let manifest_value = serde_json::to_value(&manifest).unwrap();
    let character_provenance = manifest_value["provenance"][0]["payload"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| {
            node["selectedResources"]
                .as_array()
                .is_some_and(|resources| resources.len() == required_character_roles.len())
        })
        .unwrap();
    assert_eq!(
        character_provenance["selectedResources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|resource| resource["logicalRole"].as_str().unwrap())
            .collect::<BTreeSet<_>>(),
        required_character_roles
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    for file in &manifest.files {
        let bytes = fs::read(
            manifest_path
                .parent()
                .unwrap()
                .join(&file.snapshot_relative_path),
        )
        .unwrap();
        assert_eq!(bytes.len() as u64, file.byte_length);
        assert_eq!(sha256(&bytes), file.sha256);
    }
}

#[tokio::test]
async fn sts2_standard_and_custom_profiles_build_valid_35_to_43_node_drafts() {
    let temp = tempfile::tempdir().unwrap();
    let pack = GamePackLoader::load_built_in_sts2().unwrap();
    let truth = truth(&pack);
    let resolver = ContributionResolver::new(Vec::<PrimitiveId>::new());
    let contributions = resolve::<CompositionPlanFeature>(
        &resolver,
        &pack,
        CompositionPlanFeature::contribution_requirement(),
    );
    let profile_set = pack
        .composition_profile(&CompositionId::parse("character_suite").unwrap())
        .unwrap();
    let standard = profile_set
        .profiles()
        .iter()
        .find(|profile| profile.id().as_str() == "standard")
        .unwrap();
    let mut cases = vec![(
        "standard-scale-draft",
        ItemCompositionSource::Preset {
            profile_id: CompositionProfileId::parse("standard").unwrap(),
        },
        standard.values().clone(),
        35_u32,
    )];
    let mut custom = standard.values().clone();
    custom.insert(
        ats_kernel::CompositionParameterId::parse("powers").unwrap(),
        8,
    );
    cases.push((
        "custom-scale-draft",
        ItemCompositionSource::Custom {
            base_profile_id: CompositionProfileId::parse("standard").unwrap(),
        },
        custom,
        43,
    ));

    for (draft_id, source, parameters, expected_nodes) in cases {
        let project = temp.path().join(draft_id);
        fs::create_dir_all(&project).unwrap();
        let items = FileItemRepository::new(project.clone());
        let drafts = FileCompositionDraftRepository::new(project);
        let model = QueueModel {
            responses: Mutex::new(VecDeque::from([scale_plan_response(&parameters)])),
            snapshots: Mutex::new(Vec::new()),
        };
        let execution = CompositionPlanService::built_in()
            .unwrap()
            .execute(
                &model,
                &items,
                &drafts,
                CompositionPlanRequest {
                    draft_id: CompositionDraftId::parse(draft_id).unwrap(),
                    composition_id: CompositionId::parse("character_suite").unwrap(),
                    concept: "Build a deterministic scale-gate Character suite.".into(),
                    source,
                    parameters,
                    execution: None,
                },
                CompositionPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: Some("O6 scale gate"),
                    custom_instructions: None,
                    model: None,
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(execution.result.node_count, expected_nodes);
        assert_eq!(execution.draft.nodes.len(), expected_nodes as usize);
        let mut nodes = execution.draft.nodes.clone();
        for (item_id, node) in &mut nodes {
            let roles: &[&str] = match node.definition.item_type.as_str() {
                "card" => &["card.portrait", "card.big"],
                "relic" => &["relic.normal", "relic.outline", "relic.big"],
                "potion" => &["potion.icon"],
                "power" => &["power.icon", "power.big"],
                _ => &[],
            };
            for role in roles {
                node.definition.resource_bindings.insert(
                    ResourceId::parse(*role).unwrap(),
                    ItemResourceBinding {
                        resource_id: ResourceId::parse(format!(
                            "resource.{}-{}",
                            item_id.as_str(),
                            role.replace('.', "-")
                        ))
                        .unwrap(),
                        selected_version: Sha256Digest::parse("a".repeat(64)).unwrap(),
                    },
                );
            }
        }
        let ready = execution
            .draft
            .revised(nodes, execution.draft.updated_at + Duration::seconds(1))
            .unwrap();
        let selected = ready.nodes.keys().cloned().collect::<Vec<_>>();
        let confirmation =
            CompositionConfirmationService::confirm(&pack, &ready, &selected, &items).unwrap();
        assert_eq!(confirmation.definitions.len(), expected_nodes as usize);
    }
}

struct MachinePaths {
    sts2_assembly: PathBuf,
    godot: PathBuf,
}

impl MachinePaths {
    fn from_environment() -> Option<Self> {
        let sts2_assembly = PathBuf::from(std::env::var_os("ATS_TEST_STS2_ASSEMBLY_PATH")?);
        let godot = PathBuf::from(std::env::var_os("ATS_TEST_GODOT_PATH")?);
        (sts2_assembly.is_file() && godot.is_file()).then_some(Self {
            sts2_assembly,
            godot,
        })
    }
}

fn create_project(project: &Path, machine: &MachinePaths) {
    let template = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("game_packs/sts2/template");
    copy_template(&template, project);
    fs::write(
        project.join("local.props"),
        format!(
            "<Project><PropertyGroup><Sts2AssemblyPath>{}</Sts2AssemblyPath><GodotPath>{}</GodotPath><ModsPath>$(MSBuildProjectDirectory)\\.ats\\local-mods\\</ModsPath></PropertyGroup></Project>",
            xml_escape(&machine.sts2_assembly.to_string_lossy()),
            xml_escape(&machine.godot.to_string_lossy())
        ),
    )
    .unwrap();
}

fn copy_template(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_dir()
            && matches!(
                entry.file_name().to_string_lossy().as_ref(),
                "packages" | "bin" | "obj" | ".vs" | ".ats" | ".godot"
            )
        {
            continue;
        }
        let name = entry
            .file_name()
            .to_string_lossy()
            .replace("ModTemplate", MOD_ID);
        let destination = target.join(name);
        if entry.file_type().unwrap().is_dir() {
            copy_template(&entry.path(), &destination);
        } else {
            let bytes = fs::read(entry.path()).unwrap();
            let text = String::from_utf8(bytes)
                .unwrap()
                .replace("ModTemplate", MOD_ID);
            fs::write(destination, text).unwrap();
        }
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn bind_resource(
    repository: &FileResourceRepository,
    definition: &mut ats_workspace::ItemDefinition,
    item_id: &ItemId,
    role: &str,
    width: u32,
    height: u32,
    has_alpha: bool,
) {
    let candidate = repository
        .ingest_bytes(ResourceBytesIngestRequest {
            logical_role: role.into(),
            origin: ResourceOrigin::UserUpload,
            file_name: format!("{}-{}.png", item_id.as_str(), role.replace('.', "-")),
            media: PreparedResourceMedia {
                media_type: "image/png".into(),
                width,
                height,
                has_alpha,
                bytes: format!("fixture:{}:{role}", item_id.as_str()).into_bytes(),
            },
            provenance: ResourceVersionProvenance::Original,
        })
        .unwrap();
    let selected = repository
        .select(candidate.resource_id(), &candidate.versions()[0].id)
        .unwrap();
    definition.resource_bindings.insert(
        ResourceId::parse(role).unwrap(),
        ItemResourceBinding {
            resource_id: selected.resource_id().clone(),
            selected_version: selected.selected_version().unwrap().clone(),
        },
    );
}

fn bind_unselected_resource(
    repository: &FileResourceRepository,
    definition: &mut ats_workspace::ItemDefinition,
    item_id: &ItemId,
    role: &str,
    width: u32,
    height: u32,
) {
    let candidate = repository
        .ingest_bytes(ResourceBytesIngestRequest {
            logical_role: role.into(),
            origin: ResourceOrigin::UserUpload,
            file_name: format!("{}-unselected.png", item_id.as_str()),
            media: PreparedResourceMedia {
                media_type: "image/png".into(),
                width,
                height,
                has_alpha: true,
                bytes: format!("unselected:{}:{role}", item_id.as_str()).into_bytes(),
            },
            provenance: ResourceVersionProvenance::Original,
        })
        .unwrap();
    definition.resource_bindings.insert(
        ResourceId::parse(role).unwrap(),
        ItemResourceBinding {
            resource_id: candidate.resource_id().clone(),
            selected_version: candidate.versions()[0].id.clone(),
        },
    );
}

fn prototype_plan_response() -> String {
    let cards = (1..=9)
        .map(|index| {
            let id = format!("prototype-card-{index:02}");
            let rarity = match index {
                1..=3 => "basic",
                4..=6 => "common",
                7..=8 => "uncommon",
                _ => "rare",
            };
            json!({
                "itemId": id,
                "itemType": "card",
                "canonicalFields": {
                    "pool": {"kind":"choice","value":"custom_character"},
                    "card_type": {"kind":"choice","value": if index % 2 == 0 {"skill"} else {"attack"}},
                    "rarity": {"kind":"choice","value":rarity},
                    "target": {"kind":"choice","value": if index % 2 == 0 {"self"} else {"any_enemy"}},
                    "base_cost": {"kind":"integer","value":1}
                },
                "behaviorIntent": [format!("Provide deterministic prototype card {index}.")],
                "localizations": localized_name_description(&format!("Prototype Card {index}")),
                "referenceBindings": {
                    "owner_character": [{"kind":"identity","itemId":ROOT_ID,"expectedItemType":"character"}]
                }
            })
        })
        .collect::<Vec<_>>();
    let card_refs = (1..=9)
        .map(|index| json!({"kind":"pinned","itemId":format!("prototype-card-{index:02}"),"quantity":1}))
        .collect::<Vec<_>>();
    let starting_deck = [(1, 4), (2, 3), (3, 3)]
        .into_iter()
        .map(|(index, quantity)| json!({"kind":"pinned","itemId":format!("prototype-card-{index:02}"),"quantity":quantity}))
        .collect::<Vec<_>>();
    let root = json!({
        "itemId": ROOT_ID,
        "itemType": "character",
        "canonicalFields": {
            "visual_profile": {"kind":"choice","value":"branded_placeholder"},
            "placeholder_id": {"kind":"choice","value":"ironclad"},
            "name_color": {"kind":"text","value":"7D3FC8FF"},
            "gender": {"kind":"choice","value":"neutral"},
            "starting_hp": {"kind":"integer","value":70},
            "starting_gold": {"kind":"integer","value":99},
            "max_energy": {"kind":"integer","value":3}
        },
        "behaviorIntent": ["Provide a playable placeholder Character with owned pools."],
        "localizations": character_localizations(),
        "referenceBindings": {
            "starting_deck": starting_deck,
            "cards": card_refs,
            "starting_relics": [{"kind":"pinned","itemId":RELIC_ID,"quantity":1}],
            "relics": [{"kind":"pinned","itemId":RELIC_ID,"quantity":1}]
        }
    });
    let relic = json!({
        "itemId": RELIC_ID,
        "itemType": "relic",
        "canonicalFields": {"rarity":{"kind":"choice","value":"starter"}},
        "behaviorIntent": ["Provide the Character's deterministic starter Relic."],
        "localizations": localized_name_description("Prototype Relic"),
        "referenceBindings": {
            "owner_character": [{"kind":"identity","itemId":ROOT_ID,"expectedItemType":"character"}]
        }
    });
    let mut nodes = cards;
    nodes.push(root);
    nodes.push(relic);
    json!({"rootItemId":ROOT_ID,"nodes":nodes}).to_string()
}

fn scale_plan_response(parameters: &BTreeMap<ats_kernel::CompositionParameterId, u32>) -> String {
    let count = |id: &str| parameters[&ats_kernel::CompositionParameterId::parse(id).unwrap()];
    let starter_cards = count("starter_card_types");
    let card_count =
        starter_cards + count("common_cards") + count("uncommon_cards") + count("rare_cards");
    let starter_relics = count("starter_relics");
    let relic_count = starter_relics + count("relics");
    let potion_count = count("potions");
    let power_count = count("powers");
    let root_id = "scale-character";

    let mut nodes = Vec::new();
    for index in 1..=card_count {
        nodes.push(json!({
            "itemId":format!("scale-card-{index:02}"),
            "itemType":"card",
            "canonicalFields":{
                "pool":{"kind":"choice","value":"custom_character"},
                "card_type":{"kind":"choice","value":if index % 2 == 0 {"skill"} else {"attack"}},
                "rarity":{"kind":"choice","value":if index <= starter_cards {"basic"} else {"common"}},
                "target":{"kind":"choice","value":if index % 2 == 0 {"self"} else {"any_enemy"}},
                "base_cost":{"kind":"integer","value":1}
            },
            "behaviorIntent":[format!("Provide scale card {index} behavior.")],
            "localizations":localized_name_description(&format!("Scale Card {index}")),
            "referenceBindings":{
                "owner_character":[{"kind":"identity","itemId":root_id,"expectedItemType":"character"}]
            }
        }));
    }
    for index in 1..=relic_count {
        nodes.push(json!({
            "itemId":format!("scale-relic-{index:02}"),
            "itemType":"relic",
            "canonicalFields":{"rarity":{"kind":"choice","value":if index <= starter_relics {"starter"} else {"common"}}},
            "behaviorIntent":[format!("Provide scale relic {index} behavior.")],
            "localizations":localized_name_description(&format!("Scale Relic {index}")),
            "referenceBindings":{
                "owner_character":[{"kind":"identity","itemId":root_id,"expectedItemType":"character"}]
            }
        }));
    }
    for index in 1..=potion_count {
        nodes.push(json!({
            "itemId":format!("scale-potion-{index:02}"),
            "itemType":"potion",
            "canonicalFields":{
                "rarity":{"kind":"choice","value":"common"},
                "usage":{"kind":"choice","value":"combat_only"},
                "target":{"kind":"choice","value":"any_enemy"}
            },
            "behaviorIntent":[format!("Provide scale potion {index} behavior.")],
            "localizations":localized_name_description(&format!("Scale Potion {index}")),
            "referenceBindings":{
                "owner_character":[{"kind":"identity","itemId":root_id,"expectedItemType":"character"}]
            }
        }));
    }
    for index in 1..=power_count {
        nodes.push(json!({
            "itemId":format!("scale-power-{index:02}"),
            "itemType":"power",
            "canonicalFields":{
                "power_type":{"kind":"choice","value":"buff"},
                "stack_type":{"kind":"choice","value":"counter"},
                "instance_type":{"kind":"choice","value":"none"},
                "allow_negative":{"kind":"boolean","value":false}
            },
            "behaviorIntent":[format!("Provide scale power {index} behavior.")],
            "localizations":localized_name_description(&format!("Scale Power {index}")),
            "referenceBindings":{}
        }));
    }

    let card_refs = (1..=card_count)
        .map(
            |index| json!({"kind":"pinned","itemId":format!("scale-card-{index:02}"),"quantity":1}),
        )
        .collect::<Vec<_>>();
    let deck_size = count("starting_deck_size");
    let base_quantity = deck_size / starter_cards;
    let remainder = deck_size % starter_cards;
    let starting_deck = (1..=starter_cards)
        .map(|index| {
            json!({
                "kind":"pinned",
                "itemId":format!("scale-card-{index:02}"),
                "quantity":base_quantity + u32::from(index <= remainder)
            })
        })
        .collect::<Vec<_>>();
    let relic_refs = (1..=relic_count)
        .map(|index| json!({"kind":"pinned","itemId":format!("scale-relic-{index:02}"),"quantity":1}))
        .collect::<Vec<_>>();
    let starting_relic_refs = (1..=starter_relics)
        .map(|index| json!({"kind":"pinned","itemId":format!("scale-relic-{index:02}"),"quantity":1}))
        .collect::<Vec<_>>();
    let potion_refs = (1..=potion_count)
        .map(|index| json!({"kind":"pinned","itemId":format!("scale-potion-{index:02}"),"quantity":1}))
        .collect::<Vec<_>>();
    let power_refs = (1..=power_count)
        .map(|index| json!({"kind":"pinned","itemId":format!("scale-power-{index:02}"),"quantity":1}))
        .collect::<Vec<_>>();
    let mut root_bindings = serde_json::Map::from_iter([
        ("starting_deck".into(), json!(starting_deck)),
        ("cards".into(), json!(card_refs)),
        ("starting_relics".into(), json!(starting_relic_refs)),
        ("relics".into(), json!(relic_refs)),
    ]);
    if !potion_refs.is_empty() {
        root_bindings.insert("potions".into(), json!(potion_refs));
    }
    if !power_refs.is_empty() {
        root_bindings.insert("powers".into(), json!(power_refs));
    }
    nodes.push(json!({
        "itemId":root_id,
        "itemType":"character",
        "canonicalFields":{
            "visual_profile":{"kind":"choice","value":"placeholder"},
            "placeholder_id":{"kind":"choice","value":"ironclad"},
            "name_color":{"kind":"text","value":"7D3FC8FF"},
            "gender":{"kind":"choice","value":"neutral"},
            "starting_hp":{"kind":"integer","value":70},
            "starting_gold":{"kind":"integer","value":99},
            "max_energy":{"kind":"integer","value":3}
        },
        "behaviorIntent":["Provide one deterministic scale-gate Character."],
        "localizations":character_localizations(),
        "referenceBindings":root_bindings
    }));
    json!({"rootItemId":root_id,"nodes":nodes}).to_string()
}

fn localized_name_description(name: &str) -> Value {
    json!({
        "eng":{"name":name,"description":format!("{name} description.")},
        "zhs":{"name":format!("{name} ZHS"),"description":format!("{name} ZHS description.")}
    })
}

fn character_localizations() -> Value {
    let fields = [
        ("title", "Prototype"),
        ("title_object", "Prototype"),
        ("description", "A deterministic placeholder Character."),
        ("pronoun_object", "them"),
        ("pronoun_subject", "they"),
        ("pronoun_possessive", "theirs"),
        ("possessive_adjective", "their"),
        ("aroma_principle", "A steady arcane aroma."),
        ("end_turn_ping_alive", "Ready."),
        ("end_turn_ping_dead", "..."),
        ("event_death_prevention", "Not yet."),
        ("gold_monologue", "Resources secured."),
        ("cards_modifier_title", "Prototype cards"),
        (
            "cards_modifier_description",
            "Cards owned by the Prototype.",
        ),
    ];
    let values = fields
        .into_iter()
        .map(|(key, value)| (key.to_owned(), Value::String(value.into())))
        .collect::<serde_json::Map<_, _>>();
    json!({"eng":values.clone(),"zhs":values})
}

fn prototype_item_ids() -> Vec<&'static str> {
    vec![
        "prototype-card-01",
        "prototype-card-02",
        "prototype-card-03",
        "prototype-card-04",
        "prototype-card-05",
        "prototype-card-06",
        "prototype-card-07",
        "prototype-card-08",
        "prototype-card-09",
        ROOT_ID,
        RELIC_ID,
    ]
}

fn item_type(item_id: &str) -> &'static str {
    if item_id == ROOT_ID {
        "character"
    } else if item_id == RELIC_ID {
        "relic"
    } else {
        "card"
    }
}

fn plan_response(item_id: &str, item_type: &str) -> String {
    json!({
        "itemId":item_id,
        "itemType":item_type,
        "name":item_id,
        "summary":"Prototype suite node",
        "behaviorIntent":["Compile as part of the complete Character closure."],
        "implementationConstraints":[],
        "evidenceRequirements":["Use the pinned STS2 and BaseLib evidence."],
        "acceptanceCriteria":["The complete closure compiles and packages."]
    })
    .to_string()
}

fn bundle_response(item_id: &str, item_type: &str) -> String {
    let files = match item_type {
        "character" => json!({
            "source": character_source(),
            "localization.eng": localization_object("PROTOTYPE_CHARACTER", character_loc_entries("Prototype")),
            "localization.zhs": localization_object("PROTOTYPE_CHARACTER", character_loc_entries("Prototype ZHS")),
            "localization.ancients.eng": architect_localization_object("Prototype"),
            "localization.ancients.zhs": architect_localization_object("Prototype ZHS")
        }),
        "relic" => json!({
            "source": relic_source(),
            "localization.eng": localization_object("PROTOTYPE_RELIC", vec![("title","Prototype Relic"),("description","A deterministic starter Relic."),("flavor","Built for a deterministic gate.")]),
            "localization.zhs": localization_object("PROTOTYPE_RELIC", vec![("title","Prototype Relic ZHS"),("description","A deterministic starter Relic ZHS."),("flavor","Built for a deterministic gate ZHS.")])
        }),
        "card" => {
            let index = item_id.rsplit('-').next().unwrap().parse::<u32>().unwrap();
            let key = format!("PROTOTYPE_CARD{index:02}");
            json!({
                "source": card_source(index),
                "localization.eng": localization_object(&key, vec![("title",&format!("Prototype Card {index}")),("description","A deterministic prototype card.")]),
                "localization.zhs": localization_object(&key, vec![("title",&format!("Prototype Card {index} ZHS")),("description","A deterministic prototype card ZHS.")])
            })
        }
        _ => unreachable!(),
    };
    json!({"files":files,"acceptanceNotes":["Generated for the Prototype closure."]}).to_string()
}

fn localization_object(prefix: &str, entries: Vec<(&str, &str)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(key, value)| {
                (
                    format!("{}-{prefix}.{key}", MOD_ID.to_ascii_uppercase()),
                    Value::String(value.into()),
                )
            })
            .collect(),
    )
}

fn character_loc_entries(title: &str) -> Vec<(&'static str, &str)> {
    vec![
        ("title", title),
        ("titleObject", title),
        ("description", "A deterministic placeholder Character."),
        ("pronounObject", "them"),
        ("pronounSubject", "they"),
        ("pronounPossessive", "theirs"),
        ("possessiveAdjective", "their"),
        ("aromaPrinciple", "A steady arcane aroma."),
        ("banter.alive.endTurnPing", "Ready."),
        ("banter.dead.endTurnPing", "..."),
        ("eventDeathPrevention", "Not yet."),
        ("goldMonologue", "Resources secured."),
        ("cardsModifierTitle", "Prototype cards"),
        ("cardsModifierDescription", "Cards owned by the Prototype."),
    ]
}

fn architect_localization_object(character: &str) -> Value {
    Value::Object(
        [
            (
                "THE_ARCHITECT.talk.PROTOTYPECHARACTERGATE-PROTOTYPE_CHARACTER.0-0r.char",
                character,
            ),
            (
                "THE_ARCHITECT.talk.PROTOTYPECHARACTERGATE-PROTOTYPE_CHARACTER.0-0r.next",
                "Continue",
            ),
            (
                "THE_ARCHITECT.talk.PROTOTYPECHARACTERGATE-PROTOTYPE_CHARACTER.0-1r.ancient",
                "The Architect answers.",
            ),
            (
                "THE_ARCHITECT.talk.PROTOTYPECHARACTERGATE-PROTOTYPE_CHARACTER.0-attack",
                "Both",
            ),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), Value::String(value.into())))
        .collect(),
    )
}

fn character_source() -> &'static str {
    r#"using BaseLib.Abstracts;
using Godot;
using MegaCrit.Sts2.Core.Entities.Characters;
using MegaCrit.Sts2.Core.Models;

namespace PrototypeCharacterGate;

public sealed class PrototypeCardPool : CustomCardPoolModel
{
    public override string Title => "prototype";
    public override bool IsColorless => false;
    public override Color ShaderColor => new("7D3FC8FF");
    public override Color DeckEntryCardColor => new("7D3FC8FF");
}

public sealed class PrototypeRelicPool : CustomRelicPoolModel { }

public sealed class PrototypePotionPool : CustomPotionPoolModel { }

public sealed class PrototypeCharacter : PlaceholderCharacterModel
{
    public override string PlaceholderID => "ironclad";
    public override string? CustomIconTexturePath => "PrototypeCharacterGate/images/characters/prototype-character/top_panel.png";
    public override string? CustomIconPath => "PrototypeCharacterGate/images/characters/prototype-character/top_panel_outline.png";
    public override string? CustomCharacterSelectIconPath => "PrototypeCharacterGate/images/characters/prototype-character/select.png";
    public override string? CustomCharacterSelectLockedIconPath => "PrototypeCharacterGate/images/characters/prototype-character/select_locked.png";
    public override string? CustomMapMarkerPath => "PrototypeCharacterGate/images/characters/prototype-character/map_marker.png";
    public override Color NameColor => new("7D3FC8FF");
    public override CharacterGender Gender => CharacterGender.Neutral;
    public override int StartingHp => 70;
    public override int StartingGold => 99;
    public override int MaxEnergy => 3;
    public override CardPoolModel CardPool => ModelDb.CardPool<PrototypeCardPool>();
    public override RelicPoolModel RelicPool => ModelDb.RelicPool<PrototypeRelicPool>();
    public override PotionPoolModel PotionPool => ModelDb.PotionPool<PrototypePotionPool>();
    public override IEnumerable<CardModel> StartingDeck =>
    [
        ModelDb.Card<PrototypeCard01>(), ModelDb.Card<PrototypeCard01>(),
        ModelDb.Card<PrototypeCard01>(), ModelDb.Card<PrototypeCard01>(),
        ModelDb.Card<PrototypeCard02>(), ModelDb.Card<PrototypeCard02>(),
        ModelDb.Card<PrototypeCard02>(), ModelDb.Card<PrototypeCard03>(),
        ModelDb.Card<PrototypeCard03>(), ModelDb.Card<PrototypeCard03>()
    ];
    public override IReadOnlyList<RelicModel> StartingRelics =>
        [ModelDb.Relic<PrototypeRelic>()];
}"#
}

fn card_source(index: u32) -> String {
    let card_type = if index.is_multiple_of(2) {
        "CardType.Skill"
    } else {
        "CardType.Attack"
    };
    let target = if index.is_multiple_of(2) {
        "TargetType.Self"
    } else {
        "TargetType.AnyEnemy"
    };
    let rarity = match index {
        1..=3 => "CardRarity.Basic",
        4..=6 => "CardRarity.Common",
        7..=8 => "CardRarity.Uncommon",
        _ => "CardRarity.Rare",
    };
    format!(
        r#"using BaseLib.Abstracts;
using BaseLib.Utils;
using MegaCrit.Sts2.Core.Entities.Cards;
using MegaCrit.Sts2.Core.GameActions.Multiplayer;

namespace PrototypeCharacterGate;

[Pool(typeof(PrototypeCardPool))]
public sealed class PrototypeCard{index:02}() : CustomCardModel(1, {card_type}, {rarity}, {target})
{{
    protected override Task OnPlay(PlayerChoiceContext choiceContext, CardPlay cardPlay) => Task.CompletedTask;
}}"#
    )
}

fn relic_source() -> &'static str {
    r#"using BaseLib.Abstracts;
using BaseLib.Utils;
using MegaCrit.Sts2.Core.Entities.Relics;

namespace PrototypeCharacterGate;

[Pool(typeof(PrototypeRelicPool))]
public sealed class PrototypeRelic : CustomRelicModel
{
    public override RelicRarity Rarity => RelicRarity.Starter;
}"#
}

fn truth(pack: &LoadedGamePack) -> VerifiedTruthSnapshot {
    let symbols = [
        "PlaceholderCharacterModel",
        "CustomCharacterModel",
        "CustomContentDictionary.AddCharacter",
        "CharacterModel",
        "CharacterGender",
        "CustomCardPoolModel",
        "CustomRelicPoolModel",
        "CustomPotionPoolModel",
        "CustomCardModel",
        "PoolAttribute",
        "CardType",
        "CardRarity",
        "TargetType",
        "CustomRelicModel",
        "CustomContentDictionary.AddModel",
        "RelicModel",
        "CustomPotionModel",
        "SharedPotionPool",
        "PotionRarity",
        "PotionUsage",
        "CustomPowerModel",
        "PowerModel",
        "PowerType",
        "PowerStackType",
        "PowerInstanceType",
        "PowerCmd",
    ];
    let evidence = symbols
        .into_iter()
        .map(|symbol| TruthEvidenceRecord {
            source_id: "machine-gate".into(),
            symbol: symbol.into(),
            purpose: "Prove the STS2 Character Prototype contract".into(),
            bounded_excerpt: format!("Verified declaration for {symbol}."),
            relative_path: format!("indexes/{symbol}.cs"),
        })
        .collect::<Vec<_>>();
    let index_bytes = serde_json::to_vec(&evidence).unwrap();
    let manifest = TruthSnapshotManifest::new(
        pack,
        vec![TruthSnapshotSource {
            id: "machine-gate".into(),
            kind: "verified_fixture".into(),
            version: Some("1".into()),
            relative_path: "sources/machine-gate.bin".into(),
            sha256: sha256(b"machine-gate"),
            byte_length: 12,
        }],
        vec![TruthSnapshotIndex {
            id: "symbols".into(),
            provider: PrimitiveId::parse("truth.fixture-index").unwrap(),
            relative_path: "indexes/symbols.json".into(),
            sha256: sha256(&index_bytes),
            record_count: evidence.len() as u32,
        }],
        BTreeMap::from([("fixture-index".into(), "1".into())]),
        Utc::now(),
    )
    .unwrap();
    VerifiedTruthSnapshot::verify(
        pack,
        manifest,
        BTreeMap::from([("symbols".into(), evidence)]),
    )
    .unwrap()
}

fn resolve<F: FeatureSpec>(
    resolver: &ContributionResolver,
    pack: &LoadedGamePack,
    requirement: ats_game_context::ContributionRequirement,
) -> VerifiedContributionSet {
    resolver.resolve(pack, &F::id(), &[requirement]).unwrap()
}

fn snapshot_text(snapshot: &ModelRequestSnapshot) -> String {
    snapshot
        .request()
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
}

fn has_staging(root: &Path) -> bool {
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_name().to_string_lossy().starts_with(".staging-") {
                return true;
            }
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            }
        }
    }
    false
}
