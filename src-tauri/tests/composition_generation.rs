use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use ats_adapters::{
    FileArtifactStore, FileExecutionGraphRepository, FileItemRepository, FileProjectStager,
    FileProjectWriter, FileResourceRepository, FileRunRepository, ZipPackageWriter,
};
use ats_features::FeatureSpec;
use ats_features::composition::CompositionDraftRef;
use ats_features::composition_generate::{
    CompositionGenerateContext, CompositionGenerateDependencies, CompositionGenerateFeature,
    CompositionGenerateRequest, CompositionGenerateService, EphemeralCompositionInput,
    HumanSemanticFeedbackRef,
};
use ats_features::mod_plan::{ModPlanFeature, ModPlanService};
use ats_features::project_build::{ProjectBuildFeature, ProjectBuildService};
use ats_features::project_package::{
    PackagePublication, ProjectPackageFeature, ProjectPackageRequest, ProjectPackageService,
};
use ats_features::resource_prepare::ResourcePrepareFeature;
use ats_game_context::{
    BehaviorAdapterError, BehaviorAdapterIdentity, BehaviorAdapterRegistry, BehaviorProposal,
    BehaviorRenderContext, ContributionResolver, GameBehaviorAdapter, GamePackLoader,
    GamePipelineProvider, GamePipelineRegistry, LoadedGamePack, PipelineCheckpointPolicy,
    PipelineNode, PipelineNodePhase, PipelineNodeScope, PipelineProviderError,
    PipelineProviderIdentity, PipelinePublishBarrier, PipelineResolveRequest, PipelineRetryClass,
    PipelineValueContract, RenderedFile, RenderedItemBundle, ResolvedPipelineGraph,
    TruthEvidenceRecord, TruthSnapshotIndex, TruthSnapshotManifest, TruthSnapshotSource,
    VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_game_sts2::{Sts2BehaviorAdapter, Sts2PipelineProvider};
use ats_kernel::{
    CompositionDraftId, CompositionId, CompositionParameterId, CompositionProfileId,
    ExecutionNodeId, ItemId, ItemReferenceSlotId, ItemTypeId, PipelineProfileId,
    PipelineProviderId, PrimitiveId, SchemaId, SchemaRef, SchemaVersion, Sha256Digest,
};
use ats_runtime::{
    ArtifactManifest, BuildError, BuildRunner, BuildStepReport, BuildStepRequest,
    CancellationToken, ExecutionGraphRecord, ExecutionGraphRepository, FinishReason, ModelClient,
    ModelError, ModelRequestSnapshot, ModelResponse, ModelStream, ModelStreamEvent, PackageError,
    PackagePrepareRequest, PackageReport, PackageWriter, PendingPackageOutput, RunRecord,
    RunRepository, RunStatus, RunTransition, TokenUsage, ValidationError, ValidationIssue,
    ValidationIssueRepairability, ValidationIssueSeverity, ValidationReport, ValidationRequest,
    ValidationRunner, VersionedPayload,
};
use ats_workspace::{
    ItemCompositionProfile, ItemCompositionSource, ItemDefinition, ItemReferenceBinding,
    ItemRepository, StoredItemDefinition,
};
use chrono::Utc;
use futures_util::stream;
use sha2::{Digest, Sha256};

struct QueueModel {
    responses: Mutex<VecDeque<String>>,
    requests: AtomicUsize,
}

struct BudgetCheckingModel<'a> {
    inner: &'a QueueModel,
    expected_cap: u32,
}

fn pipeline_registry() -> GamePipelineRegistry {
    let primitives = [
        "feature.mod-plan",
        "feature.composition-behavior",
        "game.behavior-render",
        "feature.composition-finalize",
        "feature.project-build",
        "feature.project-package",
        "code.dotnet-validate",
        "storage.atomic-publish",
    ]
    .into_iter()
    .map(|id| {
        (
            PrimitiveId::parse(id).unwrap(),
            SchemaVersion::new(1).unwrap(),
        )
    });
    let mut registry = GamePipelineRegistry::new(primitives).unwrap();
    registry.register(Sts2PipelineProvider::new()).unwrap();
    registry
}

fn behavior_registry() -> &'static BehaviorAdapterRegistry {
    static REGISTRY: std::sync::OnceLock<BehaviorAdapterRegistry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = BehaviorAdapterRegistry::new();
        registry.register(Sts2BehaviorAdapter::new()).unwrap();
        registry.register(FixtureBehaviorAdapter::new()).unwrap();
        registry
    })
}

fn behavior_hash(graph: &ExecutionGraphRecord, item_id: &str) -> Sha256Digest {
    graph
        .nodes()
        .values()
        .filter(|node| node.role_id == "composition.behavior")
        .find_map(|node| {
            let payload = node.active_checkpoint.as_ref()?.payload.payload();
            (payload["proposal"]["itemId"].as_str() == Some(item_id))
                .then(|| Sha256Digest::parse(payload["behaviorSha256"].as_str()?).ok())
                .flatten()
        })
        .expect("behavior checkpoint for fixture item")
}

struct FixtureBehaviorAdapter {
    identity: BehaviorAdapterIdentity,
}

impl FixtureBehaviorAdapter {
    fn new() -> Self {
        Self {
            identity: BehaviorAdapterIdentity {
                id: ats_kernel::BehaviorAdapterId::parse("fixture.behavior").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
                implementation_sha256: Sha256Digest::parse("a".repeat(64)).unwrap(),
            },
        }
    }
}

impl GameBehaviorAdapter for FixtureBehaviorAdapter {
    fn identity(&self) -> &BehaviorAdapterIdentity {
        &self.identity
    }

    fn validate_ir(
        &self,
        _context: &BehaviorRenderContext,
        proposal: &BehaviorProposal,
    ) -> Result<(), BehaviorAdapterError> {
        let supported = proposal.invocations.is_empty()
            || (proposal.invocations.len() == 1
                && proposal.invocations[0].capability_id.as_str() == "fixture.changed"
                && proposal.invocations[0].arguments.is_empty());
        if proposal.adapter == self.identity && supported {
            Ok(())
        } else {
            Err(BehaviorAdapterError::InvalidIr)
        }
    }

    fn render(
        &self,
        _context: &BehaviorRenderContext,
        proposal: &BehaviorProposal,
    ) -> Result<RenderedItemBundle, BehaviorAdapterError> {
        let bytes = format!(
            "public class Fixture{} {{ /* {} */ }}",
            proposal.item_id.as_str(),
            proposal.invocations.len()
        )
        .into_bytes();
        RenderedItemBundle::new(
            proposal,
            vec![RenderedFile::new(
                "source",
                format!("Generated/{}.cs", proposal.item_id.as_str()),
                bytes,
            )?],
        )
    }
}

struct DataOnlyProvider {
    identity: PipelineProviderIdentity,
}

impl DataOnlyProvider {
    fn new() -> Self {
        Self {
            identity: PipelineProviderIdentity {
                id: PipelineProviderId::parse("fixture.data-only").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
        }
    }
}

impl GamePipelineProvider for DataOnlyProvider {
    fn identity(&self) -> &PipelineProviderIdentity {
        &self.identity
    }

