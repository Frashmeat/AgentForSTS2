use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use ats_adapters::{
    FileArtifactStore, FileItemRepository, FileProjectStager, FileProjectWriter,
    FileResourceRepository, ZipPackageWriter,
};
use ats_features::FeatureSpec;
use ats_features::composition::CompositionDraftRef;
use ats_features::composition_generate::{
    CompositionGenerateContext, CompositionGenerateDependencies, CompositionGenerateFeature,
    CompositionGenerateRequest, CompositionGenerateService,
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
    CancellationToken, FinishReason, ModelClient, ModelError, ModelRequestSnapshot, ModelResponse,
    ModelStream, ModelStreamEvent, PackageError, PackagePrepareRequest, PackageReport,
    PackageWriter, PendingPackageOutput, RunRecord, RunStatus, RunTransition, TokenUsage,
    ValidationError, ValidationReport, ValidationRequest, ValidationRunner, VersionedPayload,
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
}

#[async_trait]
impl ModelClient for QueueModel {
    async fn complete(
        &self,
        _: ModelRequestSnapshot,
        _: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
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
        };
        if self.reject {
            Err(ValidationError::Rejected(report))
        } else {
            Ok(report)
        }
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
    let (result, parent_run_id) = fixture.execute(false, false).await;
    let execution = match result {
        Ok(execution) => execution,
        Err(failure) => panic!(
            "composition generation failed: {}",
            failure.run_failure().code
        ),
    };

    assert_eq!(execution.result.node_count, 2);
    assert_eq!(execution.result.generated_file_count, 2);
    assert_eq!(
        execution.result.package.publication,
        PackagePublication::CompositionStaged
    );
    assert_eq!(execution.child_runs.len(), 6);
    assert!(
        execution
            .child_runs
            .iter()
            .all(|run| run.status() == RunStatus::Succeeded)
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
async fn validation_failure_keeps_real_project_and_artifacts_unchanged() {
    let fixture = Fixture::new();
    let (result, _) = fixture.execute(true, false).await;
    let failed = match result {
        Ok(_) => panic!("composition generation unexpectedly succeeded"),
        Err(failure) => failure,
    };

    assert_eq!(failed.run_failure().code.as_str(), "validation.rejected");
    assert_eq!(failed.child_runs.len(), 4);
    assert!(!fixture.project.join("Generated").exists());
    assert!(!fixture.project.join("packages/FixtureMod.zip").exists());
    assert!(!fixture.project.join("artifacts").exists());
    assert!(!fixture.project.join(".ats/composition-staging").exists());
}

#[tokio::test]
async fn package_commit_failure_rolls_back_real_project_and_cleans_stage() {
    let fixture = Fixture::new();
    let (result, _) = fixture.execute(false, true).await;
    let failed = match result {
        Ok(_) => panic!("composition generation unexpectedly succeeded"),
        Err(failure) => failure,
    };

    assert_eq!(failed.run_failure().code.as_str(), "artifact.write_failed");
    assert_eq!(failed.child_runs.len(), 6);
    assert!(!fixture.project.join("Generated").exists());
    assert!(!fixture.project.join("packages/FixtureMod.zip").exists());
    assert!(!fixture.project.join("artifacts").exists());
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
            ats_features::composition_generate::CompositionGenerateExecution,
            ats_features::composition_generate::CompositionGenerateFailure,
        >,
        ats_runtime::RunId,
    ) {
        let model = QueueModel {
            responses: Mutex::new(VecDeque::from([
                plan_response("child"),
                bundle_response("child"),
                plan_response("root"),
                bundle_response("root"),
            ])),
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
        let mut run = RunRecord::new(
            CompositionGenerateFeature::id(),
            VersionedPayload::from_typed(
                CompositionGenerateFeature::request_schema(),
                &self.request,
            )
            .unwrap(),
        );
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        let run_id = run.id().clone();
        let result = CompositionGenerateService::new(
            &plan_service,
            &single_service,
            &build_service,
            &package_service,
        )
        .execute(
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
            &mut run,
            self.request.clone(),
            CompositionGenerateContext {
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
            },
            &CancellationToken::new(),
        )
        .await;
        (result, run_id)
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
                "schema":{"id":"pack.mod-generate-single","version":4},
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
