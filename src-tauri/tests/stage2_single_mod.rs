use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;

use async_trait::async_trait;
use ats_adapters::{
    FileArtifactStore, FileProjectWriter, FileResourceRepository, RegisteredValidationRunner,
};
use ats_features::FeatureSpec;
use ats_features::mod_generate_batch::{
    BatchDefinitionItem, BatchGenerateContext, BatchGenerateFeature, BatchGenerateRequest,
    BatchGenerateService,
};
use ats_features::mod_generate_single::{
    SingleGenerateContext, SingleGenerateDependencies, SingleGenerateFeature,
    SingleGeneratePublication, SingleGenerateRequest, SingleGenerateResult, SingleGenerateService,
};
use ats_features::mod_plan::{ModPlanFeature, ModPlanService, PlanItem};
use ats_features::resource_prepare::ResourcePrepareFeature;
use ats_game_context::{
    ContributionResolver, GamePackLoader, TruthEvidenceRecord, TruthSnapshotIndex,
    TruthSnapshotManifest, TruthSnapshotSource, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{
    ItemFieldId, ItemId, ItemTypeId, LocaleId, LocalizationFieldId, PrimitiveId, ResourceId,
    Sha256Digest,
};
use ats_runtime::{
    ArtifactManifest, ArtifactPublishRequest, ArtifactPublisher, CancellationReason,
    CancellationToken, FinishReason, ModelClient, ModelError, ModelRequestSnapshot, ModelResponse,
    ModelStream, PublishedArtifact, RunRecord, RunStatus, RunTransition, TokenUsage,
    ValidationError, ValidationReport, ValidationRequest, ValidationRunner, VersionedPayload,
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

fn item_localization(name: &str, description: &str) -> ItemLocalization {
    ItemLocalization {
        fields: BTreeMap::from([
            (LocalizationFieldId::parse("name").unwrap(), name.into()),
            (
                LocalizationFieldId::parse("description").unwrap(),
                description.into(),
            ),
        ]),
        status: LocalizationStatus::Confirmed,
        translated_from: None,
    }
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
                    "localization.eng": {},
                    "localization.zhs": {}
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

struct BatchFixtureModel {
    generated: Mutex<u32>,
}

#[async_trait]
impl ModelClient for BatchFixtureModel {
    async fn complete(
        &self,
        request: ModelRequestSnapshot,
        _: &CancellationToken,
    ) -> Result<ModelResponse, ModelError> {
        let content = if request.feature_id() == &ModPlanFeature::id() {
            let prompt = request
                .request()
                .messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<String>();
            let item_type = prompt
                .split("<requested-item-type>\n")
                .nth(1)
                .and_then(|value| value.split("\n</requested-item-type>").next())
                .unwrap();
            serde_json::json!({
                "itemId": "planned_item",
                "itemType": item_type,
                "name": format!("Batch {item_type}"),
                "summary": format!("Batch fixture for {item_type}"),
                "behaviorIntent": [format!("Generate the {item_type} fixture")],
                "implementationConstraints": [],
                "evidenceRequirements": ["Use verified Pack evidence"],
                "acceptanceCriteria": ["Compile and publish the item"]
            })
            .to_string()
        } else {
            let mut generated = self.generated.lock().unwrap();
            *generated += 1;
            serde_json::json!({
                "files": {
                    "source": format!("public class BatchGenerated{} {{}}", *generated),
                    "localization.eng": {},
                    "localization.zhs": {}
                },
                "acceptanceNotes": ["batch fixture assembled"]
            })
            .to_string()
        };
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
            br#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup><ItemGroup><Compile Remove="artifacts/**" /><Compile Remove=".ats/**" /></ItemGroup></Project>"#,
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
                item_localization(name, "A compile-test fixture relic."),
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

    fn card() -> Self {
        let mut fixture = Self::new();
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture_card").unwrap(),
            ItemTypeId::parse("card").unwrap(),
        );
        for (field, value) in [
            ("pool", ItemFieldValue::Choice("ironclad".into())),
            ("card_type", ItemFieldValue::Choice("attack".into())),
            ("rarity", ItemFieldValue::Choice("common".into())),
            ("target", ItemFieldValue::Choice("any_enemy".into())),
            ("base_cost", ItemFieldValue::Integer(1)),
        ] {
            definition
                .canonical_fields
                .insert(ItemFieldId::parse(field).unwrap(), value);
        }
        definition.behavior_intent = vec!["Deal testable damage and upgrade once".into()];
        for (locale, name) in [("eng", "Fixture Card"), ("zhs", "Fixture Card ZHS")] {
            definition.localizations.insert(
                LocaleId::parse(locale).unwrap(),
                item_localization(name, "A compile-test fixture card."),
            );
        }
        for (role, width, height) in [("card.portrait", 250, 190), ("card.big", 1000, 760)] {
            let candidate = fixture
                .resources
                .ingest_bytes(ResourceBytesIngestRequest {
                    logical_role: role.into(),
                    origin: ResourceOrigin::UserUpload,
                    file_name: format!("{}.png", role.replace('.', "-")),
                    media: PreparedResourceMedia {
                        media_type: "image/png".into(),
                        width,
                        height,
                        has_alpha: false,
                        bytes: format!("fixture-{role}").into_bytes(),
                    },
                    provenance: ResourceVersionProvenance::Original,
                })
                .unwrap();
            let asset = fixture
                .resources
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
        fixture.request = SingleGenerateRequest {
            artifact_id: "fixture-card".into(),
            mod_id: "FixtureMod".into(),
            plan: PlanItem {
                item_id: "fixture_card".into(),
                item_type: "card".into(),
                name: "Fixture Card".into(),
                summary: "A compile-test fixture card".into(),
                behavior_intent: vec!["Deal testable damage and upgrade once".into()],
                implementation_constraints: vec![],
                evidence_requirements: vec!["Verified Card model and enum declarations".into()],
                required_resource_roles: vec!["card.portrait".into(), "card.big".into()],
                acceptance_criteria: vec!["The generated project compiles".into()],
            },
            definition: StoredItemDefinition {
                definition_hash: definition.definition_hash().unwrap(),
                definition,
            },
        };
        fixture
    }

    fn potion() -> Self {
        let mut fixture = Self::new();
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture_potion").unwrap(),
            ItemTypeId::parse("potion").unwrap(),
        );
        for (field, value) in [
            ("rarity", ItemFieldValue::Choice("common".into())),
            ("usage", ItemFieldValue::Choice("combat_only".into())),
            ("target", ItemFieldValue::Choice("any_enemy".into())),
        ] {
            definition
                .canonical_fields
                .insert(ItemFieldId::parse(field).unwrap(), value);
        }
        definition.behavior_intent = vec!["Apply a testable temporary effect".into()];
        for (locale, name) in [("eng", "Fixture Potion"), ("zhs", "Fixture Potion ZHS")] {
            definition.localizations.insert(
                LocaleId::parse(locale).unwrap(),
                item_localization(name, "A compile-test fixture potion."),
            );
        }
        let candidate = fixture
            .resources
            .ingest_bytes(ResourceBytesIngestRequest {
                logical_role: "potion.icon".into(),
                origin: ResourceOrigin::UserUpload,
                file_name: "potion-icon.png".into(),
                media: PreparedResourceMedia {
                    media_type: "image/png".into(),
                    width: 128,
                    height: 128,
                    has_alpha: true,
                    bytes: b"fixture-potion.icon".to_vec(),
                },
                provenance: ResourceVersionProvenance::Original,
            })
            .unwrap();
        let asset = fixture
            .resources
            .select(candidate.resource_id(), &candidate.versions()[0].id)
            .unwrap();
        definition.resource_bindings.insert(
            ResourceId::parse("potion.icon").unwrap(),
            ItemResourceBinding {
                resource_id: asset.resource_id().clone(),
                selected_version: asset.selected_version().unwrap().clone(),
            },
        );
        fixture.request = SingleGenerateRequest {
            artifact_id: "fixture-potion".into(),
            mod_id: "FixtureMod".into(),
            plan: PlanItem {
                item_id: "fixture_potion".into(),
                item_type: "potion".into(),
                name: "Fixture Potion".into(),
                summary: "A compile-test fixture potion".into(),
                behavior_intent: vec!["Apply a testable temporary effect".into()],
                implementation_constraints: vec![],
                evidence_requirements: vec!["Verified Potion model and enum declarations".into()],
                required_resource_roles: vec!["potion.icon".into()],
                acceptance_criteria: vec!["The generated project compiles".into()],
            },
            definition: StoredItemDefinition {
                definition_hash: definition.definition_hash().unwrap(),
                definition,
            },
        };
        fixture
    }

    fn power() -> Self {
        let mut fixture = Self::new();
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture_power").unwrap(),
            ItemTypeId::parse("power").unwrap(),
        );
        for (field, value) in [
            ("power_type", ItemFieldValue::Choice("buff".into())),
            ("stack_type", ItemFieldValue::Choice("counter".into())),
            ("instance_type", ItemFieldValue::Choice("none".into())),
            ("allow_negative", ItemFieldValue::Boolean(false)),
        ] {
            definition
                .canonical_fields
                .insert(ItemFieldId::parse(field).unwrap(), value);
        }
        definition.behavior_intent = vec!["Grant a testable turn-scoped effect".into()];
        for (locale, name) in [("eng", "Fixture Power"), ("zhs", "Fixture Power ZHS")] {
            definition.localizations.insert(
                LocaleId::parse(locale).unwrap(),
                item_localization(name, "A compile-test fixture power."),
            );
        }
        for (role, width, height) in [("power.icon", 48, 48), ("power.big", 192, 192)] {
            let candidate = fixture
                .resources
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
            let asset = fixture
                .resources
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
        fixture.request = SingleGenerateRequest {
            artifact_id: "fixture-power".into(),
            mod_id: "FixtureMod".into(),
            plan: PlanItem {
                item_id: "fixture_power".into(),
                item_type: "power".into(),
                name: "Fixture Power".into(),
                summary: "A compile-test fixture power".into(),
                behavior_intent: vec!["Grant a testable turn-scoped effect".into()],
                implementation_constraints: vec![],
                evidence_requirements: vec![
                    "Verified Power lifecycle and stack declarations".into(),
                ],
                required_resource_roles: vec!["power.icon".into(), "power.big".into()],
                acceptance_criteria: vec!["The generated project compiles".into()],
            },
            definition: StoredItemDefinition {
                definition_hash: definition.definition_hash().unwrap(),
                definition,
            },
        };
        fixture
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
        self.context_with_truth(&self.truth)
    }

    fn context_with_truth<'a>(
        &'a self,
        truth: &'a VerifiedTruthSnapshot,
    ) -> SingleGenerateContext<'a> {
        SingleGenerateContext {
            pack: &self.pack,
            contributions: &self.generate_contributions,
            resource_contributions: &self.resource_contributions,
            truth,
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
    let (manifest_ref, manifest_sha256) = published_artifact(&execution.result);
    let manifest = fixture.project.join(manifest_ref);
    let bytes = fs::read(&manifest).unwrap();
    assert_eq!(&sha256(&bytes), manifest_sha256);
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
    assert_eq!(files_contract["properties"]["source"]["type"], "string");
    assert_eq!(
        files_contract["properties"]["localization.eng"]["type"],
        "object"
    );
    assert_eq!(
        files_contract["properties"]["localization.eng"]["additionalProperties"]["type"],
        "string"
    );
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
async fn card_pack_truth_resources_prompt_and_artifact_form_one_vertical_contract() {
    let fixture = Fixture::card();
    let service = SingleGenerateService::built_in().unwrap();
    let writer = FileProjectWriter;
    let validator = RegisteredValidationRunner;
    let artifacts = FileArtifactStore::new(fixture.project.clone());

    for missing in ["truth", "resource"] {
        let model = FixtureModel {
            source: "public class MustNotRun {}",
            snapshots: Mutex::new(Vec::new()),
        };
        let mut request = fixture.request.clone();
        let missing_truth = truth_without(&fixture.pack, "TargetType");
        if missing == "resource" {
            request
                .definition
                .definition
                .resource_bindings
                .remove(&ResourceId::parse("card.big").unwrap());
            request.definition.definition_hash =
                request.definition.definition.definition_hash().unwrap();
        }
        let payload =
            VersionedPayload::from_typed(SingleGenerateFeature::request_schema(), &request)
                .unwrap();
        let mut run = RunRecord::new(SingleGenerateFeature::id(), payload);
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        let context = if missing == "truth" {
            fixture.context_with_truth(&missing_truth)
        } else {
            fixture.context()
        };
        assert!(
            service
                .execute(
                    SingleGenerateDependencies {
                        model: &model,
                        resources: &fixture.resources,
                        writer: &writer,
                        validator: &validator,
                        artifacts: &artifacts,
                    },
                    &mut run,
                    request,
                    context,
                    &CancellationToken::new(),
                )
                .await
                .is_err()
        );
        assert!(model.snapshots.lock().unwrap().is_empty());
    }

    let model = FixtureModel {
        source: "public class FixtureCard {}",
        snapshots: Mutex::new(Vec::new()),
    };
    let mut run = fixture.run();
    let execution = service
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
    let (manifest_ref, manifest_sha256) = published_artifact(&execution.result);
    let manifest_path = fixture.project.join(manifest_ref);
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    assert_eq!(&sha256(&manifest_bytes), manifest_sha256);
    let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest.files.len(), 5);
    assert_eq!(
        manifest.feature_extension.payload()["definitionHash"],
        fixture.request.definition.definition_hash.as_str()
    );
    assert!(manifest.provenance.iter().any(|entry| {
        entry.schema().id.as_str() == "artifact.item-definition-provenance"
            && entry.payload()["definitionHash"]
                == fixture.request.definition.definition_hash.as_str()
    }));
    for file in &manifest.files {
        let bytes = fs::read(
            manifest_path
                .parent()
                .unwrap()
                .join(&file.snapshot_relative_path),
        )
        .unwrap();
        assert_eq!(u64::try_from(bytes.len()).unwrap(), file.byte_length);
        assert_eq!(sha256(&bytes), file.sha256);
    }
    let published = manifest
        .files
        .iter()
        .filter_map(|file| file.published_relative_path.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(published.contains("Generated/fixture_card.cs"));
    assert!(published.contains("FixtureMod/localization/eng/cards.json"));
    assert!(published.contains("FixtureMod/localization/zhs/cards.json"));
    assert!(published.contains("FixtureMod/images/card_portraits/fixture_card.png"));
    assert!(published.contains("FixtureMod/images/card_portraits/big/fixture_card.png"));

    let snapshots = model.snapshots.lock().unwrap();
    let prompt = snapshots[0]
        .request()
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<String>();
    assert!(prompt.contains("map pool, card_type, rarity, target, and base_cost exactly"));
    assert!(!prompt.contains("Generate one STS2 relic implementation"));
    assert!(prompt.contains(fixture.request.definition.definition_hash.as_str()));
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
async fn potion_pack_truth_resources_prompt_and_artifact_form_one_vertical_contract() {
    let fixture = Fixture::potion();
    let service = SingleGenerateService::built_in().unwrap();
    let writer = FileProjectWriter;
    let validator = RegisteredValidationRunner;
    let artifacts = FileArtifactStore::new(fixture.project.clone());

    assert_eq!(
        fixture.request.definition.definition.canonical_fields.len(),
        3
    );
    assert!(["eng", "zhs"].into_iter().all(|locale| {
        fixture.request.definition.definition.localizations[&LocaleId::parse(locale).unwrap()]
            .status
            == LocalizationStatus::Confirmed
    }));

    for missing in ["truth", "resource"] {
        let model = FixtureModel {
            source: "public class MustNotRun {}",
            snapshots: Mutex::new(Vec::new()),
        };
        let mut request = fixture.request.clone();
        let missing_truth = truth_without(&fixture.pack, "PotionUsage");
        if missing == "resource" {
            request
                .definition
                .definition
                .resource_bindings
                .remove(&ResourceId::parse("potion.icon").unwrap());
            request.definition.definition_hash =
                request.definition.definition.definition_hash().unwrap();
        }
        let payload =
            VersionedPayload::from_typed(SingleGenerateFeature::request_schema(), &request)
                .unwrap();
        let mut run = RunRecord::new(SingleGenerateFeature::id(), payload);
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        let context = if missing == "truth" {
            fixture.context_with_truth(&missing_truth)
        } else {
            fixture.context()
        };
        assert!(
            service
                .execute(
                    SingleGenerateDependencies {
                        model: &model,
                        resources: &fixture.resources,
                        writer: &writer,
                        validator: &validator,
                        artifacts: &artifacts,
                    },
                    &mut run,
                    request,
                    context,
                    &CancellationToken::new(),
                )
                .await
                .is_err()
        );
        assert!(model.snapshots.lock().unwrap().is_empty());
    }

    let model = FixtureModel {
        source: "public class FixturePotion {}",
        snapshots: Mutex::new(Vec::new()),
    };
    let mut run = fixture.run();
    let execution = service
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
    let (manifest_ref, manifest_sha256) = published_artifact(&execution.result);
    let manifest_path = fixture.project.join(manifest_ref);
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    assert_eq!(&sha256(&manifest_bytes), manifest_sha256);
    let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest.files.len(), 4);
    assert_eq!(
        manifest.feature_extension.payload()["definitionHash"],
        fixture.request.definition.definition_hash.as_str()
    );
    assert!(manifest.provenance.iter().any(|entry| {
        entry.schema().id.as_str() == "artifact.item-definition-provenance"
            && entry.payload()["definitionHash"]
                == fixture.request.definition.definition_hash.as_str()
    }));
    for file in &manifest.files {
        let bytes = fs::read(
            manifest_path
                .parent()
                .unwrap()
                .join(&file.snapshot_relative_path),
        )
        .unwrap();
        assert_eq!(u64::try_from(bytes.len()).unwrap(), file.byte_length);
        assert_eq!(sha256(&bytes), file.sha256);
    }
    let published = manifest
        .files
        .iter()
        .filter_map(|file| file.published_relative_path.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(published.contains("Generated/fixture_potion.cs"));
    assert!(published.contains("FixtureMod/localization/eng/potions.json"));
    assert!(published.contains("FixtureMod/localization/zhs/potions.json"));
    assert!(published.contains("FixtureMod/images/potions/fixture_potion.png"));

    let snapshots = model.snapshots.lock().unwrap();
    let prompt = snapshots[0]
        .request()
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<String>();
    assert!(prompt.contains("map rarity, usage, and target exactly"));
    assert!(!prompt.contains("Generate one STS2 card implementation"));
    assert!(!prompt.contains("Generate one STS2 relic implementation"));
    assert!(prompt.contains(fixture.request.definition.definition_hash.as_str()));
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
async fn power_pack_truth_resources_prompt_and_artifact_form_one_vertical_contract() {
    let fixture = Fixture::power();
    let service = SingleGenerateService::built_in().unwrap();
    let writer = FileProjectWriter;
    let validator = RegisteredValidationRunner;
    let artifacts = FileArtifactStore::new(fixture.project.clone());

    assert_eq!(
        fixture.request.definition.definition.canonical_fields.len(),
        4
    );
    assert!(["eng", "zhs"].into_iter().all(|locale| {
        fixture.request.definition.definition.localizations[&LocaleId::parse(locale).unwrap()]
            .status
            == LocalizationStatus::Confirmed
    }));

    for missing in ["truth", "resource"] {
        let model = FixtureModel {
            source: "public class MustNotRun {}",
            snapshots: Mutex::new(Vec::new()),
        };
        let mut request = fixture.request.clone();
        let missing_truth = truth_without(&fixture.pack, "PowerStackType");
        if missing == "resource" {
            request
                .definition
                .definition
                .resource_bindings
                .remove(&ResourceId::parse("power.big").unwrap());
            request.definition.definition_hash =
                request.definition.definition.definition_hash().unwrap();
        }
        let payload =
            VersionedPayload::from_typed(SingleGenerateFeature::request_schema(), &request)
                .unwrap();
        let mut run = RunRecord::new(SingleGenerateFeature::id(), payload);
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        let context = if missing == "truth" {
            fixture.context_with_truth(&missing_truth)
        } else {
            fixture.context()
        };
        assert!(
            service
                .execute(
                    SingleGenerateDependencies {
                        model: &model,
                        resources: &fixture.resources,
                        writer: &writer,
                        validator: &validator,
                        artifacts: &artifacts,
                    },
                    &mut run,
                    request,
                    context,
                    &CancellationToken::new(),
                )
                .await
                .is_err()
        );
        assert!(model.snapshots.lock().unwrap().is_empty());
    }

    let model = FixtureModel {
        source: "public class FixturePower {}",
        snapshots: Mutex::new(Vec::new()),
    };
    let mut run = fixture.run();
    let execution = service
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
    let (manifest_ref, manifest_sha256) = published_artifact(&execution.result);
    let manifest_path = fixture.project.join(manifest_ref);
    let manifest_bytes = fs::read(&manifest_path).unwrap();
    assert_eq!(&sha256(&manifest_bytes), manifest_sha256);
    let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest.files.len(), 5);
    assert_eq!(
        manifest.feature_extension.payload()["definitionHash"],
        fixture.request.definition.definition_hash.as_str()
    );
    assert!(manifest.provenance.iter().any(|entry| {
        entry.schema().id.as_str() == "artifact.item-definition-provenance"
            && entry.payload()["definitionHash"]
                == fixture.request.definition.definition_hash.as_str()
    }));
    for file in &manifest.files {
        let bytes = fs::read(
            manifest_path
                .parent()
                .unwrap()
                .join(&file.snapshot_relative_path),
        )
        .unwrap();
        assert_eq!(u64::try_from(bytes.len()).unwrap(), file.byte_length);
        assert_eq!(sha256(&bytes), file.sha256);
    }
    let published = manifest
        .files
        .iter()
        .filter_map(|file| file.published_relative_path.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(published.contains("Generated/fixture_power.cs"));
    assert!(published.contains("FixtureMod/localization/eng/powers.json"));
    assert!(published.contains("FixtureMod/localization/zhs/powers.json"));
    assert!(published.contains("FixtureMod/images/powers/fixture_power.png"));
    assert!(published.contains("FixtureMod/images/powers/big/fixture_power.png"));

    let snapshots = model.snapshots.lock().unwrap();
    let prompt = snapshots[0]
        .request()
        .messages
        .iter()
        .map(|message| message.content.as_str())
        .collect::<String>();
    assert!(
        prompt.contains("map power_type, stack_type, instance_type, and allow_negative exactly")
    );
    assert!(!prompt.contains("Generate one STS2 card implementation"));
    assert!(!prompt.contains("Generate one STS2 relic implementation"));
    assert!(!prompt.contains("Generate one STS2 potion implementation"));
    assert!(prompt.contains(fixture.request.definition.definition_hash.as_str()));
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
async fn batch_generates_all_four_definition_bound_sts2_types_with_real_compile() {
    let fixture = Fixture::new();
    let mut items = vec![BatchDefinitionItem {
        artifact_id: fixture.request.artifact_id.clone(),
        definition: fixture.request.definition.clone(),
    }];
    for source in [Fixture::card(), Fixture::potion(), Fixture::power()] {
        items.push(copy_batch_item(&source, &fixture));
    }
    let plan_contributions = ContributionResolver::new([])
        .resolve(
            &fixture.pack,
            &ModPlanFeature::id(),
            &[ModPlanFeature::contribution_requirement()],
        )
        .unwrap();
    let batch_contributions = ContributionResolver::new([])
        .resolve(
            &fixture.pack,
            &BatchGenerateFeature::id(),
            &[BatchGenerateFeature::contribution_requirement()],
        )
        .unwrap();
    let request = BatchGenerateRequest {
        mod_id: "FixtureMod".into(),
        items,
        fail_fast: false,
    };
    let payload =
        VersionedPayload::from_typed(BatchGenerateFeature::request_schema(), &request).unwrap();
    let mut run = RunRecord::new(BatchGenerateFeature::id(), payload);
    run.apply_transition(RunTransition::Start, Utc::now())
        .unwrap();
    let model = BatchFixtureModel {
        generated: Mutex::new(0),
    };
    let plan = ModPlanService::built_in().unwrap();
    let single = SingleGenerateService::built_in().unwrap();
    let batch = BatchGenerateService::new(&plan, &single);
    let writer = FileProjectWriter;
    let validator = RegisteredValidationRunner;
    let artifacts = FileArtifactStore::new(fixture.project.clone());
    let execution = batch
        .execute(
            SingleGenerateDependencies {
                model: &model,
                resources: &fixture.resources,
                writer: &writer,
                validator: &validator,
                artifacts: &artifacts,
            },
            &mut run,
            request,
            BatchGenerateContext {
                pack: &fixture.pack,
                batch_contributions: &batch_contributions,
                plan_contributions: &plan_contributions,
                single_contributions: &fixture.generate_contributions,
                resource_contributions: &fixture.resource_contributions,
                truth: &fixture.truth,
                project_root: &fixture.project,
                project_context: "Four definition-bound STS2 items.",
                custom_instructions: None,
                model: None,
            },
            &CancellationToken::new(),
        )
        .await
        .unwrap();

    assert_eq!(run.status(), RunStatus::Succeeded);
    assert_eq!(execution.result.total, 4);
    assert_eq!(
        execution.result.succeeded, 4,
        "{:#?}",
        execution.result.items
    );
    assert_eq!(execution.result.failed, 0);
    assert_eq!(execution.child_runs.len(), 8);
    assert!(
        execution
            .child_runs
            .iter()
            .all(|child| child.status() == RunStatus::Succeeded)
    );
    for item in execution.result.items {
        let generated = item.result.unwrap();
        let (manifest_ref, manifest_sha256) = published_artifact(&generated);
        let manifest_bytes = fs::read(fixture.project.join(manifest_ref)).unwrap();
        assert_eq!(&sha256(&manifest_bytes), manifest_sha256);
        let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes).unwrap();
        assert_eq!(
            manifest.feature_extension.payload()["definitionHash"],
            item.definition_hash.as_str(),
        );
    }
    assert!(!has_staging(&fixture.project.join("artifacts")));
    assert!(
        !fixture
            .project
            .join(".ats/transactions")
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_some())
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

fn copy_batch_item(source: &Fixture, target: &Fixture) -> BatchDefinitionItem {
    let mut stored = source.request.definition.clone();
    for binding in stored.definition.resource_bindings.values_mut() {
        let asset = source.resources.load(&binding.resource_id).unwrap();
        let selected = asset.selected().unwrap().clone();
        let bytes = source
            .resources
            .read_selected_bytes(&binding.resource_id, &binding.selected_version)
            .unwrap();
        let candidate = target
            .resources
            .ingest_bytes(ResourceBytesIngestRequest {
                logical_role: asset.logical_role().into(),
                origin: ResourceOrigin::UserUpload,
                file_name: format!("{}.png", asset.logical_role().replace('.', "-")),
                media: PreparedResourceMedia {
                    media_type: selected.blob.media_type,
                    width: selected.blob.width,
                    height: selected.blob.height,
                    has_alpha: selected.blob.has_alpha,
                    bytes,
                },
                provenance: ResourceVersionProvenance::Original,
            })
            .unwrap();
        let selected = target
            .resources
            .select(candidate.resource_id(), &candidate.versions()[0].id)
            .unwrap();
        binding.resource_id = selected.resource_id().clone();
        binding.selected_version = selected.selected_version().unwrap().clone();
    }
    stored.definition_hash = stored.definition.definition_hash().unwrap();
    BatchDefinitionItem {
        artifact_id: source.request.artifact_id.clone(),
        definition: stored,
    }
}

fn truth(pack: &ats_game_context::LoadedGamePack) -> VerifiedTruthSnapshot {
    truth_without(pack, "")
}

fn truth_without(
    pack: &ats_game_context::LoadedGamePack,
    excluded_symbol: &str,
) -> VerifiedTruthSnapshot {
    let source_bytes = b"fixture-source";
    let evidence = [
        ("CustomRelicModel", "public abstract class CustomRelicModel"),
        (
            "CustomContentDictionary.AddModel",
            "public static void AddModel(Type modelType)",
        ),
        (
            "CustomCardPoolModel",
            "public abstract class CustomCardPoolModel",
        ),
        (
            "CustomRelicPoolModel",
            "public abstract class CustomRelicPoolModel",
        ),
        (
            "CustomPotionPoolModel",
            "public abstract class CustomPotionPoolModel",
        ),
        ("RelicModel", "public abstract class RelicModel"),
        ("CustomCardModel", "public abstract class CustomCardModel"),
        ("PoolAttribute", "public sealed class PoolAttribute"),
        ("CardType", "public enum CardType"),
        ("CardRarity", "public enum CardRarity"),
        ("TargetType", "public enum TargetType"),
        (
            "CustomPotionModel",
            "public abstract class CustomPotionModel",
        ),
        ("SharedPotionPool", "public static class SharedPotionPool"),
        ("PotionRarity", "public enum PotionRarity"),
        ("PotionUsage", "public enum PotionUsage"),
        ("CustomPowerModel", "public abstract class CustomPowerModel"),
        ("PowerModel", "public abstract class PowerModel"),
        ("PowerType", "public enum PowerType"),
        ("PowerStackType", "public enum PowerStackType"),
        ("PowerInstanceType", "public enum PowerInstanceType"),
        ("PowerCmd", "public static class PowerCmd"),
    ]
    .into_iter()
    .filter(|(symbol, _)| *symbol != excluded_symbol)
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

fn published_artifact(result: &SingleGenerateResult) -> (&str, &Sha256Digest) {
    assert_eq!(result.publication, SingleGeneratePublication::Published);
    (
        result
            .artifact_manifest_ref
            .as_deref()
            .expect("published Single result has a manifest ref"),
        result
            .manifest_sha256
            .as_ref()
            .expect("published Single result has a manifest hash"),
    )
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