    fn resolve(
        &self,
        request: &PipelineResolveRequest,
    ) -> Result<ResolvedPipelineGraph, PipelineProviderError> {
        if request.profile_id != PipelineProfileId::parse("fixture.data-json").unwrap() {
            return Err(PipelineProviderError::UnsupportedProfile);
        }
        let render_id = ExecutionNodeId::parse("data.render").unwrap();
        let value = |slot_id: &str, schema_id: &str| PipelineValueContract {
            slot_id: slot_id.into(),
            schema: SchemaRef {
                id: SchemaId::parse(schema_id).unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
        };
        ResolvedPipelineGraph::new(
            self.identity.clone(),
            request,
            vec![
                PipelineNode {
                    node_id: render_id.clone(),
                    scope: PipelineNodeScope::Composition,
                    phase: PipelineNodePhase::Prepare,
                    primitive_id: PrimitiveId::parse("data.render-json").unwrap(),
                    primitive_version: SchemaVersion::new(1).unwrap(),
                    consumes: Vec::new(),
                    produces: PipelineValueContract {
                        slot_id: "composition.prepared-checkpoint".into(),
                        schema: SchemaRef {
                            id: SchemaId::parse("feature.composition-generate-finalize-checkpoint")
                                .unwrap(),
                            version: SchemaVersion::new(3).unwrap(),
                        },
                    },
                    depends_on: Vec::new(),
                    checkpoint_policy: PipelineCheckpointPolicy::OnSuccess,
                    retry_class: PipelineRetryClass::Never,
                    validation: Vec::new(),
                    publish_barrier: PipelinePublishBarrier::BeforeCommit,
                },
                PipelineNode {
                    node_id: ExecutionNodeId::parse("data.publish").unwrap(),
                    scope: PipelineNodeScope::Composition,
                    phase: PipelineNodePhase::Publish,
                    primitive_id: PrimitiveId::parse("storage.atomic-publish").unwrap(),
                    primitive_version: SchemaVersion::new(1).unwrap(),
                    consumes: vec![PipelineValueContract {
                        slot_id: "composition.prepared-checkpoint".into(),
                        schema: SchemaRef {
                            id: SchemaId::parse("feature.composition-generate-finalize-checkpoint")
                                .unwrap(),
                            version: SchemaVersion::new(3).unwrap(),
                        },
                    }],
                    produces: value("data.publication", "pipeline.publication"),
                    depends_on: vec![render_id],
                    checkpoint_policy: PipelineCheckpointPolicy::OnSuccess,
                    retry_class: PipelineRetryClass::LocalTransient,
                    validation: Vec::new(),
                    publish_barrier: PipelinePublishBarrier::Commit,
                },
            ],
        )
        .map_err(Into::into)
    }
}

fn data_only_pipeline_registry() -> GamePipelineRegistry {
    let mut registry = GamePipelineRegistry::new([
        (
            PrimitiveId::parse("data.render-json").unwrap(),
            SchemaVersion::new(1).unwrap(),
        ),
        (
            PrimitiveId::parse("storage.atomic-publish").unwrap(),
            SchemaVersion::new(1).unwrap(),
        ),
    ])
    .unwrap();
    registry.register(DataOnlyProvider::new()).unwrap();
    registry
}

#[async_trait]
impl ModelClient for QueueModel {
    async fn complete(
        &self,
        _: ModelRequestSnapshot,
        _: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
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

#[async_trait]
impl ModelClient for BudgetCheckingModel<'_> {
    async fn complete(
        &self,
        request: ModelRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        assert_eq!(
            request.request().max_output_tokens,
            expected_budget(&request, self.expected_cap)
        );
        self.inner.complete(request, cancellation).await
    }

    async fn stream(
        &self,
        request: ModelRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<ModelStream, ModelError> {
        assert_eq!(
            request.request().max_output_tokens,
            expected_budget(&request, self.expected_cap)
        );
        self.inner.stream(request, cancellation).await
    }
}

fn expected_budget(request: &ModelRequestSnapshot, cap: u32) -> u32 {
    match request.feature_id().as_str() {
        "mod.plan" => 4_096.min(cap),
        "composition.generate" => 4_096.min(cap),
        feature => panic!("unexpected model Feature {feature}"),
    }
}

struct FixtureValidation {
    reject: bool,
}

#[async_trait]
impl ValidationRunner for FixtureValidation {
    async fn validate(
        &self,
        request: ValidationRequest,
        _: &CancellationToken,
    ) -> Result<ValidationReport, ValidationError> {
        assert!(
            request
                .project_root
                .join("Generated/fixture-child.cs")
                .is_file()
        );
        assert!(
            request
                .project_root
                .join("Generated/fixture-root.cs")
                .is_file()
        );
        let report = ValidationReport {
            exit_code: i32::from(self.reject),
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            issues: Vec::new(),
        };
        if self.reject {
            Err(ValidationError::Rejected(report))
        } else {
            Ok(report)
        }
    }
}

struct RejectFirstMultiItemValidation {
    calls: AtomicUsize,
}

#[async_trait]
impl ValidationRunner for RejectFirstMultiItemValidation {
    async fn validate(
        &self,
        _: ValidationRequest,
        _: &CancellationToken,
    ) -> Result<ValidationReport, ValidationError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            let issue = |path: &str, fingerprint: char| ValidationIssue {
                validator_id: "code.dotnet-validate".into(),
                code: "CS0246".into(),
                severity: ValidationIssueSeverity::Error,
                relative_path: Some(path.into()),
                line: Some(1),
                column: Some(1),
                message: "A generated API symbol could not be found.".into(),
                symbol: Some("GeneratedApi".into()),
                repairability: ValidationIssueRepairability::GeneratedContent,
                fingerprint: Sha256Digest::parse(fingerprint.to_string().repeat(64)).unwrap(),
            };
            return Err(ValidationError::Rejected(ValidationReport {
                exit_code: 1,
                stdout_tail: String::new(),
                stderr_tail: String::new(),
                issues: vec![
                    issue("Generated/fixture-child.cs", 'd'),
                    issue("Generated/fixture-root.cs", 'e'),
                ],
            }));
        }
        Ok(ValidationReport {
            exit_code: 0,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
            issues: Vec::new(),
        })
    }
}

struct FixtureBuild;

#[async_trait]
impl BuildRunner for FixtureBuild {
    async fn run_step(
        &self,
        request: BuildStepRequest,
        _: &CancellationToken,
    ) -> Result<BuildStepReport, BuildError> {
        assert_eq!(
            request.isolated_output_property.as_deref(),
            Some("ModsPath")
        );
        assert_eq!(request.output_relative_root.as_deref(), Some("delivery"));
        let output = request.project_root.join("delivery/FixtureMod");
        fs::create_dir_all(&output).unwrap();
        fs::write(output.join("FixtureMod.dll"), b"fixture-binary").unwrap();
        Ok(BuildStepReport {
            primitive: request.primitive,
            exit_code: 0,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
        })
    }
}

#[tokio::test]
async fn whole_closure_publishes_once_and_cleans_stage() {
    let fixture = Fixture::new();
    let (result, parent_run_id, child_runs) = fixture.execute(false, false).await;
    let execution = match result {
        Ok(execution) => execution,
        Err(error) => panic!(
            "composition generation failed: {}",
            error.run_failure().code
        ),
    };

    assert_eq!(execution.result.node_count, 2);
    assert_eq!(execution.result.generated_file_count, 2);
    assert_eq!(
        execution.result.package.as_ref().unwrap().publication,
        PackagePublication::CompositionStaged
    );
    assert_eq!(child_runs.len(), 4);
    assert!(
        child_runs
            .iter()
            .all(|run| run.status == RunStatus::Succeeded)
    );
    assert!(fixture.project.join("Generated/fixture-child.cs").is_file());
    assert!(fixture.project.join("Generated/fixture-root.cs").is_file());
    assert!(fixture.project.join("packages/FixtureMod.zip").is_file());
    assert!(!fixture.project.join(".ats/composition-staging").exists());

    let manifest_path = fixture
        .project
        .join(&execution.result.artifact_manifest_ref);
    let manifest: ArtifactManifest =
        serde_json::from_slice(&fs::read(manifest_path).unwrap()).unwrap();
    assert_eq!(manifest.artifact_kind, "composition");
    assert_eq!(manifest.files.len(), 3);
    assert_eq!(manifest.producing_run_id, parent_run_id);
}

#[tokio::test]
async fn data_only_provider_executes_without_model_validation_or_toolchain() {
    let mut fixture = Fixture::new();
    fixture.pack = data_only_pack();
    fixture.truth = truth(&fixture.pack);
    fixture.request.artifact_id = "fixture-data-only".into();
    fixture.request.package = None;

    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.dotnet-validate").unwrap(),
        PrimitiveId::parse("process.fixture-build").unwrap(),
    ]);
    let composition = resolve::<CompositionGenerateFeature>(
        &resolver,
        &fixture.pack,
        CompositionGenerateFeature::contribution_requirement(),
    );
    let plan = resolve::<ModPlanFeature>(
        &resolver,
        &fixture.pack,
        ModPlanFeature::contribution_requirement(),
    );
    let resource = resolve::<ResourcePrepareFeature>(
        &resolver,
        &fixture.pack,
        ResourcePrepareFeature::contribution_requirement(),
    );
    let plan_service = ModPlanService::built_in().unwrap();
    let pipelines = data_only_pipeline_registry();
    let service = CompositionGenerateService::new(
        &plan_service,
        &ProjectBuildService,
        &ProjectPackageService,
        &pipelines,
    );
    let context = || CompositionGenerateContext {
        pack: &fixture.pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        resource_contributions: &resource,
        build_contributions: None,
        package_contributions: None,
        truth: &fixture.truth,
        project_root: &fixture.project,
        project_context: "Data-only fixture",
        custom_instructions: None,
        model: None,
        model_request_limits: ats_runtime::ModelRequestLimits::default(),
    };
    let run_id = ats_runtime::RunId::new();
    let start = service
        .prepare_staged_start(
            fixture.request.clone(),
            context(),
            &fixture.items,
            &fixture.resources,
            run_id.clone(),
        )
        .unwrap();
    assert_eq!(start.graph.nodes().len(), 1);
    assert_eq!(
        start.graph.nodes().values().next().unwrap().role_id,
        "data.render-json"
    );

