use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;

use async_trait::async_trait;
use ats_adapters::{
    FileArtifactStore, FileProjectWriter, FileResourceRepository, RegisteredValidationRunner,
};
use ats_features::FeatureSpec;
use ats_features::mod_generate_single::{
    SingleGenerateContext, SingleGenerateDependencies, SingleGenerateFeature,
    SingleGenerateRequest, SingleGenerateService,
};
use ats_features::mod_plan::PlanItem;
use ats_features::resource_prepare::ResourcePrepareFeature;
use ats_game_context::{
    ContributionResolver, GamePackLoader, TruthEvidenceRecord, TruthSnapshotIndex,
    TruthSnapshotManifest, TruthSnapshotSource, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{
    ItemFieldId, ItemId, ItemTypeId, LocaleId, PrimitiveId, ResourceId, Sha256Digest,
};
use ats_runtime::{
    ArtifactPublishRequest, ArtifactPublisher, CancellationReason, CancellationToken, FinishReason,
    ModelClient, ModelError, ModelRequestSnapshot, ModelResponse, ModelStream, PublishedArtifact,
    RunRecord, RunStatus, RunTransition, TokenUsage, ValidationError, ValidationReport,
    ValidationRequest, ValidationRunner, VersionedPayload,
};
use ats_workspace::{
    ItemDefinition, ItemFieldValue, ItemLocalization, ItemResourceBinding, LocalizationStatus,
    PreparedResourceMedia, ResourceBytesIngestRequest, ResourceOrigin, ResourceRepository,
    ResourceVersionProvenance, StoredItemDefinition,
};
use chrono::Utc;
use futures_util::stream;
use sha2::{Digest, Sha256};

struct FixtureModel {
    source: &'static str,
    snapshots: Mutex<Vec<ModelRequestSnapshot>>,
}

#[async_trait]
impl ModelClient for FixtureModel {
    async fn complete(
        &self,
        request: ModelRequestSnapshot,
        _: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        self.snapshots.lock().unwrap().push(request);
        Ok(ModelResponse {
            model: "fixture-model".into(),
            content: serde_json::json!({
                "files": {
                    "source": self.source,
                    "localization.eng": "{}",
                    "localization.zhs": "{}"
                },
                "acceptanceNotes": ["fixture bundle assembled"]
            })
            .to_string(),
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

#[derive(Debug, thiserror::Error)]
#[error("fixture publication failure")]
struct FixtureArtifactError;

struct FailingArtifactPublisher;

impl ArtifactPublisher for FailingArtifactPublisher {
    type Error = FixtureArtifactError;

    fn publish(&self, _: ArtifactPublishRequest) -> Result<PublishedArtifact, Self::Error> {
        Err(FixtureArtifactError)
    }

    fn remove_published_run(&self, _: &str, _: &ats_runtime::RunId) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct CancellingValidator;

#[async_trait]
impl ValidationRunner for CancellingValidator {
    async fn validate(
        &self,
        _: ValidationRequest,
        cancellation: &CancellationToken,
    ) -> Result<ValidationReport, ValidationError> {
        cancellation.cancel(CancellationReason::User);
        Ok(ValidationReport {
            exit_code: 0,
            stdout_tail: String::new(),
            stderr_tail: String::new(),
        })
    }
}

struct Fixture {
    _temp: tempfile::TempDir,
    project: std::path::PathBuf,
    pack: ats_game_context::LoadedGamePack,
    generate_contributions: VerifiedContributionSet,
    resource_contributions: VerifiedContributionSet,
    truth: VerifiedTruthSnapshot,
    resources: FileResourceRepository,
    request: SingleGenerateRequest,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(
            project.join("Fixture.csproj"),
            br#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>"#,
        )
        .unwrap();
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let generate_contributions =
            ContributionResolver::new([PrimitiveId::parse("code.dotnet-validate").unwrap()])
                .resolve(
                    &pack,
                    &SingleGenerateFeature::id(),
                    &[SingleGenerateFeature::contribution_requirement()],
                )
                .unwrap();
        let resource_contributions =
            ContributionResolver::new([PrimitiveId::parse("image.role-transform").unwrap()])
                .resolve(
                    &pack,
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )
                .unwrap();
        let truth = truth(&pack);
        let resources = FileResourceRepository::new(project.clone());
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture_relic").unwrap(),
            ItemTypeId::parse("relic").unwrap(),
        );
        definition.canonical_fields.insert(
            ItemFieldId::parse("rarity").unwrap(),
            ItemFieldValue::Choice("common".into()),
        );
        definition.behavior_intent = vec!["Expose a testable fixture type".into()];
        for (locale, name) in [("eng", "Fixture Relic"), ("zhs", "Fixture Relic ZHS")] {
            definition.localizations.insert(
                LocaleId::parse(locale).unwrap(),
                ItemLocalization {
                    name: name.into(),
                    description: "A compile-test fixture relic.".into(),
                    status: LocalizationStatus::Confirmed,
                    translated_from: None,
                },
            );
        }
        for role in ["relic.normal", "relic.outline", "relic.big"] {
            let (width, height) = if role == "relic.big" {
                (256, 256)
            } else {
                (128, 128)
            };
            let candidate = resources
                .ingest_bytes(ResourceBytesIngestRequest {
                    logical_role: role.into(),
                    origin: ResourceOrigin::UserUpload,
                    file_name: format!("{}.png", role.replace('.', "-")),
                    media: PreparedResourceMedia {
                        media_type: "image/png".into(),
                        width,
                        height,
                        has_alpha: true,
                        bytes: format!("fixture-{role}").into_bytes(),
                    },
                    provenance: ResourceVersionProvenance::Original,
                })
                .unwrap();
            let asset = resources
                .select(candidate.resource_id(), &candidate.versions()[0].id)
                .unwrap();
            definition.resource_bindings.insert(
                ResourceId::parse(role).unwrap(),
                ItemResourceBinding {
                    resource_id: asset.resource_id().clone(),
                    selected_version: asset.selected_version().unwrap().clone(),
                },
            );
        }
        let definition = StoredItemDefinition {
            definition_hash: definition.definition_hash().unwrap(),
            definition,
        };
        let request = SingleGenerateRequest {
            artifact_id: "fixture-relic".into(),
            mod_id: "FixtureMod".into(),
            plan: PlanItem {
                item_id: "fixture_relic".into(),
                item_type: "relic".into(),
                name: "Fixture Relic".into(),
                summary: "A compile-test fixture relic".into(),
                behavior_intent: vec!["Expose a testable fixture type".into()],
                implementation_constraints: vec![],
                evidence_requirements: vec!["A verified relic declaration".into()],
                required_resource_roles: vec![
                    "relic.normal".into(),
                    "relic.outline".into(),
                    "relic.big".into(),
                ],
                acceptance_criteria: vec!["The generated project compiles".into()],
            },
            definition,
        };
        Self {
            _temp: temp,
            project,
            pack,
            generate_contributions,
            resource_contributions,
            truth,
            resources,
            request,
        }
    }

    fn run(&self) -> RunRecord {
        let payload =
            VersionedPayload::from_typed(SingleGenerateFeature::request_schema(), &self.request)
                .unwrap();
        let mut run = RunRecord::new(SingleGenerateFeature::id(), payload);
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        run
    }

    fn context(&self) -> SingleGenerateContext<'_> {
        SingleGenerateContext {
            pack: &self.pack,
            contributions: &self.generate_contributions,
            resource_contributions: &self.resource_contributions,
            truth: &self.truth,
            project_root: &self.project,
            project_context: "An isolated SDK-style test project.",
            custom_instructions: Some("CUSTOM-CANARY"),
            model: None,
        }
    }

    fn assert_clean_rollback(&self, run: &RunRecord) {
        assert_eq!(
            fs::read_to_string(self.project.join("Generated/fixture_relic.cs")).unwrap(),
            "public class Existing {}"
        );
        assert!(!self.project.join("FixtureMod").exists());
        assert!(
            !self
                .project
                .join("artifacts/fixture-relic/runs")
                .join(run.id().as_str())
                .exists()
        );
        assert!(
            !self
                .project
                .join(".ats/transactions")
                .join(run.id().as_str())
                .exists()
        );
    }
}

#[tokio::test]
async fn real_compile_artifact_and_run_chain_succeeds_without_staging_residue() {
    let fixture = Fixture::new();
    let model = FixtureModel {
        source: "public class FixtureRelic {}",
        snapshots: Mutex::new(Vec::new()),
    };
    let writer = FileProjectWriter;
    let validator = RegisteredValidationRunner;
    let artifacts = FileArtifactStore::new(fixture.project.clone());
    let mut run = fixture.run();
    let execution = SingleGenerateService::built_in()
        .unwrap()
        .execute(
            SingleGenerateDependencies {
                model: &model,
                resources: &fixture.resources,
                writer: &writer,
                validator: &validator,
                artifacts: &artifacts,
            },
            &mut run,
            fixture.request.clone(),
            fixture.context(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(run.status(), RunStatus::Succeeded);
    assert_eq!(execution.result.generated_file_count, 3);
    let manifest = fixture
        .project
        .join(&execution.result.artifact_manifest_ref);
    let bytes = fs::read(&manifest).unwrap();
    assert_eq!(sha256(&bytes), execution.result.manifest_sha256);
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["schemaVersion"], 3);
    assert_eq!(value["files"].as_array().unwrap().len(), 6);
    assert_eq!(
        value["featureExtension"]["payload"]["definitionHash"],
        fixture.request.definition.definition_hash.as_str()
    );
    assert!(value["provenance"].as_array().unwrap().iter().any(|entry| {
        entry["schema"]["id"] == "artifact.item-definition-provenance"
            && entry["payload"]["definitionHash"]
                == fixture.request.definition.definition_hash.as_str()
    }));
    let snapshots = model.snapshots.lock().unwrap();
    let request = snapshots[0].request();
    let files_contract = &request.output_contract.json_schema["properties"]["files"];
    assert_eq!(
        files_contract["required"],
        serde_json::json!(["source", "localization.eng", "localization.zhs"])
    );
    assert_eq!(files_contract["additionalProperties"], false);
    assert_eq!(files_contract["properties"].as_object().unwrap().len(), 3);
    let rendered_contract =
        serde_json::to_string_pretty(&request.output_contract.json_schema).unwrap();
    assert_eq!(
        request
            .messages
            .iter()
            .filter(|message| message.content.contains(&rendered_contract))
            .count(),
        1
    );
    assert!(request.messages.iter().any(|message| {
        message.content.contains("generatedFileRoles")
            && message.content.contains("localization.eng")
    }));
    assert_eq!(
        request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<String>()
            .matches(fixture.request.definition.definition_hash.as_str())
            .count(),
        1
    );
    assert_eq!(
        request
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<String>()
            .matches("CUSTOM-CANARY")
            .count(),
        1
    );
    assert!(!has_staging(&fixture.project.join("artifacts")));
    assert!(
        !fixture
            .project
            .join(".ats/transactions")
            .join(run.id().as_str())
            .exists()
    );
}

#[tokio::test]
async fn compile_artifact_and_cancellation_failures_restore_project_files() {
    let service = SingleGenerateService::built_in().unwrap();
    for failure in ["compile", "artifact", "cancel"] {
        let fixture = Fixture::new();
        fs::create_dir(fixture.project.join("Generated")).unwrap();
        fs::write(
            fixture.project.join("Generated/fixture_relic.cs"),
            "public class Existing {}",
        )
        .unwrap();
        let source = if failure == "compile" {
            "public class Broken {"
        } else {
            "public class Replacement {}"
        };
        let model = FixtureModel {
            source,
            snapshots: Mutex::new(Vec::new()),
        };
        let writer = FileProjectWriter;
        let real_validator = RegisteredValidationRunner;
        let real_artifacts = FileArtifactStore::new(fixture.project.clone());
        let mut run = fixture.run();
        let cancellation = CancellationToken::new();

        let result = match failure {
            "compile" => {
                service
                    .execute(
                        SingleGenerateDependencies {
                            model: &model,
                            resources: &fixture.resources,
                            writer: &writer,
                            validator: &real_validator,
                            artifacts: &real_artifacts,
                        },
                        &mut run,
                        fixture.request.clone(),
                        fixture.context(),
                        &cancellation,
                    )
                    .await
            }
            "artifact" => {
                service
                    .execute(
                        SingleGenerateDependencies {
                            model: &model,
                            resources: &fixture.resources,
                            writer: &writer,
                            validator: &real_validator,
                            artifacts: &FailingArtifactPublisher,
                        },
                        &mut run,
                        fixture.request.clone(),
                        fixture.context(),
                        &cancellation,
                    )
                    .await
            }
            _ => {
                service
                    .execute(
                        SingleGenerateDependencies {
                            model: &model,
                            resources: &fixture.resources,
                            writer: &writer,
                            validator: &CancellingValidator,
                            artifacts: &real_artifacts,
                        },
                        &mut run,
                        fixture.request.clone(),
                        fixture.context(),
                        &cancellation,
                    )
                    .await
            }
        };
        assert!(result.is_err(), "{failure} unexpectedly succeeded");
        assert_eq!(run.status(), RunStatus::Running);
        fixture.assert_clean_rollback(&run);
    }
}

fn truth(pack: &ats_game_context::LoadedGamePack) -> VerifiedTruthSnapshot {
    let source_bytes = b"fixture-source";
    let evidence = [
        ("CustomRelicModel", "public abstract class CustomRelicModel"),
        (
            "CustomContentDictionary.AddModel",
            "public static void AddModel(Type modelType)",
        ),
        ("RelicModel", "public abstract class RelicModel"),
    ]
    .into_iter()
    .map(|(symbol, excerpt)| TruthEvidenceRecord {
        source_id: "fixture-source".into(),
        symbol: symbol.into(),
        purpose: "Prove the fixture relic contract".into(),
        bounded_excerpt: excerpt.into(),
        relative_path: "sources/fixture.cs".into(),
    })
    .collect::<Vec<_>>();
    let index_bytes = serde_json::to_vec(&evidence).unwrap();
    let manifest = TruthSnapshotManifest::new(
        pack,
        vec![TruthSnapshotSource {
            id: "fixture-source".into(),
            kind: "source".into(),
            version: Some("1".into()),
            relative_path: "sources/fixture.cs".into(),
            sha256: sha256(source_bytes),
            byte_length: source_bytes.len() as u64,
        }],
        vec![TruthSnapshotIndex {
            id: "fixture-index".into(),
            provider: PrimitiveId::parse("truth.fixture-index").unwrap(),
            relative_path: "indexes/fixture.json".into(),
            sha256: sha256(&index_bytes),
            record_count: u32::try_from(evidence.len()).unwrap(),
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

fn has_staging(root: &std::path::Path) -> bool {
    if !root.exists() {
        return false;
    }
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
