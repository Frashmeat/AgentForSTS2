use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use ats_adapters::{
    FileArtifactStore, FileExecutionGraphRepository, FileItemRepository, FileProjectStager,
    FileProjectWriter, FileResourceRepository, FileRunRepository, ZipPackageWriter,
};
use ats_features::FeatureSpec;
use ats_features::composition::CompositionDraftRef;
use ats_features::composition_generate::{
    CompositionGenerateContext, CompositionGenerateDependencies, CompositionGenerateFeature,
    CompositionGenerateRequest, CompositionGenerateService, ItemAdjustment,
};
use ats_features::mod_generate_single::{SingleGenerateFeature, SingleGenerateService};
use ats_features::mod_plan::{ModPlanFeature, ModPlanService};
use ats_features::project_build::{ProjectBuildFeature, ProjectBuildService};
use ats_features::project_package::{
    PackagePublication, ProjectPackageFeature, ProjectPackageRequest, ProjectPackageService,
};
use ats_features::resource_prepare::ResourcePrepareFeature;
use ats_game_context::{
    ContributionResolver, GamePackLoader, LoadedGamePack, TruthEvidenceRecord, TruthSnapshotIndex,
    TruthSnapshotManifest, TruthSnapshotSource, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{
    CompositionDraftId, CompositionId, CompositionParameterId, CompositionProfileId, ItemId,
    ItemReferenceSlotId, ItemTypeId, PrimitiveId, Sha256Digest,
};
use ats_runtime::{
    ArtifactManifest, BuildError, BuildRunner, BuildStepReport, BuildStepRequest,
    CancellationToken, ExecutionGraphRecord, ExecutionGraphRecovery, ExecutionGraphRepository,
    ExecutionGraphRepositoryError, FinishReason, ModelClient, ModelError, ModelRequestSnapshot,
    ModelResponse, ModelStream, ModelStreamEvent, PackageError, PackagePrepareRequest,
    PackageReport, PackageWriter, PendingPackageOutput, RunRecord, RunRepository, RunStatus,
    RunTransition, TokenUsage, ValidationError, ValidationIssue, ValidationIssueRepairability,
    ValidationIssueSeverity, ValidationReport, ValidationRequest, ValidationRunner,
    VersionedPayload,
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

struct RepairOnceValidation {
    calls: AtomicUsize,
}

#[async_trait]
impl ValidationRunner for RepairOnceValidation {
    async fn validate(
        &self,
        _: ValidationRequest,
        _: &CancellationToken,
    ) -> Result<ValidationReport, ValidationError> {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            let issue = ValidationIssue {
                validator_id: "code.dotnet-validate".into(),
                code: "CS0246".into(),
                severity: ValidationIssueSeverity::Error,
                relative_path: Some("Generated/fixture-child.cs".into()),
                line: Some(1),
                column: Some(1),
                message: "The type 'ImaginaryType' could not be found.".into(),
                symbol: Some("ImaginaryType".into()),
                repairability: ValidationIssueRepairability::GeneratedContent,
                fingerprint: Sha256Digest::parse("d".repeat(64)).unwrap(),
            };
            return Err(ValidationError::Rejected(ValidationReport {
                exit_code: 1,
                stdout_tail: String::new(),
                stderr_tail: String::new(),
                issues: vec![issue],
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

struct MultiItemRepairOnceValidation {
    calls: AtomicUsize,
}

#[async_trait]
impl ValidationRunner for MultiItemRepairOnceValidation {
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

struct CrashAfterRepairedSingleCheckpoint {
    inner: FileExecutionGraphRepository,
    interrupted: AtomicBool,
}

impl CrashAfterRepairedSingleCheckpoint {
    fn new(project_root: &Path) -> Self {
        Self {
            inner: FileExecutionGraphRepository::new(project_root.to_path_buf()),
            interrupted: AtomicBool::new(false),
        }
    }
}

impl ExecutionGraphRepository for CrashAfterRepairedSingleCheckpoint {
    fn create_claimed(
        &self,
        graph: &ExecutionGraphRecord,
        run_id: &ats_runtime::RunId,
    ) -> Result<(), ExecutionGraphRepositoryError> {
        self.inner.create_claimed(graph, run_id)
    }

    fn get(
        &self,
        id: &ats_kernel::ExecutionGraphId,
    ) -> Result<ExecutionGraphRecord, ExecutionGraphRepositoryError> {
        self.inner.get(id)
    }

    fn compare_and_set(
        &self,
        expected_revision: u64,
        next: &ExecutionGraphRecord,
    ) -> Result<(), ExecutionGraphRepositoryError> {
        let repaired_single_was_persisted = next.repair_campaign().is_some_and(|campaign| {
            campaign.current_target > 0
                && campaign.targets.iter().any(|target| {
                    target.status == ats_runtime::ExecutionRepairTargetStatus::Completed
                })
        });
        self.inner.compare_and_set(expected_revision, next)?;
        if repaired_single_was_persisted && !self.interrupted.swap(true, Ordering::SeqCst) {
            return Err(ExecutionGraphRepositoryError::Conflict);
        }
        Ok(())
    }

    fn list(&self) -> Result<Vec<ExecutionGraphRecord>, ExecutionGraphRepositoryError> {
        self.inner.list()
    }

    fn recover_structure(
        &self,
        runs: &dyn RunRepository,
    ) -> Result<ExecutionGraphRecovery, ExecutionGraphRepositoryError> {
        self.inner.recover_structure(runs)
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
        execution.result.package.publication,
        PackagePublication::CompositionStaged
    );
    assert_eq!(child_runs.len(), 6);
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
    assert_eq!(child_runs.len(), 7);
    assert_eq!(
        child_runs
            .iter()
            .filter(|run| run.status == RunStatus::Failed)
            .count(),
        1
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
        .filter(|node| node.role_id == "mod.generate.single")
        .map(|node| {
            (
                node.node_id.clone(),
                node.active_checkpoint.as_ref().unwrap().sha256.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.fixture-validate").unwrap(),
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
    let single = resolve::<SingleGenerateFeature>(
        &resolver,
        &fixture.pack,
        SingleGenerateFeature::contribution_requirement(),
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
    let single_service = SingleGenerateService::built_in().unwrap();
    let service = CompositionGenerateService::new(
        &plan_service,
        &single_service,
        &ProjectBuildService,
        &ProjectPackageService,
    );
    let context = || CompositionGenerateContext {
        pack: &fixture.pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        single_contributions: &single,
        resource_contributions: &resource,
        build_contributions: &build,
        package_contributions: &package,
        truth: &fixture.truth,
        project_root: &fixture.project,
        project_context: "Fixture project",
        custom_instructions: None,
        model: None,
    };
    let adjustment_run_id = ats_runtime::RunId::new();
    let child = fixture
        .items
        .load_current(&ItemId::parse("fixture-child").unwrap())
        .unwrap();
    let adjustment = ItemAdjustment::new(
        child.definition.item_id.clone(),
        child.definition_hash,
        "Make this item clearer without changing its identity.",
        Utc::now(),
    )
    .unwrap();
    let start = service
        .prepare_staged_adjustment(
            source_graph.clone(),
            source_revision,
            adjustment_run_id.clone(),
            adjustment,
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
        responses: Mutex::new(VecDeque::from([repaired_bundle_response("child")])),
        requests: AtomicUsize::new(0),
    };
    let validator = MultiItemRepairOnceValidation {
        calls: AtomicUsize::new(1),
    };
    let execution = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &model,
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
        .await
        .unwrap();
    assert_eq!(model.requests.load(Ordering::SeqCst), 1);
    assert_eq!(validator.calls.load(Ordering::SeqCst), 2);
    assert_eq!(run.status(), RunStatus::Succeeded);
    let derived = graphs
        .get(execution.result.execution_graph_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(derived.semantic_request_count(), 1);
    let changed = derived
        .nodes()
        .values()
        .filter(|node| node.role_id == "mod.generate.single")
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
        PrimitiveId::parse("code.fixture-validate").unwrap(),
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
    let single = resolve::<SingleGenerateFeature>(
        &resolver,
        &fixture.pack,
        SingleGenerateFeature::contribution_requirement(),
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
    let single_service = SingleGenerateService::built_in().unwrap();
    let service = CompositionGenerateService::new(
        &plan_service,
        &single_service,
        &ProjectBuildService,
        &ProjectPackageService,
    );
    let error = service
        .prepare_staged_adjustment(
            graph.clone(),
            graph.revision(),
            ats_runtime::RunId::new(),
            ItemAdjustment::new(
                ItemId::parse("fixture-child").unwrap(),
                Sha256Digest::parse("f".repeat(64)).unwrap(),
                "Change this item.",
                Utc::now(),
            )
            .unwrap(),
            CompositionGenerateContext {
                pack: &fixture.pack,
                composition_contributions: &composition,
                plan_contributions: &plan,
                single_contributions: &single,
                resource_contributions: &resource,
                build_contributions: &build,
                package_contributions: &package,
                truth: &fixture.truth,
                project_root: &fixture.project,
                project_context: "Fixture project",
                custom_instructions: None,
                model: None,
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
    assert_eq!(child_runs.len(), 4);
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
    assert_eq!(child_runs.len(), 6);
    assert!(!fixture.project.join("Generated").exists());
    assert!(!fixture.project.join("packages/FixtureMod.zip").exists());
    assert!(!fixture.project.join("artifacts").exists());
    assert!(!fixture.project.join(".ats/composition-staging").exists());
}

#[tokio::test]
async fn staged_generation_resumes_only_the_failed_single_node() {
    let fixture = Fixture::new();
    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.fixture-validate").unwrap(),
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
    let single = resolve::<SingleGenerateFeature>(
        &resolver,
        &fixture.pack,
        SingleGenerateFeature::contribution_requirement(),
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
    let single_service = SingleGenerateService::built_in().unwrap();
    let build_service = ProjectBuildService;
    let package_service = ProjectPackageService;
    let service = CompositionGenerateService::new(
        &plan_service,
        &single_service,
        &build_service,
        &package_service,
    );
    let context = || CompositionGenerateContext {
        pack: &fixture.pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        single_contributions: &single,
        resource_contributions: &resource,
        build_contributions: &build,
        package_contributions: &package,
        truth: &fixture.truth,
        project_root: &fixture.project,
        project_context: "Fixture project",
        custom_instructions: None,
        model: None,
    };
    let graphs = FileExecutionGraphRepository::new(fixture.project.clone());
    let runs = FileRunRepository::new(fixture.project.clone()).unwrap();
    let first_run_id = ats_runtime::RunId::new();
    let start = service
        .prepare_staged_start(
            fixture.request.clone(),
            context(),
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
            context(),
            &CancellationToken::new(),
        )
        .await;
    let first = match first {
        Ok(_) => panic!("staged generation unexpectedly succeeded"),
        Err(error) => error,
    };
    assert_eq!(first.run_failure().code.as_str(), "model.output_invalid");
    assert_eq!(first_model.requests.load(Ordering::SeqCst), 3);
    let paused = graphs.get(start.graph.id()).unwrap();
    assert_eq!(paused.status(), ats_runtime::ExecutionGraphStatus::Paused);
    let persisted_graph = serde_json::to_string(&paused).unwrap();
    assert!(!persisted_graph.contains("Fixture project"));
    assert!(!persisted_graph.contains("messages"));
    assert!(!persisted_graph.contains("response_format"));
    assert!(!persisted_graph.contains("provider"));
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
        .prepare_staged_resume(paused, paused_revision, second_run_id.clone(), context())
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
    let execution = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &second_model,
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
            context(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(second_model.requests.load(Ordering::SeqCst), 3);
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
            context(),
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
            context(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(no_model.requests.load(Ordering::SeqCst), 0);
    assert_eq!(reconciled_execution.result, execution.result);
    assert_eq!(third_run.status(), RunStatus::Succeeded);
}

#[tokio::test]
async fn staged_validation_repairs_multiple_items_serially_then_revalidates_the_whole_closure() {
    let fixture = Fixture::new();
    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.fixture-validate").unwrap(),
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
    let single = resolve::<SingleGenerateFeature>(
        &resolver,
        &fixture.pack,
        SingleGenerateFeature::contribution_requirement(),
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
    let single_service = SingleGenerateService::built_in().unwrap();
    let service = CompositionGenerateService::new(
        &plan_service,
        &single_service,
        &ProjectBuildService,
        &ProjectPackageService,
    );
    let context = || CompositionGenerateContext {
        pack: &fixture.pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        single_contributions: &single,
        resource_contributions: &resource,
        build_contributions: &build,
        package_contributions: &package,
        truth: &fixture.truth,
        project_root: &fixture.project,
        project_context: "Fixture project",
        custom_instructions: None,
        model: None,
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
            "not-json".into(),
            repaired_bundle_response("child"),
            repaired_bundle_response("root"),
        ])),
        requests: AtomicUsize::new(0),
    };
    let validator = MultiItemRepairOnceValidation {
        calls: AtomicUsize::new(0),
    };
    let execution = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &model,
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
        .await
        .unwrap();
    assert_eq!(model.requests.load(Ordering::SeqCst), 7);
    assert_eq!(validator.calls.load(Ordering::SeqCst), 2);
    let graph = graphs
        .get(execution.result.execution_graph_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(graph.status(), ats_runtime::ExecutionGraphStatus::Succeeded);
    assert_eq!(graph.semantic_request_count(), 3);
    assert!(graph.repair_campaign().is_none());
    let feedback_states = graph
        .nodes()
        .values()
        .filter_map(|node| node.feedback_state.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(feedback_states.len(), 1);
    let feedback = feedback_states[0];
    assert_eq!(
        feedback.phase,
        ats_runtime::ExecutionFeedbackPhase::OutputContract
    );
    assert!(feedback.candidate_sha256.is_some());
    assert!(feedback.checkpoint_hash.is_some());
    assert!(!fixture.project.join(".ats/composition-staging").exists());
}

#[tokio::test]
async fn repaired_single_checkpoint_resumes_before_finalize_without_another_model_request() {
    let fixture = Fixture::new();
    let resolver = ContributionResolver::new([
        PrimitiveId::parse("code.fixture-validate").unwrap(),
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
    let single = resolve::<SingleGenerateFeature>(
        &resolver,
        &fixture.pack,
        SingleGenerateFeature::contribution_requirement(),
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
    let single_service = SingleGenerateService::built_in().unwrap();
    let service = CompositionGenerateService::new(
        &plan_service,
        &single_service,
        &ProjectBuildService,
        &ProjectPackageService,
    );
    let context = || CompositionGenerateContext {
        pack: &fixture.pack,
        composition_contributions: &composition,
        plan_contributions: &plan,
        single_contributions: &single,
        resource_contributions: &resource,
        build_contributions: &build,
        package_contributions: &package,
        truth: &fixture.truth,
        project_root: &fixture.project,
        project_context: "Fixture project",
        custom_instructions: None,
        model: None,
    };
    let crashing_graphs = CrashAfterRepairedSingleCheckpoint::new(&fixture.project);
    let runs = FileRunRepository::new(fixture.project.clone()).unwrap();
    let first_run_id = ats_runtime::RunId::new();
    let start = service
        .prepare_staged_start(
            fixture.request.clone(),
            context(),
            &fixture.items,
            &fixture.resources,
            first_run_id.clone(),
        )
        .unwrap();
    crashing_graphs
        .create_claimed(&start.graph, &first_run_id)
        .unwrap();
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
            bundle_response("child"),
            plan_response("root"),
            bundle_response("root"),
            repaired_bundle_response("child"),
        ])),
        requests: AtomicUsize::new(0),
    };
    let validator = RepairOnceValidation {
        calls: AtomicUsize::new(0),
    };
    let interrupted = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &first_model,
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
            &crashing_graphs,
            &mut first_run,
            start.request,
            context(),
            &CancellationToken::new(),
        )
        .await;
    let interrupted = match interrupted {
        Ok(_) => panic!("staged generation unexpectedly survived the injected crash window"),
        Err(error) => error,
    };
    assert_eq!(
        interrupted.run_failure().code.as_str(),
        "composition.execution.conflict"
    );
    assert_eq!(first_model.requests.load(Ordering::SeqCst), 5);
    assert_eq!(validator.calls.load(Ordering::SeqCst), 1);

    let persisted = crashing_graphs.get(start.graph.id()).unwrap();
    let stale_finalize_hash = persisted
        .nodes()
        .values()
        .find(|node| node.role_id == "composition.finalize")
        .and_then(|node| node.active_checkpoint.as_ref())
        .map(|checkpoint| checkpoint.sha256.clone())
        .unwrap();
    assert_eq!(persisted.semantic_request_count(), 1);
    assert_eq!(persisted.repair_campaign().unwrap().current_target, 1);

    let reopened_graphs = FileExecutionGraphRepository::new(fixture.project.clone());
    assert_eq!(
        reopened_graphs.recover_structure(&runs).unwrap().recovered,
        1
    );
    let paused = reopened_graphs.get(start.graph.id()).unwrap();
    assert_eq!(paused.status(), ats_runtime::ExecutionGraphStatus::Paused);
    let paused_revision = paused.revision();
    let second_run_id = ats_runtime::RunId::new();
    let resumed = service
        .prepare_staged_resume(paused, paused_revision, second_run_id.clone(), context())
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
    let no_model = QueueModel {
        responses: Mutex::new(VecDeque::new()),
        requests: AtomicUsize::new(0),
    };
    let execution = service
        .execute_staged(
            CompositionGenerateDependencies {
                model: &no_model,
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
            &reopened_graphs,
            &mut second_run,
            resumed.request,
            context(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(no_model.requests.load(Ordering::SeqCst), 0);
    assert_eq!(validator.calls.load(Ordering::SeqCst), 2);
    assert_eq!(second_run.status(), RunStatus::Succeeded);
    let succeeded = reopened_graphs
        .get(execution.result.execution_graph_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(
        succeeded.status(),
        ats_runtime::ExecutionGraphStatus::Succeeded
    );
    let rebuilt_finalize_hash = succeeded
        .nodes()
        .values()
        .find(|node| node.role_id == "composition.finalize")
        .and_then(|node| node.active_checkpoint.as_ref())
        .map(|checkpoint| checkpoint.sha256.clone())
        .unwrap();
    assert_ne!(rebuilt_finalize_hash, stale_finalize_hash);
    assert!(fixture.project.join("Generated/fixture-child.cs").is_file());
    assert!(fixture.project.join("Generated/fixture-root.cs").is_file());
    assert!(fixture.project.join("packages/FixtureMod.zip").is_file());
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
            package: ProjectPackageRequest {
                artifact_id: "fixture-composition".into(),
                mod_id: "FixtureMod".into(),
                source_relative_root: "delivery".into(),
                output_relative_path: "packages/FixtureMod.zip".into(),
                compression_level: Some(6),
            },
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
            PrimitiveId::parse("code.fixture-validate").unwrap(),
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
        let single = resolve::<SingleGenerateFeature>(
            &resolver,
            &self.pack,
            SingleGenerateFeature::contribution_requirement(),
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
        let single_service = SingleGenerateService::built_in().unwrap();
        let build_service = ProjectBuildService;
        let package_service = ProjectPackageService;
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
            single_contributions: &single,
            resource_contributions: &resource,
            build_contributions: &build,
            package_contributions: &package,
            truth: &self.truth,
            project_root: &self.project,
            project_context: "Fixture project",
            custom_instructions: None,
            model: None,
        };
        let service = CompositionGenerateService::new(
            &plan_service,
            &single_service,
            &build_service,
            &package_service,
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
    serde_json::json!({
        "files":{"source":format!("public class Fixture{item_type} {{}}")},
        "acceptanceNotes":["Generated in the isolated composition stage."]
    })
    .to_string()
}

fn repaired_bundle_response(item_type: &str) -> String {
    serde_json::json!({
        "files":{"source":format!("public class Fixture{item_type} {{ public bool Repaired => true; }}")},
        "acceptanceNotes":["Repaired from a registered validation diagnostic."]
    })
    .to_string()
}

fn pack() -> LoadedGamePack {
    let value = serde_json::json!({
        "schemaVersion":4,
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
        "contributions":[
            {
                "slotId":"composition.generate","featureId":"composition.generate",
                "schema":{"id":"pack.composition-generate","version":1},
                "payload":{"compose":["mod.plan","mod.generate.single","project.build","project.package"]}
            },
            {
                "slotId":"mod.plan.guidance","featureId":"mod.plan",
                "schema":{"id":"pack.mod-plan-guidance","version":3},
                "payload":{"guidance":["Use fixture evidence."]}
            },
            {
                "slotId":"mod.generate.single","featureId":"mod.generate.single",
                "schema":{"id":"pack.mod-generate-single","version":5},
                "requiredPrimitives":["code.fixture-validate"],
                "payload":{
                    "validationPrimitive":"code.fixture-validate",
                    "guidance":["Generate fixture source."],
                    "itemTypes":[
                        {"id":"root","guidance":["Generate root source."],"generatedFiles":[{"role":"source","targetPath":"Generated/{item_id}.cs"}]},
                        {"id":"child","guidance":["Generate child source."],"generatedFiles":[{"role":"source","targetPath":"Generated/{item_id}.cs"}]}
                    ]
                }
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