    let graphs = FileExecutionGraphRepository::new(fixture.project.clone());
    graphs.create_claimed(&start.graph, &run_id).unwrap();
    let runs = FileRunRepository::new(fixture.project.clone()).unwrap();
    let mut run = RunRecord::new_with_id(
        run_id,
        CompositionGenerateFeature::id(),
        VersionedPayload::from_typed(CompositionGenerateFeature::request_schema(), &start.request)
            .unwrap(),
    );
    run.apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let model = QueueModel {
        responses: Mutex::new(VecDeque::new()),
        requests: AtomicUsize::new(0),
    };
    let execution = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &model,
                behavior_adapters: behavior_registry(),
                items: &fixture.items,
                resources: &fixture.resources,
                writer: &FileProjectWriter,
                stager: &FileProjectStager,
                validator: &FixtureValidation { reject: false },
                artifacts: &FileArtifactStore::new(fixture.project.clone()),
                build_runner: &FixtureBuild,
                package_writer: &FixturePackageWriter {
                    reject_commit: true,
                },
            },
            &runs,
            &graphs,
            &mut run,
            start.request,
            context(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(model.requests.load(Ordering::SeqCst), 0);
    assert!(runs.list().unwrap().is_empty());
    assert_eq!(run.status(), RunStatus::Succeeded);
    assert_eq!(execution.result.generated_file_count, 1);
    assert!(execution.result.items.is_empty());
    assert!(execution.result.build.is_none());
    assert!(execution.result.package.is_none());
    assert!(fixture.project.join("Generated/items.json").is_file());
    assert!(!fixture.project.join("delivery").exists());
    assert!(!fixture.project.join("packages").exists());
    assert!(!fixture.project.join(".ats/composition-staging").exists());
    let graph = graphs
        .get(execution.result.execution_graph_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(graph.status(), ats_runtime::ExecutionGraphStatus::Succeeded);
    assert_eq!(
        graph
            .nodes()
            .values()
            .filter(|node| node.status == ats_runtime::ExecutionNodeStatus::Succeeded)
            .count(),
        1
    );
    let manifest: ArtifactManifest = serde_json::from_slice(
        &fs::read(
            fixture
                .project
                .join(&execution.result.artifact_manifest_ref),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(manifest.files.len(), 1);
    assert_eq!(manifest.files[0].role, "data.items");
}

#[tokio::test]
async fn output_contract_feedback_regenerates_complete_bundle_and_publishes() {
    let fixture = Fixture::new();
    let (result, _, child_runs) = fixture
        .execute_with_responses(
            false,
            false,
            VecDeque::from([
                plan_response("child"),
                "not-json".into(),
                bundle_response("child"),
                plan_response("root"),
                bundle_response("root"),
            ]),
        )
        .await;
    let execution = result.unwrap();
    assert_eq!(child_runs.len(), 4);
    assert_eq!(
        child_runs
            .iter()
            .filter(|run| run.status == RunStatus::Failed)
            .count(),
        0
    );
    let graph = FileExecutionGraphRepository::new(fixture.project.clone())
        .get(execution.result.execution_graph_id.as_ref().unwrap())
        .unwrap();
    let feedback = graph
        .nodes()
        .values()
        .find_map(|node| node.feedback_state.as_ref())
        .unwrap();
    assert_eq!(feedback.round, 1);
    assert_eq!(
        feedback.phase,
        ats_runtime::ExecutionFeedbackPhase::OutputContract
    );
    assert_eq!(graph.semantic_request_count(), 3);
    assert_eq!(graph.semantic_feedback_count(), 1);
    assert!(fixture.project.join("Generated/fixture-child.cs").is_file());
    assert!(fixture.project.join("Generated/fixture-root.cs").is_file());
    assert!(fixture.project.join("packages/FixtureMod.zip").is_file());
}

#[tokio::test]
async fn succeeded_composition_adjusts_one_item_in_a_new_graph_and_revalidates_the_closure() {
    let fixture = Fixture::new();
    let (initial, source_run_id, _) = fixture.execute(false, false).await;
    let initial = initial.unwrap();
    let source_graph_id = initial.result.execution_graph_id.clone().unwrap();
    let graphs = FileExecutionGraphRepository::new(fixture.project.clone());
    let source_graph = graphs.get(&source_graph_id).unwrap();
    let source_revision = source_graph.revision();
    let source_manifest = fixture.project.join(&initial.result.artifact_manifest_ref);
    let source_checkpoints = source_graph
        .nodes()
        .values()
        .filter(|node| node.role_id == "composition.behavior")
        .map(|node| {
            (
                node.node_id.clone(),
                node.active_checkpoint.as_ref().unwrap().sha256.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.dotnet-validate").unwrap(),
        PrimitiveId::parse("process.fixture-build").unwrap(),
    ]);
    let composition = resolve::<CompositionGenerateFeature>(
        &resolver,
        &fixture.pack,
        CompositionGenerateFeature::contribution_requirement(),
    );
    let plan = resolve::<ModPlanFeature>(
        &resolver,
        &fixture.pack,
        ModPlanFeature::contribution_requirement(),
    );
    let resource = resolve::<ResourcePrepareFeature>(
        &resolver,
        &fixture.pack,
        ResourcePrepareFeature::contribution_requirement(),
    );
    let build = resolve::<ProjectBuildFeature>(
        &resolver,
        &fixture.pack,
        ProjectBuildFeature::contribution_requirement(),
    );
    let package = resolve::<ProjectPackageFeature>(
        &resolver,
        &fixture.pack,
        ProjectPackageFeature::contribution_requirement(),
    );
    let plan_service = ModPlanService::built_in().unwrap();
    let pipelines = pipeline_registry();
    let service = CompositionGenerateService::new(
        &plan_service,
        &ProjectBuildService,
        &ProjectPackageService,
        &pipelines,
    );
    let context = || CompositionGenerateContext {
        pack: &fixture.pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        resource_contributions: &resource,
        build_contributions: Some(&build),
        package_contributions: Some(&package),
        truth: &fixture.truth,
        project_root: &fixture.project,
        project_context: "Fixture project",
        custom_instructions: None,
        model: None,
        model_request_limits: ats_runtime::ModelRequestLimits::default(),
    };
    let adjustment_run_id = ats_runtime::RunId::new();
    let child = fixture
        .items
        .load_current(&ItemId::parse("fixture-child").unwrap())
        .unwrap();
    let adjustment = HumanSemanticFeedbackRef::new_for_source(
        source_graph_id.clone(),
        source_revision,
        child.definition.item_id.clone(),
        child.definition_hash.clone(),
        behavior_hash(&source_graph, child.definition.item_id.as_str()),
        "Make this item clearer without changing its identity.",
        Utc::now(),
    )
    .unwrap();
    let start = service
        .prepare_staged_adjustment(
            source_graph.clone(),
            source_revision,
            adjustment_run_id.clone(),
            adjustment.clone(),
            context(),
        )
        .unwrap();
    assert_ne!(start.graph.id(), &source_graph_id);
    graphs
        .create_claimed(&start.graph, &adjustment_run_id)
        .unwrap();
    let runs = FileRunRepository::new(fixture.project.clone()).unwrap();
    let mut run = RunRecord::new_with_id(
        adjustment_run_id,
        CompositionGenerateFeature::id(),
        VersionedPayload::from_typed(CompositionGenerateFeature::request_schema(), &start.request)
            .unwrap(),
    );
    run.apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let model = QueueModel {
        responses: Mutex::new(VecDeque::from([
            "not-json".into(),
            repaired_bundle_response("child"),
        ])),
        requests: AtomicUsize::new(0),
    };
    let validator = RejectFirstMultiItemValidation {
        calls: AtomicUsize::new(1),
    };
    let execution = service
        .execute_staged_with_input(
            CompositionGenerateDependencies {
                model: &model,
                behavior_adapters: behavior_registry(),
                items: &fixture.items,
                resources: &fixture.resources,
                writer: &FileProjectWriter,
                stager: &FileProjectStager,
                validator: &validator,
                artifacts: &FileArtifactStore::new(fixture.project.clone()),
                build_runner: &FixtureBuild,
                package_writer: &FixturePackageWriter {
                    reject_commit: false,
                },
            },
            &runs,
            &graphs,
            &mut run,
            start.request,
            context(),
            EphemeralCompositionInput::HumanSemanticFeedback {
                feedback_ref: adjustment,
                instruction: "Make this item clearer without changing its identity.".into(),
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(model.requests.load(Ordering::SeqCst), 2);
    assert_eq!(validator.calls.load(Ordering::SeqCst), 2);
    assert_eq!(run.status(), RunStatus::Succeeded);
    let derived = graphs
        .get(execution.result.execution_graph_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(derived.semantic_request_count(), 2);
    assert_eq!(derived.semantic_feedback_count(), 1);
    let changed = derived
        .nodes()
        .values()
        .filter(|node| node.role_id == "composition.behavior")
        .filter(|node| {
            source_checkpoints[&node.node_id] != node.active_checkpoint.as_ref().unwrap().sha256
        })
        .count();
    assert_eq!(changed, 1);
    assert_eq!(graphs.get(&source_graph_id).unwrap(), source_graph);
    assert!(source_manifest.is_file());
    assert!(
        fixture
            .project
            .join(&execution.result.artifact_manifest_ref)
            .is_file()
    );
    assert_ne!(
        execution.result.artifact_manifest_ref,
        initial.result.artifact_manifest_ref
    );
    assert!(!source_run_id.as_str().is_empty());
    assert!(!fixture.project.join(".ats/composition-staging").exists());

    let no_change_run_id = ats_runtime::RunId::new();
    let no_change_start = service
        .prepare_staged_adjustment(
            derived.clone(),
            derived.revision(),
            no_change_run_id.clone(),
            HumanSemanticFeedbackRef::new_for_source(
                start.graph.id().clone(),
                start.graph.revision(),
                child.definition.item_id,
                child.definition_hash,
                behavior_hash(&derived, "fixture-child"),
                "Keep the adjusted behavior exactly as it is.",
                Utc::now(),
            )
            .unwrap(),
            context(),
        )
        .unwrap();
    graphs
        .create_claimed(&no_change_start.graph, &no_change_run_id)
        .unwrap();
    let mut no_change_run = RunRecord::new_with_id(
        no_change_run_id,
        CompositionGenerateFeature::id(),
        VersionedPayload::from_typed(
            CompositionGenerateFeature::request_schema(),
            &no_change_start.request,
        )
        .unwrap(),
    );
    no_change_run
        .apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let no_change_model = QueueModel {
        responses: Mutex::new(VecDeque::from([repaired_bundle_response("child")])),
        requests: AtomicUsize::new(0),
    };
    let no_change_feedback_ref = no_change_start
        .request
        .adjustment
        .clone()
        .expect("adjustment ref");
    let no_change = service
        .execute_staged_with_input(
            CompositionGenerateDependencies {
                model: &no_change_model,
                behavior_adapters: behavior_registry(),
                items: &fixture.items,
                resources: &fixture.resources,
                writer: &FileProjectWriter,
                stager: &FileProjectStager,
                validator: &validator,
                artifacts: &FileArtifactStore::new(fixture.project.clone()),
                build_runner: &FixtureBuild,
                package_writer: &FixturePackageWriter {
                    reject_commit: false,
                },
            },
            &runs,
            &graphs,
            &mut no_change_run,
            no_change_start.request,
            context(),
            EphemeralCompositionInput::HumanSemanticFeedback {
                feedback_ref: no_change_feedback_ref,
                instruction: "Keep the adjusted behavior exactly as it is.".into(),
            },
            &CancellationToken::new(),
        )
        .await;
    let no_change = match no_change {
        Ok(_) => panic!("unchanged adjustment unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(
        no_change.run_failure().code.as_str(),
        "composition.adjustment.invalid"
    );
    assert_eq!(no_change_model.requests.load(Ordering::SeqCst), 1);
    assert_eq!(
        graphs.get(no_change_start.graph.id()).unwrap().status(),
        ats_runtime::ExecutionGraphStatus::Paused
    );
}

#[tokio::test]
async fn composition_adjustment_rejects_a_stale_definition_before_model_work() {
    let fixture = Fixture::new();
    let (initial, _, _) = fixture.execute(false, false).await;
    let initial = initial.unwrap();
    let graphs = FileExecutionGraphRepository::new(fixture.project.clone());
    let graph = graphs
        .get(initial.result.execution_graph_id.as_ref().unwrap())
        .unwrap();
    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.dotnet-validate").unwrap(),
        PrimitiveId::parse("process.fixture-build").unwrap(),
    ]);
    let composition = resolve::<CompositionGenerateFeature>(
        &resolver,
        &fixture.pack,
        CompositionGenerateFeature::contribution_requirement(),
    );
    let plan = resolve::<ModPlanFeature>(
        &resolver,
        &fixture.pack,
        ModPlanFeature::contribution_requirement(),
    );
    let resource = resolve::<ResourcePrepareFeature>(
        &resolver,
        &fixture.pack,
        ResourcePrepareFeature::contribution_requirement(),
    );
    let build = resolve::<ProjectBuildFeature>(
        &resolver,
        &fixture.pack,
        ProjectBuildFeature::contribution_requirement(),
    );
    let package = resolve::<ProjectPackageFeature>(
        &resolver,
        &fixture.pack,
        ProjectPackageFeature::contribution_requirement(),
    );
    let plan_service = ModPlanService::built_in().unwrap();
    let pipelines = pipeline_registry();
    let service = CompositionGenerateService::new(
        &plan_service,
        &ProjectBuildService,
        &ProjectPackageService,
        &pipelines,
    );
    let error = service
        .prepare_staged_adjustment(
            graph.clone(),
            graph.revision(),
            ats_runtime::RunId::new(),
            HumanSemanticFeedbackRef::new_for_source(
                graph.id().clone(),
                graph.revision(),
                ItemId::parse("fixture-child").unwrap(),
                Sha256Digest::parse("f".repeat(64)).unwrap(),
                Sha256Digest::parse("0".repeat(64)).unwrap(),
                "Change this item.",
                Utc::now(),
            )
            .unwrap(),
            CompositionGenerateContext {
                pack: &fixture.pack,
                composition_contributions: &composition,
                plan_contributions: &plan,
                resource_contributions: &resource,
                build_contributions: Some(&build),
                package_contributions: Some(&package),
                truth: &fixture.truth,
                project_root: &fixture.project,
                project_context: "Fixture project",
                custom_instructions: None,
                model: None,
                model_request_limits: ats_runtime::ModelRequestLimits::default(),
            },
        )
        .unwrap_err();
    assert_eq!(
        error.run_failure().code.as_str(),
        "composition.adjustment.stale"
    );
    assert_eq!(graphs.get(graph.id()).unwrap(), graph);
}

#[tokio::test]
async fn validation_failure_keeps_real_project_and_artifacts_unchanged() {
    let fixture = Fixture::new();
    let (result, _, child_runs) = fixture.execute(true, false).await;
    let error = match result {
        Ok(_) => panic!("composition generation unexpectedly succeeded"),
        Err(error) => error,
    };

    assert_eq!(error.run_failure().code.as_str(), "validation.rejected");
    assert_eq!(child_runs.len(), 2);
    assert!(!fixture.project.join("Generated").exists());
    assert!(!fixture.project.join("packages/FixtureMod.zip").exists());
    assert!(!fixture.project.join("artifacts").exists());
    assert!(!fixture.project.join(".ats/composition-staging").exists());
}

#[tokio::test]
async fn package_commit_failure_rolls_back_real_project_and_cleans_stage() {
    let fixture = Fixture::new();
    let (result, _, child_runs) = fixture.execute(false, true).await;
    let error = match result {
        Ok(_) => panic!("composition generation unexpectedly succeeded"),
        Err(error) => error,
    };

    assert_eq!(error.run_failure().code.as_str(), "artifact.write_failed");
    assert_eq!(child_runs.len(), 4);
    assert!(!fixture.project.join("Generated").exists());
    assert!(!fixture.project.join("packages/FixtureMod.zip").exists());
    assert!(!fixture.project.join("artifacts").exists());
    assert!(!fixture.project.join(".ats/composition-staging").exists());
}

#[tokio::test]
async fn staged_generation_resumes_only_the_failed_behavior_node() {
    let fixture = Fixture::new();
    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.dotnet-validate").unwrap(),
        PrimitiveId::parse("process.fixture-build").unwrap(),
    ]);
    let composition = resolve::<CompositionGenerateFeature>(
        &resolver,
        &fixture.pack,
        CompositionGenerateFeature::contribution_requirement(),
    );
    let plan = resolve::<ModPlanFeature>(
        &resolver,
        &fixture.pack,
        ModPlanFeature::contribution_requirement(),
    );
    let resource = resolve::<ResourcePrepareFeature>(
        &resolver,
        &fixture.pack,
        ResourcePrepareFeature::contribution_requirement(),
    );
    let build = resolve::<ProjectBuildFeature>(
        &resolver,
        &fixture.pack,
        ProjectBuildFeature::contribution_requirement(),
    );
    let package = resolve::<ProjectPackageFeature>(
        &resolver,
        &fixture.pack,
        ProjectPackageFeature::contribution_requirement(),
    );
    let plan_service = ModPlanService::built_in().unwrap();
    let build_service = ProjectBuildService;
    let package_service = ProjectPackageService;
    let pipelines = pipeline_registry();
    let service = CompositionGenerateService::new(
        &plan_service,
        &build_service,
        &package_service,
        &pipelines,
    );
    let context = |max_output_tokens| CompositionGenerateContext {
        pack: &fixture.pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        resource_contributions: &resource,
        build_contributions: Some(&build),
        package_contributions: Some(&package),
        truth: &fixture.truth,
        project_root: &fixture.project,
        project_context: "Fixture project",
        custom_instructions: None,
        model: None,
        model_request_limits: ats_runtime::ModelRequestLimits::new(Some(max_output_tokens))
            .unwrap(),
    };
    let graphs = FileExecutionGraphRepository::new(fixture.project.clone());
    let runs = FileRunRepository::new(fixture.project.clone()).unwrap();
    let first_run_id = ats_runtime::RunId::new();
    let start = service
        .prepare_staged_start(
            fixture.request.clone(),
            context(4_096),
            &fixture.items,
            &fixture.resources,
            first_run_id.clone(),
        )
        .unwrap();
    graphs.create_claimed(&start.graph, &first_run_id).unwrap();
    let mut first_run = RunRecord::new_with_id(
        first_run_id,
        CompositionGenerateFeature::id(),
        VersionedPayload::from_typed(CompositionGenerateFeature::request_schema(), &start.request)
            .unwrap(),
    );
    first_run
        .apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let first_model = QueueModel {
        responses: Mutex::new(VecDeque::from([
            plan_response("child"),
            "not-json".into(),
            "not-json".into(),
        ])),
        requests: AtomicUsize::new(0),
    };
    let first_model = BudgetCheckingModel {
        inner: &first_model,
        expected_cap: 4_096,
    };
    let writer = FileProjectWriter;
    let stager = FileProjectStager;
    let validator = FixtureValidation { reject: false };
    let artifacts = FileArtifactStore::new(fixture.project.clone());
    let package_writer = FixturePackageWriter {
        reject_commit: false,
    };
    let first = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &first_model,
                behavior_adapters: behavior_registry(),
                items: &fixture.items,
                resources: &fixture.resources,
                writer: &writer,
                stager: &stager,
                validator: &validator,
                artifacts: &artifacts,
                build_runner: &FixtureBuild,
                package_writer: &package_writer,
            },
            &runs,
            &graphs,
            &mut first_run,
            start.request,
            context(4_096),
            &CancellationToken::new(),
        )
        .await;
    let first = match first {
        Ok(_) => panic!("staged generation unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(first.run_failure().code.as_str(), "model.output_invalid");
    assert_eq!(first_model.inner.requests.load(Ordering::SeqCst), 3);
    let paused = graphs.get(start.graph.id()).unwrap();
    assert_eq!(paused.status(), ats_runtime::ExecutionGraphStatus::Paused);
    let persisted_graph = serde_json::to_string(&paused).unwrap();
    assert!(!persisted_graph.contains("Fixture project"));
    assert!(!persisted_graph.contains("messages"));
    assert!(!persisted_graph.contains("response_format"));
    assert!(!persisted_graph.contains("providerBody"));
    assert!(!persisted_graph.contains("rawCompletion"));
    let feedback = paused
        .nodes()
        .values()
        .find_map(|node| node.feedback_state.as_ref())
        .unwrap();
    assert_eq!(feedback.round, 1);
    assert_eq!(
        feedback.phase,
        ats_runtime::ExecutionFeedbackPhase::OutputContract
    );
    assert!(feedback.candidate_sha256.is_some());
    assert!(feedback.checkpoint_hash.is_none());
    assert_eq!(
        paused
            .nodes()
            .values()
            .find_map(|node| node.safe_failure.as_ref())
            .unwrap()
            .code
            .as_str(),
        "model.feedback_no_progress"
    );
    assert_eq!(
        paused
            .nodes()
            .values()
            .filter(|node| node.status == ats_runtime::ExecutionNodeStatus::Succeeded)
            .count(),
        1
    );

    let reopened_graphs = FileExecutionGraphRepository::new(fixture.project.clone());
    let reopened_runs = FileRunRepository::new(fixture.project.clone()).unwrap();
    let paused_revision = paused.revision();
    let second_run_id = ats_runtime::RunId::new();
    let resumed = service
        .prepare_staged_resume(
            paused,
            paused_revision,
            second_run_id.clone(),
            context(1_024),
        )
        .unwrap();
    reopened_graphs
        .compare_and_set(paused_revision, &resumed.graph)
        .unwrap();
    let mut second_run = RunRecord::new_with_id(
        second_run_id,
        CompositionGenerateFeature::id(),
        VersionedPayload::from_typed(
            CompositionGenerateFeature::request_schema(),
            &resumed.request,
        )
        .unwrap(),
    );
    second_run
        .apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let second_model = QueueModel {
        responses: Mutex::new(VecDeque::from([
            bundle_response("child"),
            plan_response("root"),
            bundle_response("root"),
        ])),
        requests: AtomicUsize::new(0),
    };
    let second_model = BudgetCheckingModel {
        inner: &second_model,
        expected_cap: 4_096,
    };
    let execution = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &second_model,
                behavior_adapters: behavior_registry(),
                items: &fixture.items,
                resources: &fixture.resources,
                writer: &writer,
                stager: &stager,
                validator: &validator,
                artifacts: &artifacts,
                build_runner: &FixtureBuild,
                package_writer: &package_writer,
            },
            &reopened_runs,
            &reopened_graphs,
            &mut second_run,
            resumed.request,
            context(1_024),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(second_model.inner.requests.load(Ordering::SeqCst), 3);
    assert_eq!(execution.result.node_count, 2);
    assert_eq!(second_run.status(), RunStatus::Succeeded);
    assert_eq!(
        reopened_graphs.get(resumed.graph.id()).unwrap().status(),
        ats_runtime::ExecutionGraphStatus::Succeeded
    );

    let succeeded = reopened_graphs.get(resumed.graph.id()).unwrap();
    let third_run_id = ats_runtime::RunId::new();
    let reconciled = service
        .prepare_staged_resume(
            succeeded,
            reopened_graphs.get(resumed.graph.id()).unwrap().revision(),
            third_run_id.clone(),
            context(1_024),
        )
        .unwrap();
    let mut third_run = RunRecord::new_with_id(
        third_run_id,
        CompositionGenerateFeature::id(),
        VersionedPayload::from_typed(
            CompositionGenerateFeature::request_schema(),
            &reconciled.request,
        )
        .unwrap(),
    );
    third_run
        .apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let no_model = QueueModel {
        responses: Mutex::new(VecDeque::new()),
        requests: AtomicUsize::new(0),
    };
    let reconciled_execution = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &no_model,
                behavior_adapters: behavior_registry(),
                items: &fixture.items,
                resources: &fixture.resources,
                writer: &writer,
                stager: &stager,
                validator: &validator,
                artifacts: &artifacts,
                build_runner: &FixtureBuild,
                package_writer: &package_writer,
            },
            &reopened_runs,
            &reopened_graphs,
            &mut third_run,
            reconciled.request,
            context(1_024),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(no_model.requests.load(Ordering::SeqCst), 0);
    assert_eq!(reconciled_execution.result, execution.result);
    assert_eq!(third_run.status(), RunStatus::Succeeded);
}

#[tokio::test]
async fn compiler_rejection_never_enters_behavior_feedback() {
    let fixture = Fixture::new();
    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.dotnet-validate").unwrap(),
        PrimitiveId::parse("process.fixture-build").unwrap(),
    ]);
    let composition = resolve::<CompositionGenerateFeature>(
        &resolver,
        &fixture.pack,
        CompositionGenerateFeature::contribution_requirement(),
    );
    let plan = resolve::<ModPlanFeature>(
        &resolver,
        &fixture.pack,
        ModPlanFeature::contribution_requirement(),
    );
    let resource = resolve::<ResourcePrepareFeature>(
        &resolver,
        &fixture.pack,
        ResourcePrepareFeature::contribution_requirement(),
    );
    let build = resolve::<ProjectBuildFeature>(
        &resolver,
        &fixture.pack,
        ProjectBuildFeature::contribution_requirement(),
    );
    let package = resolve::<ProjectPackageFeature>(
        &resolver,
        &fixture.pack,
        ProjectPackageFeature::contribution_requirement(),
    );
    let plan_service = ModPlanService::built_in().unwrap();
    let pipelines = pipeline_registry();
    let service = CompositionGenerateService::new(
        &plan_service,
        &ProjectBuildService,
        &ProjectPackageService,
        &pipelines,
    );
    let context = || CompositionGenerateContext {
        pack: &fixture.pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        resource_contributions: &resource,
        build_contributions: Some(&build),
        package_contributions: Some(&package),
        truth: &fixture.truth,
        project_root: &fixture.project,
        project_context: "Fixture project",
        custom_instructions: None,
        model: None,
        model_request_limits: ats_runtime::ModelRequestLimits::default(),
    };
    let graphs = FileExecutionGraphRepository::new(fixture.project.clone());
    let runs = FileRunRepository::new(fixture.project.clone()).unwrap();
    let run_id = ats_runtime::RunId::new();
    let start = service
        .prepare_staged_start(
            fixture.request.clone(),
            context(),
            &fixture.items,
            &fixture.resources,
            run_id.clone(),
        )
        .unwrap();
    graphs.create_claimed(&start.graph, &run_id).unwrap();
    let mut run = RunRecord::new_with_id(
        run_id,
        CompositionGenerateFeature::id(),
        VersionedPayload::from_typed(CompositionGenerateFeature::request_schema(), &start.request)
            .unwrap(),
    );
    run.apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let model = QueueModel {
        responses: Mutex::new(VecDeque::from([
            plan_response("child"),
            bundle_response("child"),
            plan_response("root"),
            bundle_response("root"),
        ])),
        requests: AtomicUsize::new(0),
    };
    let validator = RejectFirstMultiItemValidation {
        calls: AtomicUsize::new(0),
    };
    let result = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &model,
                behavior_adapters: behavior_registry(),
                items: &fixture.items,
                resources: &fixture.resources,
                writer: &FileProjectWriter,
                stager: &FileProjectStager,
                validator: &validator,
                artifacts: &FileArtifactStore::new(fixture.project.clone()),
                build_runner: &FixtureBuild,
                package_writer: &FixturePackageWriter {
                    reject_commit: false,
                },
            },
            &runs,
            &graphs,
            &mut run,
            start.request,
            context(),
            &CancellationToken::new(),
        )
        .await;
    let error = match result {
        Ok(_) => panic!("compiler rejection unexpectedly entered publication"),
        Err(error) => error,
    };
    assert_eq!(error.run_failure().code.as_str(), "validation.rejected");
    assert_eq!(model.requests.load(Ordering::SeqCst), 4);
    assert_eq!(validator.calls.load(Ordering::SeqCst), 1);
    let graph = graphs.get(start.graph.id()).unwrap();
    assert_eq!(graph.status(), ats_runtime::ExecutionGraphStatus::Paused);
    assert_eq!(graph.semantic_request_count(), 2);
    assert_eq!(graph.semantic_feedback_count(), 0);
    assert!(graph.repair_campaign().is_none());
    assert!(
        graph
            .nodes()
            .values()
            .all(|node| node.feedback_state.is_none())
    );
    assert_eq!(
        graph.graph_failure().unwrap().code.as_str(),
        "game.adapter_invalid"
    );
    assert!(!fixture.project.join(".ats/composition-staging").exists());
}

