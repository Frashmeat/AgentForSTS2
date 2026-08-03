use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;

use async_trait::async_trait;
use ats_adapters::{
    FileArtifactStore, FileProjectWriter, FileResourceRepository, RegisteredBuildRunner,
    RegisteredValidationRunner, ZipPackageWriter,
};
use ats_features::FeatureSpec;
use ats_features::mod_generate_batch::{
    BatchGenerateContext, BatchGenerateFeature, BatchGenerateRequest, BatchGenerateService,
};
use ats_features::mod_generate_complex::{
    ComplexGenerateContext, ComplexGenerateDependencies, ComplexGenerateFeature,
    ComplexGenerateRequest, ComplexGenerateService, ComplexPlanningItem,
};
use ats_features::mod_generate_single::{
    SingleGenerateDependencies, SingleGenerateFeature, SingleGenerateRequest, SingleGenerateService,
};
use ats_features::mod_plan::{ModPlanFeature, ModPlanRequest, ModPlanService, PlanItem};
use ats_features::project_build::{ProjectBuildFeature, ProjectBuildService};
use ats_features::project_package::{
    ProjectPackageContext, ProjectPackageFeature, ProjectPackageRequest, ProjectPackageService,
};
use ats_features::resource_prepare::ResourcePrepareFeature;
use ats_game_context::{
    ContributionResolver, GamePackLoader, LoadedGamePack, TruthEvidenceRecord, TruthSnapshotIndex,
    TruthSnapshotManifest, TruthSnapshotSource, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{PrimitiveId, Sha256Digest};
use ats_runtime::{
    ArtifactPublishRequest, ArtifactPublisher, CancellationToken, FinishReason, ModelClient,
    ModelError, ModelRequestSnapshot, ModelResponse, ModelStream, PublishedArtifact, RunRecord,
    RunStatus, RunTransition, TokenUsage, VersionedPayload,
};
use chrono::Utc;
use futures_util::stream;
use sha2::{Digest, Sha256};

struct CompositionModel {
    generated: Mutex<u32>,
    fail_generation_calls: Vec<u32>,
    cancel_after_completion: Option<CancellationToken>,
}

#[async_trait]
impl ModelClient for CompositionModel {
    async fn complete(
        &self,
        request: ModelRequestSnapshot,
        _: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        let content = if request.feature_id() == &ModPlanFeature::id() {
            serde_json::json!({
                "itemId":"planned_item",
                "itemType":"custom_code",
                "name":"Planned Item",
                "summary":"A composed fixture item",
                "behaviorIntent":["Expose a compiled fixture type"],
                "implementationConstraints":[],
                "evidenceRequirements":["A verified fixture type declaration"],
                "acceptanceCriteria":["The project publishes"]
            })
            .to_string()
        } else {
            let mut generated = self.generated.lock().unwrap();
            let call = *generated;
            *generated += 1;
            if self.fail_generation_calls.contains(&call) {
                return Ok(ModelResponse {
                    model: "fixture-model".into(),
                    content: "not-json".into(),
                    finish_reason: FinishReason::EndTurn,
                    usage: TokenUsage::default(),
                });
            }
            let class_name = format!("Generated{}", *generated);
            serde_json::json!({
                "files":[{"role":"source","content":format!("public class {class_name} {{}}") }],
                "acceptanceNotes":["fixture generated"]
            })
            .to_string()
        };
        if let Some(cancellation) = &self.cancel_after_completion {
            cancellation.cancel(ats_runtime::CancellationReason::User);
        }
        Ok(ModelResponse {
            model: "fixture-model".into(),
            content,
            finish_reason: FinishReason::EndTurn,
            usage: TokenUsage::default(),
        })
    }

    async fn stream(
        &self,
        _: ModelRequestSnapshot,
        _: &CancellationToken,
    ) -> Result<ModelStream, ModelError> {
        Ok(Box::pin(stream::empty()))
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    project: std::path::PathBuf,
    pack: LoadedGamePack,
    plan: VerifiedContributionSet,
    single: VerifiedContributionSet,
    batch: VerifiedContributionSet,
    complex: VerifiedContributionSet,
    resources: VerifiedContributionSet,
    build: VerifiedContributionSet,
    package: VerifiedContributionSet,
    truth: VerifiedTruthSnapshot,
    repository: FileResourceRepository,
    artifacts: FileArtifactStore,
    writer: FileProjectWriter,
    validator: RegisteredValidationRunner,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(
            project.join("Fixture.csproj"),
            br#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework><AssemblyName>FixtureMod</AssemblyName></PropertyGroup><ItemGroup><Compile Remove="artifacts/**" /><Compile Remove=".ats/**" /></ItemGroup></Project>"#,
        )
        .unwrap();
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let plan =
            resolve::<ModPlanFeature>(&pack, &[], ModPlanFeature::contribution_requirement());
        let single = resolve::<SingleGenerateFeature>(
            &pack,
            &["code.dotnet-validate"],
            SingleGenerateFeature::contribution_requirement(),
        );
        let batch = resolve::<BatchGenerateFeature>(
            &pack,
            &[],
            BatchGenerateFeature::contribution_requirement(),
        );
        let complex = resolve::<ComplexGenerateFeature>(
            &pack,
            &[],
            ComplexGenerateFeature::contribution_requirement(),
        );
        let resources = resolve::<ResourcePrepareFeature>(
            &pack,
            &["image.role-transform"],
            ResourcePrepareFeature::contribution_requirement(),
        );
        let build = resolve::<ProjectBuildFeature>(
            &pack,
            &["process.dotnet-publish"],
            ProjectBuildFeature::contribution_requirement(),
        );
        let package = resolve::<ProjectPackageFeature>(
            &pack,
            &[],
            ProjectPackageFeature::contribution_requirement(),
        );
        let truth = truth(&pack);
        let repository = FileResourceRepository::new(project.clone());
        let artifacts = FileArtifactStore::new(project.clone());
        Self {
            _temp: temp,
            project,
            pack,
            plan,
            single,
            batch,
            complex,
            resources,
            build,
            package,
            truth,
            repository,
            artifacts,
            writer: FileProjectWriter,
            validator: RegisteredValidationRunner,
        }
    }

    fn single_context(&self) -> BatchGenerateContext<'_> {
        BatchGenerateContext {
            pack: &self.pack,
            batch_contributions: &self.batch,
            single_contributions: &self.single,
            resource_contributions: &self.resources,
            truth: &self.truth,
            project_root: &self.project,
            project_context: "Isolated SDK-style project",
            custom_instructions: None,
            model: None,
        }
    }

    fn populate_delivery(&self) {
        for relative in [
            "BaseLib/BaseLib.dll",
            "BaseLib/BaseLib.pck",
            "BaseLib/BaseLib.json",
            "FixtureMod/FixtureMod.dll",
            "FixtureMod/FixtureMod.pck",
            "FixtureMod/FixtureMod.json",
        ] {
            let path = self.project.join("delivery").join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, relative.as_bytes()).unwrap();
        }
    }
}