struct FixturePackageWriter {
    reject_commit: bool,
}

impl PackageWriter for FixturePackageWriter {
    fn prepare(
        &self,
        request: PackagePrepareRequest,
        cancellation: &CancellationToken,
    ) -> Result<Box<dyn PendingPackageOutput>, PackageError> {
        let inner = ZipPackageWriter.prepare(request, cancellation)?;
        Ok(Box::new(FixturePendingPackage {
            inner,
            reject_commit: self.reject_commit,
        }))
    }
}

struct FixturePendingPackage {
    inner: Box<dyn PendingPackageOutput>,
    reject_commit: bool,
}

impl PendingPackageOutput for FixturePendingPackage {
    fn report(&self) -> &PackageReport {
        self.inner.report()
    }

    fn output_path(&self) -> &Path {
        self.inner.output_path()
    }

    fn output_relative_path(&self) -> &str {
        self.inner.output_relative_path()
    }

    fn commit(self: Box<Self>) -> Result<(), PackageError> {
        if self.reject_commit {
            Err(PackageError::Io {
                operation: "fixture_commit",
                kind: io::ErrorKind::PermissionDenied,
                source: io::Error::new(io::ErrorKind::PermissionDenied, "fixture rejection"),
            })
        } else {
            self.inner.commit()
        }
    }

    fn rollback(self: Box<Self>) -> Result<(), PackageError> {
        self.inner.rollback()
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    project: std::path::PathBuf,
    pack: LoadedGamePack,
    truth: VerifiedTruthSnapshot,
    items: FileItemRepository,
    resources: FileResourceRepository,
    request: CompositionGenerateRequest,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("Fixture.csproj"), b"<Project />").unwrap();
        let pack = pack();
        let truth = truth(&pack);
        let items = FileItemRepository::new(project.clone());
        let resources = FileResourceRepository::new(project.clone());
        let child = child_definition();
        items.save(&child.definition).unwrap();
        let root = root_definition(&child);
        items.save(&root.definition).unwrap();
        let request = CompositionGenerateRequest {
            artifact_id: "fixture-composition".into(),
            mod_id: "FixtureMod".into(),
            root,
            draft: Some(CompositionDraftRef {
                draft_id: CompositionDraftId::parse("fixture-draft").unwrap(),
                revision: 1,
            }),
            package: Some(ProjectPackageRequest {
                artifact_id: "fixture-composition".into(),
                mod_id: "FixtureMod".into(),
                source_relative_root: "delivery".into(),
                output_relative_path: "packages/FixtureMod.zip".into(),
                compression_level: Some(6),
            }),
            repair_policy: ats_features::composition_generate::RepairPolicy::MaxRounds {
                max_rounds: 3,
            },
            adjustment: None,
            execution: None,
        };
        Self {
            _temp: temp,
            project,
            pack,
            truth,
            items,
            resources,
            request,
        }
    }

    async fn execute(
        &self,
        reject_validation: bool,
        reject_package_commit: bool,
    ) -> (
        Result<
            ats_features::composition_generate::StagedCompositionGenerateExecution,
            ats_features::composition_generate::CompositionGenerateError,
        >,
        ats_runtime::RunId,
        Vec<ats_runtime::RunSummary>,
    ) {
        self.execute_with_responses(
            reject_validation,
            reject_package_commit,
            VecDeque::from([
                plan_response("child"),
                bundle_response("child"),
                plan_response("root"),
                bundle_response("root"),
            ]),
        )
        .await
    }

    async fn execute_with_responses(
        &self,
        reject_validation: bool,
        reject_package_commit: bool,
        responses: VecDeque<String>,
    ) -> (
        Result<
            ats_features::composition_generate::StagedCompositionGenerateExecution,
            ats_features::composition_generate::CompositionGenerateError,
        >,
        ats_runtime::RunId,
        Vec<ats_runtime::RunSummary>,
    ) {
        let model = QueueModel {
            responses: Mutex::new(responses),
            requests: AtomicUsize::new(0),
        };
        let resolver = ContributionResolver::new([
            PrimitiveId::parse("code.dotnet-validate").unwrap(),
            PrimitiveId::parse("process.fixture-build").unwrap(),
        ]);
        let composition = resolve::<CompositionGenerateFeature>(
            &resolver,
            &self.pack,
            CompositionGenerateFeature::contribution_requirement(),
        );
        let plan = resolve::<ModPlanFeature>(
            &resolver,
            &self.pack,
            ModPlanFeature::contribution_requirement(),
        );
        let resource = resolve::<ResourcePrepareFeature>(
            &resolver,
            &self.pack,
            ResourcePrepareFeature::contribution_requirement(),
        );
        let build = resolve::<ProjectBuildFeature>(
            &resolver,
            &self.pack,
            ProjectBuildFeature::contribution_requirement(),
        );
        let package = resolve::<ProjectPackageFeature>(
            &resolver,
            &self.pack,
            ProjectPackageFeature::contribution_requirement(),
        );
        let plan_service = ModPlanService::built_in().unwrap();
        let build_service = ProjectBuildService;
        let package_service = ProjectPackageService;
        let pipelines = pipeline_registry();
        let writer = FileProjectWriter;
        let stager = FileProjectStager;
        let validator = FixtureValidation {
            reject: reject_validation,
        };
        let artifacts = FileArtifactStore::new(self.project.clone());
        let package_writer = FixturePackageWriter {
            reject_commit: reject_package_commit,
        };
        let run_id = ats_runtime::RunId::new();
        let context = || CompositionGenerateContext {
            pack: &self.pack,
            composition_contributions: &composition,
            plan_contributions: &plan,
            resource_contributions: &resource,
            build_contributions: Some(&build),
            package_contributions: Some(&package),
            truth: &self.truth,
            project_root: &self.project,
            project_context: "Fixture project",
            custom_instructions: None,
            model: None,
            model_request_limits: ats_runtime::ModelRequestLimits::default(),
        };
        let service = CompositionGenerateService::new(
            &plan_service,
            &build_service,
            &package_service,
            &pipelines,
        );
        let start = service
            .prepare_staged_start(
                self.request.clone(),
                context(),
                &self.items,
                &self.resources,
                run_id.clone(),
            )
            .unwrap();
        let graphs = FileExecutionGraphRepository::new(self.project.clone());
        graphs.create_claimed(&start.graph, &run_id).unwrap();
        let runs = FileRunRepository::new(self.project.clone()).unwrap();
        let mut run = RunRecord::new_with_id(
            run_id.clone(),
            CompositionGenerateFeature::id(),
            VersionedPayload::from_typed(
                CompositionGenerateFeature::request_schema(),
                &start.request,
            )
            .unwrap(),
        );
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        let result = service
            .execute_staged(
                CompositionGenerateDependencies {
                    model: &model,
                    behavior_adapters: behavior_registry(),
                    items: &self.items,
                    resources: &self.resources,
                    writer: &writer,
                    stager: &stager,
                    validator: &validator,
                    artifacts: &artifacts,
                    build_runner: &FixtureBuild,
                    package_writer: &package_writer,
                },
                &runs,
                &graphs,
                &mut run,
                start.request,
                context(),
                &CancellationToken::new(),
            )
            .await;
        (result, run_id, runs.list().unwrap())
    }
}