#[tokio::test]
async fn batch_invokes_single_twice_without_parallel_codegen() {
    let fixture = Fixture::new();
    let model = CompositionModel {
        generated: Mutex::new(0),
        fail_generation_calls: Vec::new(),
        cancel_after_completion: None,
    };
    let single = SingleGenerateService::built_in().unwrap();
    let batch = BatchGenerateService::new(&single);
    let request = BatchGenerateRequest {
        items: vec![single_request("one"), single_request("two")],
        fail_fast: true,
    };
    let mut run = running_run::<BatchGenerateFeature, _>(&request);
    let result = batch
        .execute(
            single_dependencies(&fixture, &model),
            &mut run,
            request,
            fixture.single_context(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(run.status(), RunStatus::Succeeded);
    assert_eq!(result.result.succeeded, 2);
    assert_eq!(result.child_runs.len(), 2);
    assert!(
        result
            .child_runs
            .iter()
            .all(|child| child.status() == RunStatus::Succeeded)
    );
    assert_eq!(*model.generated.lock().unwrap(), 2);
}

#[tokio::test]
async fn batch_continue_and_fail_fast_preserve_child_outcomes() {
    for fail_fast in [false, true] {
        let fixture = Fixture::new();
        let model = CompositionModel {
            generated: Mutex::new(0),
            fail_generation_calls: vec![0],
            cancel_after_completion: None,
        };
        let single = SingleGenerateService::built_in().unwrap();
        let batch = BatchGenerateService::new(&single);
        let request = BatchGenerateRequest {
            items: vec![single_request("one"), single_request("two")],
            fail_fast,
        };
        let mut run = running_run::<BatchGenerateFeature, _>(&request);
        let result = batch
            .execute(
                single_dependencies(&fixture, &model),
                &mut run,
                request,
                fixture.single_context(),
                &CancellationToken::new(),
            )
            .await;
        if fail_fast {
            assert!(result.is_err());
            assert_eq!(*model.generated.lock().unwrap(), 1);
        } else {
            let execution = result.unwrap();
            assert_eq!(execution.result.succeeded, 1);
            assert_eq!(execution.result.failed, 1);
            assert_eq!(execution.child_runs[0].status(), RunStatus::Failed);
            assert_eq!(execution.child_runs[1].status(), RunStatus::Succeeded);
            assert_eq!(*model.generated.lock().unwrap(), 2);
        }
    }
}

#[tokio::test]
async fn batch_cancellation_stops_before_project_writes() {
    let fixture = Fixture::new();
    let cancellation = CancellationToken::new();
    let model = CompositionModel {
        generated: Mutex::new(0),
        fail_generation_calls: Vec::new(),
        cancel_after_completion: Some(cancellation.clone()),
    };
    let single = SingleGenerateService::built_in().unwrap();
    let batch = BatchGenerateService::new(&single);
    let request = BatchGenerateRequest {
        items: vec![single_request("one"), single_request("two")],
        fail_fast: false,
    };
    let mut run = running_run::<BatchGenerateFeature, _>(&request);
    let result = batch
        .execute(
            single_dependencies(&fixture, &model),
            &mut run,
            request,
            fixture.single_context(),
            &cancellation,
        )
        .await;
    assert!(result.is_err());
    assert_eq!(*model.generated.lock().unwrap(), 1);
    assert!(!fixture.project.join("Generated").exists());
    assert!(!fixture.project.join("artifacts").exists());
}

#[tokio::test]
async fn complex_composes_plan_batch_real_build_and_package() {
    let fixture = Fixture::new();
    fixture.populate_delivery();
    fs::create_dir(fixture.project.join("packages")).unwrap();
    let model = CompositionModel {
        generated: Mutex::new(0),
        fail_generation_calls: Vec::new(),
        cancel_after_completion: None,
    };
    let plan = ModPlanService::built_in().unwrap();
    let single = SingleGenerateService::built_in().unwrap();
    let batch = BatchGenerateService::new(&single);
    let build = ProjectBuildService;
    let package = ProjectPackageService;
    let complex = ComplexGenerateService::new(&plan, &batch, &build, &package);
    let request = ComplexGenerateRequest {
        mod_id: "FixtureMod".into(),
        planning_items: vec![ComplexPlanningItem {
            request: ModPlanRequest {
                requirements: "Create one fixture type".into(),
                item_type: Some("custom_code".into()),
            },
            artifact_id: "complex-item".into(),
            selected_resources: Vec::new(),
        }],
        fail_fast: true,
        package: ProjectPackageRequest {
            artifact_id: "complex-package".into(),
            mod_id: "FixtureMod".into(),
            source_relative_root: "delivery".into(),
            output_relative_path: "packages/FixtureMod.zip".into(),
            compression_level: Some(5),
        },
    };
    let mut run = running_run::<ComplexGenerateFeature, _>(&request);
    let execution = complex
        .execute(
            ComplexGenerateDependencies {
                model: &model,
                resources: &fixture.repository,
                writer: &fixture.writer,
                validator: &fixture.validator,
                artifacts: &fixture.artifacts,
                build_runner: &RegisteredBuildRunner,
                package_writer: &ZipPackageWriter,
            },
            &mut run,
            request,
            ComplexGenerateContext {
                pack: &fixture.pack,
                complex_contributions: &fixture.complex,
                plan_contributions: &fixture.plan,
                batch_contributions: &fixture.batch,
                single_contributions: &fixture.single,
                resource_contributions: &fixture.resources,
                build_contributions: &fixture.build,
                package_contributions: &fixture.package,
                truth: &fixture.truth,
                project_root: &fixture.project,
                project_context: "Isolated SDK-style project",
                custom_instructions: None,
                model: None,
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(run.status(), RunStatus::Succeeded);
    assert_eq!(execution.result.plans.len(), 1);
    assert_eq!(execution.result.batch.succeeded, 1);
    assert_eq!(execution.result.build.steps.len(), 1);
    assert_eq!(execution.result.package.report.file_count, 6);
    assert!(
        execution
            .child_runs
            .iter()
            .all(|child| child.status() == RunStatus::Succeeded)
    );
    let zip_file = fs::File::open(fixture.project.join("packages/FixtureMod.zip")).unwrap();
    let archive = zip::ZipArchive::new(zip_file).unwrap();
    assert_eq!(archive.len(), 6);
    assert!(!has_transaction_residue(&fixture.project));
}

#[derive(Debug, thiserror::Error)]
#[error("fixture artifact failure")]
struct ArtifactFailure;

struct FailingArtifacts;

impl ArtifactPublisher for FailingArtifacts {
    type Error = ArtifactFailure;

    fn publish(&self, _: ArtifactPublishRequest) -> Result<PublishedArtifact, Self::Error> {
        Err(ArtifactFailure)
    }

    fn remove_published_run(&self, _: &str, _: &ats_runtime::RunId) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[test]
fn package_artifact_failure_restores_previous_output() {
    let fixture = Fixture::new();
    fixture.populate_delivery();
    fs::create_dir(fixture.project.join("packages")).unwrap();
    let output = fixture.project.join("packages/FixtureMod.zip");
    fs::write(&output, b"previous-package").unwrap();
    let request = ProjectPackageRequest {
        artifact_id: "failed-package".into(),
        mod_id: "FixtureMod".into(),
        source_relative_root: "delivery".into(),
        output_relative_path: "packages/FixtureMod.zip".into(),
        compression_level: None,
    };
    let mut run = running_run::<ProjectPackageFeature, _>(&request);
    let result = ProjectPackageService.execute(
        &ZipPackageWriter,
        &FailingArtifacts,
        &mut run,
        request,
        ProjectPackageContext {
            pack: &fixture.pack,
            contributions: &fixture.package,
            project_root: &fixture.project,
        },
        &CancellationToken::new(),
    );
    assert!(result.is_err());
    assert_eq!(fs::read(output).unwrap(), b"previous-package");
    assert!(!has_transaction_residue(&fixture.project));
}

fn single_dependencies<'a>(
    fixture: &'a Fixture,
    model: &'a CompositionModel,
) -> SingleGenerateDependencies<
    'a,
    CompositionModel,
    FileResourceRepository,
    FileProjectWriter,
    RegisteredValidationRunner,
    FileArtifactStore,
> {
    SingleGenerateDependencies {
        model,
        resources: &fixture.repository,
        writer: &fixture.writer,
        validator: &fixture.validator,
        artifacts: &fixture.artifacts,
    }
}

fn single_request(id: &str) -> SingleGenerateRequest {
    SingleGenerateRequest {
        artifact_id: format!("artifact-{id}"),
        mod_id: "FixtureMod".into(),
        plan: PlanItem {
            item_id: id.into(),
            item_type: "custom_code".into(),
            name: format!("Item {id}"),
            summary: "A batch fixture item".into(),
            behavior_intent: vec!["Expose one fixture type".into()],
            implementation_constraints: Vec::new(),
            evidence_requirements: vec!["A verified fixture type declaration".into()],
            required_resource_roles: Vec::new(),
            acceptance_criteria: vec!["The project compiles".into()],
        },
        selected_resources: Vec::new(),
    }
}

fn resolve<S: FeatureSpec>(
    pack: &LoadedGamePack,
    primitives: &[&str],
    requirement: ats_game_context::ContributionRequirement,
) -> VerifiedContributionSet {
    ContributionResolver::new(primitives.iter().map(|id| PrimitiveId::parse(*id).unwrap()))
        .resolve(pack, &S::id(), &[requirement])
        .unwrap()
}

fn running_run<S: FeatureSpec, T: serde::Serialize>(request: &T) -> RunRecord {
    let payload = VersionedPayload::from_typed(S::request_schema(), request).unwrap();
    let mut run = RunRecord::new(S::id(), payload);
    run.apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    run
}

fn truth(pack: &LoadedGamePack) -> VerifiedTruthSnapshot {
    let evidence = vec![TruthEvidenceRecord {
        source_id: "fixture-source".into(),
        symbol: "ICustomModel".into(),
        purpose: "Prove the fixture type".into(),
        bounded_excerpt: "public class Fixture".into(),
        relative_path: "sources/fixture.cs".into(),
    }];
    let index_bytes = serde_json::to_vec(&evidence).unwrap();
    let manifest = TruthSnapshotManifest::new(
        pack,
        vec![TruthSnapshotSource {
            id: "fixture-source".into(),
            kind: "source".into(),
            version: Some("1".into()),
            relative_path: "sources/fixture.cs".into(),
            sha256: sha256(b"fixture-source"),
            byte_length: 14,
        }],
        vec![TruthSnapshotIndex {
            id: "fixture-index".into(),
            provider: PrimitiveId::parse("truth.fixture-index").unwrap(),
            relative_path: "indexes/fixture.json".into(),
            sha256: sha256(&index_bytes),
            record_count: 1,
        }],
        BTreeMap::from([("fixture-tool".into(), "1".into())]),
        Utc::now(),
    )
    .unwrap();
    VerifiedTruthSnapshot::verify(
        pack,
        manifest,
        BTreeMap::from([("fixture-index".into(), evidence)]),
    )
    .unwrap()
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
}

fn has_transaction_residue(project: &std::path::Path) -> bool {
    for relative in [".ats/transactions", ".ats/package-transactions"] {
        let root = project.join(relative);
        if root
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_some())
        {
            return true;
        }
    }
    false
}