fn resolve<F: FeatureSpec>(
    resolver: &ContributionResolver,
    pack: &LoadedGamePack,
    requirement: ats_game_context::ContributionRequirement,
) -> VerifiedContributionSet {
    resolver.resolve(pack, &F::id(), &[requirement]).unwrap()
}

fn stored(definition: ItemDefinition) -> StoredItemDefinition {
    StoredItemDefinition {
        definition_hash: definition.definition_hash().unwrap(),
        definition,
    }
}

fn child_definition() -> StoredItemDefinition {
    let mut definition = ItemDefinition::new(
        ItemId::parse("fixture-child").unwrap(),
        ItemTypeId::parse("child").unwrap(),
    );
    definition.behavior_intent = vec!["Provide child behavior.".into()];
    definition.reference_bindings.insert(
        ItemReferenceSlotId::parse("owner").unwrap(),
        vec![ItemReferenceBinding::Identity {
            item_id: ItemId::parse("fixture-root").unwrap(),
            expected_item_type: ItemTypeId::parse("root").unwrap(),
        }],
    );
    stored(definition)
}

fn root_definition(child: &StoredItemDefinition) -> StoredItemDefinition {
    let mut definition = ItemDefinition::new(
        ItemId::parse("fixture-root").unwrap(),
        ItemTypeId::parse("root").unwrap(),
    );
    definition.behavior_intent = vec!["Provide root behavior.".into()];
    definition.reference_bindings.insert(
        ItemReferenceSlotId::parse("children").unwrap(),
        vec![ItemReferenceBinding::Pinned {
            item_id: child.definition.item_id.clone(),
            definition_hash: child.definition_hash.clone(),
            quantity: 1,
        }],
    );
    definition.composition_profile = Some(ItemCompositionProfile {
        composition_id: CompositionId::parse("fixture-suite").unwrap(),
        source: ItemCompositionSource::Preset {
            profile_id: CompositionProfileId::parse("standard").unwrap(),
        },
        parameters: BTreeMap::from([(CompositionParameterId::parse("child-count").unwrap(), 1)]),
    });
    stored(definition)
}

fn plan_response(item_type: &str) -> String {
    serde_json::json!({
        "itemId":format!("fixture-{item_type}"),
        "itemType":item_type,
        "name":format!("Fixture {item_type}"),
        "summary":"Fixture summary",
        "behaviorIntent":[format!("Provide {item_type} behavior.")],
        "implementationConstraints":[],
        "evidenceRequirements":["Use verified fixture evidence."],
        "acceptanceCriteria":["The closure validates."]
    })
    .to_string()
}

fn bundle_response(item_type: &str) -> String {
    let _ = item_type;
    serde_json::json!({"invocations":[]}).to_string()
}

fn repaired_bundle_response(item_type: &str) -> String {
    let _ = item_type;
    serde_json::json!({
        "invocations":[{"capabilityId":"fixture.changed","arguments":{}}]
    })
    .to_string()
}

fn pack() -> LoadedGamePack {
    pack_with_pipeline("game.sts2", "sts2.composition-generate")
}

fn data_only_pack() -> LoadedGamePack {
    pack_with_pipeline("fixture.data-only", "fixture.data-json")
}

fn pack_with_pipeline(provider_id: &str, profile_id: &str) -> LoadedGamePack {
    let value = serde_json::json!({
        "schemaVersion":5,
        "id":"fixture-game",
        "displayName":"Fixture",
        "itemTypes":[
            {
                "id":"root","displayNames":{"eng":"Root"},
                "referenceSlots":[{
                    "id":"children","displayNames":{"eng":"Children"},"kind":"pinned",
                    "allowedItemTypes":["child"],"minItems":1,"maxItems":1,
                    "minQuantity":1,"maxQuantity":1
                }],
                "evidenceQueries":[{"symbols":["Root.Symbol"],"terms":[]}]
            },
            {
                "id":"child","displayNames":{"eng":"Child"},
                "referenceSlots":[{
                    "id":"owner","displayNames":{"eng":"Owner"},"kind":"identity",
                    "allowedItemTypes":["root"],"minItems":1,"maxItems":1,
                    "minQuantity":1,"maxQuantity":1
                }],
                "evidenceQueries":[{"symbols":["Child.Symbol"],"terms":[]}]
            }
        ],
        "compositionProfiles":[{
            "id":"fixture-suite","displayNames":{"eng":"Fixture suite"},
            "rootItemType":"root","defaultProfile":"standard","customBaseProfile":"standard",
            "maxNodes":8,"baseNodeCount":1,
            "parameters":[{
                "id":"child-count","displayNames":{"eng":"Children"},
                "min":1,"max":2,"nodeWeight":1
            }],
            "profiles":[{
                "id":"standard","displayNames":{"eng":"Standard"},
                "values":{"child-count":1}
            }],
            "constraints":[]
        }],
        "behavior":{
            "adapter":{
                "id":"fixture.behavior",
                "version":1,
                "implementationSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            },
            "catalog":{
                "schemaVersion":1,
                "id":"fixture.capabilities",
                "version":1,
                "adapter":{
                    "id":"fixture.behavior",
                    "version":1,
                    "implementationSha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                },
                "capabilities":[{
                    "id":"fixture.changed",
                    "description":"Marks an operator-requested semantic change.",
                    "maxInvocationsPerItem":1,
                    "parameters":[]
                }],
                "itemTypes":[
                    {"itemType":"root","allowedCapabilities":["fixture.changed"],"minInvocations":0,"maxInvocations":1},
                    {"itemType":"child","allowedCapabilities":["fixture.changed"],"minInvocations":0,"maxInvocations":1}
                ]
            }
        },
        "contributions":[
            {
                "slotId":"composition.generate","featureId":"composition.generate",
                "schema":{"id":"pack.composition-generate","version":2},
                "payload":{"pipeline":{"provider":{"id":provider_id,"version":1},"profileId":profile_id}}
            },
            {
                "slotId":"mod.plan.guidance","featureId":"mod.plan",
                "schema":{"id":"pack.mod-plan-guidance","version":3},
                "payload":{"guidance":["Use fixture evidence."]}
            },
            {
                "slotId":"resource.prepare.specs","featureId":"resource.prepare",
                "schema":{"id":"pack.resource-specs","version":3},
                "payload":{"roles":[{
                    "id":"fixture.unused","mediaTypes":["image/png"],
                    "width":1,"height":1,"requireAlpha":false,"source":{"kind":"master"}
                }]}
            },
            {
                "slotId":"project.build.recipe","featureId":"project.build",
                "schema":{"id":"pack.build-recipe","version":1},
                "requiredPrimitives":["process.fixture-build"],
                "payload":{"steps":[{
                    "id":"publish","primitive":"process.fixture-build",
                    "isolatedOutputProperty":"ModsPath"
                }]}
            },
            {
                "slotId":"project.package.layout","featureId":"project.package",
                "schema":{"id":"pack.package-layout","version":1},
                "payload":{"requiredFiles":["{mod_id}/{mod_id}.dll"]}
            }
        ]
    });
    let bytes = serde_json::to_vec(&value).unwrap();
    GamePackLoader::load(&bytes, &sha256(&bytes)).unwrap()
}

fn truth(pack: &LoadedGamePack) -> VerifiedTruthSnapshot {
    let evidence = ["Root.Symbol", "Child.Symbol"]
        .into_iter()
        .map(|symbol| TruthEvidenceRecord {
            source_id: "fixture".into(),
            symbol: symbol.into(),
            purpose: "fixture readiness".into(),
            bounded_excerpt: format!("{symbol} is available."),
            relative_path: format!("sources/{symbol}.cs"),
        })
        .collect::<Vec<_>>();
    let index_bytes = serde_json::to_vec(&evidence).unwrap();
    let manifest = TruthSnapshotManifest::new(
        pack,
        vec![TruthSnapshotSource {
            id: "fixture".into(),
            kind: "source".into(),
            version: Some("1".into()),
            relative_path: "sources/fixture.cs".into(),
            sha256: sha256(b"fixture"),
            byte_length: 7,
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

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
}
