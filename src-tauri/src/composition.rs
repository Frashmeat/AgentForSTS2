use std::path::{Path, PathBuf};

use ats_adapters::{
    FileArtifactStore, FileProjectWriter, FileResourceRepository, FileTruthSnapshotRepository,
    HttpMediaClient, HttpModelClient, RegisteredBuildRunner, RegisteredValidationRunner,
    ZipPackageWriter,
};
use ats_features::FeatureSpec;
use ats_features::log_analyze::{LogAnalyzeContext, LogAnalyzeFeature, LogAnalyzeService};
use ats_features::mod_generate_batch::{
    BatchGenerateContext, BatchGenerateFeature, BatchGenerateService,
};
use ats_features::mod_generate_complex::{
    ComplexGenerateContext, ComplexGenerateDependencies, ComplexGenerateFeature,
    ComplexGenerateService,
};
use ats_features::mod_generate_single::{
    SingleGenerateContext, SingleGenerateDependencies, SingleGenerateFeature, SingleGenerateService,
};
use ats_features::mod_plan::{ModPlanContext, ModPlanFeature, ModPlanService};
use ats_features::project_build::{ProjectBuildContext, ProjectBuildFeature, ProjectBuildService};
use ats_features::project_package::{
    ProjectPackageContext, ProjectPackageFeature, ProjectPackageService,
};
use ats_features::resource_prepare::{
    ResourcePrepareContext, ResourcePrepareError, ResourcePrepareFeature, ResourcePrepareService,
    ResourcePrepareSource,
};
use ats_features::{FeatureRegistry, built_in_feature_registry};
use ats_game_context::{
    ContributionRequirement, ContributionResolver, EvidenceQuery, GamePackLoader, LoadedGamePack,
    TruthSnapshotRepository, VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{FailureCode, FeatureId, PrimitiveId};
use ats_runtime::{
    CancellationToken, MediaError, ModelClient, RunFailure, RunRecord, RunRepository, RunStatus,
    RunTransition, VersionedPayload,
};
use ats_workspace::ProjectMeta;
use chrono::Utc;

use crate::AppConfig;

pub struct Stage2Composition {
    pack: LoadedGamePack,
    contributions: ContributionResolver,
    registry: FeatureRegistry,
    runtime_root: PathBuf,
}

impl Stage2Composition {
    pub fn built_in(runtime_root: PathBuf) -> Result<Self, ()> {
        let pack = GamePackLoader::load_built_in_sts2().map_err(|_| ())?;
        let contributions = ContributionResolver::new(
            [
                "code.dotnet-validate",
                "image.role-transform",
                "log.dotnet-parser",
                "process.dotnet-publish",
                "storage.project-template",
            ]
            .into_iter()
            .map(|id| PrimitiveId::parse(id).expect("built-in Primitive ID is valid")),
        );
        let registry = built_in_feature_registry().map_err(|_| ())?;
        Ok(Self {
            pack,
            contributions,
            registry,
            runtime_root,
        })
    }

    #[must_use]
    pub fn pack(&self) -> &LoadedGamePack {
        &self.pack
    }

    #[must_use]
    pub fn runtime_root(&self) -> &Path {
        &self.runtime_root
    }

    pub fn resolve(
        &self,
        feature_id: &FeatureId,
        requirements: &[ContributionRequirement],
    ) -> Result<VerifiedContributionSet, RunFailure> {
        self.contributions
            .resolve(&self.pack, feature_id, requirements)
            .map_err(|_| failure("pack.contribution_invalid", "feature.contribution"))
    }

    pub fn current_truth(&self) -> Result<VerifiedTruthSnapshot, RunFailure> {
        FileTruthSnapshotRepository::new(self.runtime_root.clone())
            .open_current(&self.pack)
            .map_err(|_| failure("truth.invalid", "feature.truth"))?
            .ok_or_else(|| failure("truth.missing", "feature.truth"))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        config: &AppConfig,
        project_root: &Path,
        project: &ProjectMeta,
        run: RunRecord,
        repository: &dyn RunRepository,
        source_path: Option<PathBuf>,
        cancellation: &CancellationToken,
    ) -> Result<RunRecord, RunFailure> {
        self.execute_inner(
            config,
            project_root,
            project,
            run,
            repository,
            source_path,
            cancellation,
            None,
        )
        .await
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_with_model(
        &self,
        config: &AppConfig,
        project_root: &Path,
        project: &ProjectMeta,
        run: RunRecord,
        repository: &dyn RunRepository,
        source_path: Option<PathBuf>,
        cancellation: &CancellationToken,
        model: &dyn ModelClient,
    ) -> Result<RunRecord, RunFailure> {
        self.execute_inner(
            config,
            project_root,
            project,
            run,
            repository,
            source_path,
            cancellation,
            Some(model),
        )
        .await
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    async fn execute_inner(
        &self,
        config: &AppConfig,
        project_root: &Path,
        project: &ProjectMeta,
        mut run: RunRecord,
        repository: &dyn RunRepository,
        source_path: Option<PathBuf>,
        cancellation: &CancellationToken,
        model_override: Option<&dyn ModelClient>,
    ) -> Result<RunRecord, RunFailure> {
        self.registry
            .validate_request(run.feature_id(), run.request())
            .map_err(|_| failure("run.input_invalid", "feature.request"))?;
        let settings = config.settings_snapshot();
        let project_context = format!(
            "Project name: {}; Mod ID: {}; Game Pack: {}.",
            project.name, project.csharp_name, project.game_id
        );
        let custom_instructions = (!settings.llm.custom_prompt.trim().is_empty())
            .then_some(settings.llm.custom_prompt.as_str());
        let model_name =
            (!settings.llm.model.trim().is_empty()).then_some(settings.llm.model.clone());

        match run.feature_id().as_str() {
            "mod.plan" => {
                let request = self.decode::<ModPlanFeature>(&run)?;
                let contributions = self.resolve(
                    &ModPlanFeature::id(),
                    &[ModPlanFeature::contribution_requirement()],
                )?;
                let model = select_model(model_override, &settings.llm)?;
                let execution = ModPlanService::built_in()
                    .map_err(|_| failure("feature.recipe_invalid", "mod.plan.recipe"))?
                    .execute(
                        model.client(),
                        request,
                        ModPlanContext {
                            pack: &self.pack,
                            contributions: &contributions,
                            project_context: Some(&project_context),
                            custom_instructions,
                            model: model_name,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|_| failure("feature.execution_failed", "mod.plan.execute"))?;
                succeed::<ModPlanFeature, _>(&mut run, &execution.item)?;
            }
            "mod.generate.single" => {
                let request = self.decode::<SingleGenerateFeature>(&run)?;
                let truth = self.current_truth()?;
                let single = self.resolve(
                    &SingleGenerateFeature::id(),
                    &[SingleGenerateFeature::contribution_requirement()],
                )?;
                let resource = self.resolve(
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )?;
                let model = select_model(model_override, &settings.llm)?;
                let adapters = SingleAdapters::new(project_root);
                SingleGenerateService::built_in()
                    .map_err(|_| failure("feature.recipe_invalid", "mod.generate.single.recipe"))?
                    .execute(
                        adapters.dependencies(model.client()),
                        &mut run,
                        request,
                        SingleGenerateContext {
                            pack: &self.pack,
                            contributions: &single,
                            resource_contributions: &resource,
                            truth: &truth,
                            project_root,
                            project_context: &project_context,
                            custom_instructions,
                            model: model_name,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|_| {
                        failure("feature.execution_failed", "mod.generate.single.execute")
                    })?;
            }
            "mod.generate.batch" => {
                let request = self.decode::<BatchGenerateFeature>(&run)?;
                let truth = self.current_truth()?;
                let batch_contributions = self.resolve(
                    &BatchGenerateFeature::id(),
                    &[BatchGenerateFeature::contribution_requirement()],
                )?;
                let single_contributions = self.resolve(
                    &SingleGenerateFeature::id(),
                    &[SingleGenerateFeature::contribution_requirement()],
                )?;
                let resource_contributions = self.resolve(
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )?;
                let model = select_model(model_override, &settings.llm)?;
                let adapters = SingleAdapters::new(project_root);
                let single = SingleGenerateService::built_in()
                    .map_err(|_| failure("feature.recipe_invalid", "mod.generate.batch.recipe"))?;
                let execution = BatchGenerateService::new(&single)
                    .execute(
                        adapters.dependencies(model.client()),
                        &mut run,
                        request,
                        BatchGenerateContext {
                            pack: &self.pack,
                            batch_contributions: &batch_contributions,
                            single_contributions: &single_contributions,
                            resource_contributions: &resource_contributions,
                            truth: &truth,
                            project_root,
                            project_context: &project_context,
                            custom_instructions,
                            model: model_name,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|_| {
                        failure("feature.execution_failed", "mod.generate.batch.execute")
                    })?;
                persist_children(repository, execution.child_runs)?;
            }
            "mod.generate.complex" => {
                let request = self.decode::<ComplexGenerateFeature>(&run)?;
                let truth = self.current_truth()?;
                let complex_contributions = self.resolve(
                    &ComplexGenerateFeature::id(),
                    &[ComplexGenerateFeature::contribution_requirement()],
                )?;
                let plan_contributions = self.resolve(
                    &ModPlanFeature::id(),
                    &[ModPlanFeature::contribution_requirement()],
                )?;
                let batch_contributions = self.resolve(
                    &BatchGenerateFeature::id(),
                    &[BatchGenerateFeature::contribution_requirement()],
                )?;
                let single_contributions = self.resolve(
                    &SingleGenerateFeature::id(),
                    &[SingleGenerateFeature::contribution_requirement()],
                )?;
                let resource_contributions = self.resolve(
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )?;
                let build_contributions = self.resolve(
                    &ProjectBuildFeature::id(),
                    &[ProjectBuildFeature::contribution_requirement()],
                )?;
                let package_contributions = self.resolve(
                    &ProjectPackageFeature::id(),
                    &[ProjectPackageFeature::contribution_requirement()],
                )?;
                let model = select_model(model_override, &settings.llm)?;
                let plan = ModPlanService::built_in().map_err(|_| {
                    failure("feature.recipe_invalid", "mod.generate.complex.plan_recipe")
                })?;
                let single = SingleGenerateService::built_in().map_err(|_| {
                    failure(
                        "feature.recipe_invalid",
                        "mod.generate.complex.single_recipe",
                    )
                })?;
                let batch = BatchGenerateService::new(&single);
                let build = ProjectBuildService;
                let package = ProjectPackageService;
                let resources = FileResourceRepository::new(project_root.to_path_buf());
                let writer = FileProjectWriter;
                let validator = RegisteredValidationRunner;
                let artifacts = FileArtifactStore::new(project_root.to_path_buf());
                let build_runner = RegisteredBuildRunner;
                let package_writer = ZipPackageWriter;
                let execution = ComplexGenerateService::new(&plan, &batch, &build, &package)
                    .execute(
                        ComplexGenerateDependencies {
                            model: model.client(),
                            resources: &resources,
                            writer: &writer,
                            validator: &validator,
                            artifacts: &artifacts,
                            build_runner: &build_runner,
                            package_writer: &package_writer,
                        },
                        &mut run,
                        request,
                        ComplexGenerateContext {
                            pack: &self.pack,
                            complex_contributions: &complex_contributions,
                            plan_contributions: &plan_contributions,
                            batch_contributions: &batch_contributions,
                            single_contributions: &single_contributions,
                            resource_contributions: &resource_contributions,
                            build_contributions: &build_contributions,
                            package_contributions: &package_contributions,
                            truth: &truth,
                            project_root,
                            project_context: &project_context,
                            custom_instructions,
                            model: model_name,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|_| {
                        failure("feature.execution_failed", "mod.generate.complex.execute")
                    })?;
                persist_children(repository, execution.child_runs)?;
            }
            "log.analyze" => {
                let request = self.decode::<LogAnalyzeFeature>(&run)?;
                let truth = self.current_truth()?;
                let contributions = self.resolve(
                    &LogAnalyzeFeature::id(),
                    &[LogAnalyzeFeature::contribution_requirement()],
                )?;
                let model = select_model(model_override, &settings.llm)?;
                let execution = LogAnalyzeService::built_in()
                    .map_err(|_| failure("feature.recipe_invalid", "log.analyze.recipe"))?
                    .execute(
                        model.client(),
                        request,
                        LogAnalyzeContext {
                            pack: &self.pack,
                            contributions: &contributions,
                            truth_snapshot: &truth,
                            evidence_query: &EvidenceQuery {
                                symbols: Vec::new(),
                                terms: vec!["error".into(), "exception".into(), "warning".into()],
                                limit: 20,
                            },
                            selected_resources: &[],
                            project_context: Some(&project_context),
                            custom_instructions,
                            model: model_name,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|_| failure("feature.execution_failed", "log.analyze.execute"))?;
                succeed::<LogAnalyzeFeature, _>(&mut run, &execution.result)?;
            }
            "resource.prepare" => {
                let request = self.decode::<ResourcePrepareFeature>(&run)?;
                let contributions = self.resolve(
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )?;
                let repository = FileResourceRepository::new(project_root.to_path_buf());
                let context = ResourcePrepareContext {
                    pack: &self.pack,
                    contributions: &contributions,
                };
                let result = if matches!(&request.source, ResourcePrepareSource::AiGenerated { .. })
                {
                    let media = HttpMediaClient::new(&settings.image_gen).map_err(|_| {
                        failure("resource.media_configuration", "resource.prepare.media")
                    })?;
                    ResourcePrepareService
                        .prepare_ai(&media, &repository, request, context, cancellation)
                        .await
                        .map_err(resource_prepare_failure)?
                } else {
                    let source_path =
                        source_path.filter(|path| path.is_file()).ok_or_else(|| {
                            failure("resource.source_missing", "resource.prepare.source")
                        })?;
                    ResourcePrepareService
                        .prepare_file(&repository, request, source_path, context)
                        .map_err(resource_prepare_failure)?
                };
                succeed::<ResourcePrepareFeature, _>(&mut run, &result)?;
            }
            "project.build" => {
                let request = self.decode::<ProjectBuildFeature>(&run)?;
                let contributions = self.resolve(
                    &ProjectBuildFeature::id(),
                    &[ProjectBuildFeature::contribution_requirement()],
                )?;
                ProjectBuildService
                    .execute(
                        &RegisteredBuildRunner,
                        &mut run,
                        request,
                        ProjectBuildContext {
                            pack: &self.pack,
                            contributions: &contributions,
                            project_root,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|_| failure("feature.execution_failed", "project.build.execute"))?;
            }
            "project.package" => {
                let request = self.decode::<ProjectPackageFeature>(&run)?;
                let contributions = self.resolve(
                    &ProjectPackageFeature::id(),
                    &[ProjectPackageFeature::contribution_requirement()],
                )?;
                ProjectPackageService
                    .execute(
                        &ZipPackageWriter,
                        &FileArtifactStore::new(project_root.to_path_buf()),
                        &mut run,
                        request,
                        ProjectPackageContext {
                            pack: &self.pack,
                            contributions: &contributions,
                            project_root,
                        },
                        cancellation,
                    )
                    .map_err(|_| failure("feature.execution_failed", "project.package.execute"))?;
            }
            _ => return Err(failure("feature.unknown", "feature.dispatch")),
        }

        if run.status() != RunStatus::Succeeded {
            return Err(failure("run.incomplete", "feature.dispatch"));
        }
        Ok(run)
    }

    fn decode<F: FeatureSpec>(&self, run: &RunRecord) -> Result<F::Request, RunFailure> {
        self.registry
            .decode_request::<F>(run.request())
            .map_err(|_| failure("run.input_invalid", "feature.request"))
    }
}

enum SelectedModel<'a> {
    Borrowed(&'a dyn ModelClient),
    Owned(HttpModelClient),
}

impl SelectedModel<'_> {
    fn client(&self) -> &dyn ModelClient {
        match self {
            Self::Borrowed(client) => *client,
            Self::Owned(client) => client,
        }
    }
}

fn select_model<'a>(
    model: Option<&'a dyn ModelClient>,
    config: &ats_adapters::LlmConfig,
) -> Result<SelectedModel<'a>, RunFailure> {
    model.map_or_else(
        || {
            HttpModelClient::new(config)
                .map(SelectedModel::Owned)
                .map_err(|_| failure("llm.configuration", "feature.model"))
        },
        |client| Ok(SelectedModel::Borrowed(client)),
    )
}

struct SingleAdapters {
    resources: FileResourceRepository,
    writer: FileProjectWriter,
    validator: RegisteredValidationRunner,
    artifacts: FileArtifactStore,
}

impl SingleAdapters {
    fn new(project_root: &Path) -> Self {
        Self {
            resources: FileResourceRepository::new(project_root.to_path_buf()),
            writer: FileProjectWriter,
            validator: RegisteredValidationRunner,
            artifacts: FileArtifactStore::new(project_root.to_path_buf()),
        }
    }

    fn dependencies<'a>(
        &'a self,
        model: &'a dyn ModelClient,
    ) -> SingleGenerateDependencies<
        'a,
        dyn ModelClient + 'a,
        FileResourceRepository,
        FileProjectWriter,
        RegisteredValidationRunner,
        FileArtifactStore,
    > {
        SingleGenerateDependencies {
            model,
            resources: &self.resources,
            writer: &self.writer,
            validator: &self.validator,
            artifacts: &self.artifacts,
        }
    }
}

fn succeed<F, T>(run: &mut RunRecord, result: &T) -> Result<(), RunFailure>
where
    F: FeatureSpec,
    T: serde::Serialize,
{
    let payload = VersionedPayload::from_typed(F::result_schema(), result)
        .map_err(|_| failure("run.result_invalid", "feature.result"))?;
    run.apply_transition(RunTransition::Succeed { result: payload }, Utc::now())
        .map_err(|_| failure("run.transition_failed", "feature.result"))
}

fn persist_children(
    repository: &dyn RunRepository,
    children: Vec<RunRecord>,
) -> Result<(), RunFailure> {
    for child in children {
        repository
            .create(&child)
            .map_err(|_| failure("run.storage_failed", "feature.child_run"))?;
    }
    Ok(())
}

fn resource_prepare_failure(error: ResourcePrepareError) -> RunFailure {
    match error {
        ResourcePrepareError::Media(MediaError::Authentication) => {
            failure("resource.media_authentication", "resource.prepare.media")
        }
        ResourcePrepareError::Media(MediaError::RateLimited { .. }) => {
            failure("resource.media_rate_limited", "resource.prepare.media")
        }
        ResourcePrepareError::Media(MediaError::Configuration) => {
            failure("resource.media_configuration", "resource.prepare.media")
        }
        ResourcePrepareError::Media(MediaError::Cancelled) | ResourcePrepareError::Cancelled => {
            failure("run.cancelled", "resource.prepare.media")
        }
        ResourcePrepareError::Media(_) => {
            failure("resource.media_failed", "resource.prepare.media")
        }
        ResourcePrepareError::Repository => {
            failure("resource.storage_failed", "resource.prepare.store")
        }
        _ => failure("feature.execution_failed", "resource.prepare.execute"),
    }
}

pub fn failure(code: &str, stage: &str) -> RunFailure {
    RunFailure::new(
        FailureCode::parse(code).expect("built-in failure code is valid"),
        stage,
        None,
    )
    .expect("built-in Run failure is valid")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use ats_adapters::{ConfigStatus, Settings};
    use ats_features::mod_generate_single::{SingleGenerateRequest, SingleGenerateResult};
    use ats_features::mod_plan::{ModPlanRequest, PlanItem};
    use ats_game_context::{
        TruthEvidenceRecord, TruthSnapshotIndex, TruthSnapshotManifest, TruthSnapshotSource,
        built_in_project_template,
    };
    use ats_kernel::{PrimitiveId, Sha256Digest};
    use ats_runtime::{
        ArtifactManifest, CancellationReason, FinishReason, ModelError, ModelRequestSnapshot,
        ModelResponse, ModelStream, RunId, TokenUsage,
    };
    use ats_workspace::ProjectFolder;
    use futures_util::stream;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::project_session::ProjectSession;

    struct FixtureModel {
        snapshots: Mutex<Vec<ModelRequestSnapshot>>,
    }

    #[async_trait]
    impl ModelClient for FixtureModel {
        async fn complete(
            &self,
            request: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelResponse, ModelError> {
            let content = if request.feature_id() == &ModPlanFeature::id() {
                serde_json::json!({
                    "itemId": "fixture_item",
                    "itemType": "custom_code",
                    "name": "Fixture Item",
                    "summary": "A deterministic facade fixture",
                    "behaviorIntent": ["Expose a fixture type"],
                    "implementationConstraints": [],
                    "requiredEvidence": ["Fixture.Symbol"],
                    "requiredResourceRoles": [],
                    "acceptanceCriteria": ["The project compiles"]
                })
                .to_string()
            } else {
                serde_json::json!({
                    "files": [{
                        "role": "source",
                        "content": "public class FixtureGenerated {}"
                    }],
                    "acceptanceNotes": ["deterministic facade fixture"]
                })
                .to_string()
            };
            self.snapshots.lock().unwrap().push(request);
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

    #[tokio::test]
    async fn facade_persists_plan_generation_artifact_and_releases_project_lock() {
        let temp = tempfile::TempDir::new().unwrap();
        let runtime_root = temp.path().join("runtime");
        let composition = Arc::new(Stage2Composition::built_in(runtime_root.clone()).unwrap());
        write_truth_fixture(&runtime_root, composition.pack());

        let template = built_in_project_template(composition.pack(), "sts2.default").unwrap();
        let folder = ProjectFolder::create(
            temp.path(),
            "FacadeFixture",
            composition.pack().id(),
            &template,
        )
        .unwrap();
        let project_root = folder.path().to_path_buf();
        fs::write(
            project_root.join("FacadeFixture.csproj"),
            br#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework><EnableDefaultCompileItems>false</EnableDefaultCompileItems></PropertyGroup><ItemGroup><Compile Include="Generated/**/*.cs" /></ItemGroup></Project>"#,
        )
        .unwrap();
        let session = ProjectSession::open(folder).unwrap();
        let mut settings = Settings::default();
        settings.llm.custom_prompt = "FACADE-CUSTOM-CANARY".into();
        let config = Arc::new(AppConfig::new(
            settings,
            ConfigStatus {
                path: None,
                file_present: false,
                loaded: false,
                errors: Vec::new(),
            },
        ));
        let model = Arc::new(FixtureModel {
            snapshots: Mutex::new(Vec::new()),
        });

        let plan_run = RunRecord::new(
            ModPlanFeature::id(),
            VersionedPayload::from_typed(
                ModPlanFeature::request_schema(),
                &ModPlanRequest {
                    requirements: "Create one compileable fixture type".into(),
                    item_type: Some("custom_code".into()),
                },
            )
            .unwrap(),
        );
        let plan_id = submit_fixture_run(
            &session,
            Arc::clone(&composition),
            Arc::clone(&config),
            Arc::clone(&model),
            plan_run,
        )
        .await;
        let plan_record = wait_for_terminal(&session, &plan_id).await;
        assert_eq!(plan_record.status(), RunStatus::Succeeded);
        let plan: PlanItem = plan_record
            .result()
            .unwrap()
            .decode(&ModPlanFeature::result_schema())
            .unwrap();

        let generate_run = RunRecord::new(
            SingleGenerateFeature::id(),
            VersionedPayload::from_typed(
                SingleGenerateFeature::request_schema(),
                &SingleGenerateRequest {
                    artifact_id: "fixture-artifact".into(),
                    mod_id: "FacadeFixture".into(),
                    plan,
                    selected_resources: Vec::new(),
                },
            )
            .unwrap(),
        );
        let generate_id = submit_fixture_run(
            &session,
            Arc::clone(&composition),
            Arc::clone(&config),
            Arc::clone(&model),
            generate_run,
        )
        .await;
        let generate_record = wait_for_terminal(&session, &generate_id).await;
        assert_eq!(generate_record.status(), RunStatus::Succeeded);
        let result: SingleGenerateResult = generate_record
            .result()
            .unwrap()
            .decode(&SingleGenerateFeature::result_schema())
            .unwrap();
        let manifest_path = project_root.join(&result.artifact_manifest_ref);
        let manifest_bytes = fs::read(&manifest_path).unwrap();
        assert_eq!(sha256(&manifest_bytes), result.manifest_sha256);
        let manifest: ArtifactManifest = serde_json::from_slice(&manifest_bytes).unwrap();
        assert_eq!(manifest.producing_run_id, generate_id);
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
        assert!(!has_staging(&project_root.join("artifacts")));
        assert_eq!(
            model
                .snapshots
                .lock()
                .unwrap()
                .iter()
                .filter(|snapshot| snapshot
                    .request()
                    .messages
                    .iter()
                    .any(|message| message.content.contains("FACADE-CUSTOM-CANARY")))
                .count(),
            2
        );

        session
            .cancel_and_drain(CancellationReason::ProjectClose, Duration::from_secs(5))
            .await
            .unwrap();
        session.release_project_lock().unwrap();
        assert!(ProjectFolder::open(&project_root).is_ok());
    }

    async fn submit_fixture_run(
        session: &Arc<ProjectSession>,
        composition: Arc<Stage2Composition>,
        config: Arc<AppConfig>,
        model: Arc<FixtureModel>,
        run: RunRecord,
    ) -> RunId {
        let project_root = session.path().to_path_buf();
        let project = session.meta().clone();
        session
            .submit(run, move |run, cancellation, repository| async move {
                composition
                    .execute_with_model(
                        &config,
                        &project_root,
                        &project,
                        run,
                        repository.as_ref(),
                        None,
                        &cancellation,
                        model.as_ref(),
                    )
                    .await
            })
            .await
            .unwrap()
    }

    async fn wait_for_terminal(session: &ProjectSession, id: &RunId) -> RunRecord {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let record = session.repository().get(id).unwrap();
                if record.status().is_terminal() {
                    break record;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("fixture Run did not reach a persisted terminal state")
    }

    fn write_truth_fixture(runtime_root: &Path, pack: &LoadedGamePack) {
        let source_bytes = b"public class FixtureGenerated {}";
        let evidence = vec![TruthEvidenceRecord {
            source_id: "fixture-source".into(),
            symbol: "Fixture.Symbol".into(),
            purpose: "Prove the fixture type contract".into(),
            bounded_excerpt: "public class FixtureGenerated".into(),
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
                sha256: sha256(source_bytes),
                byte_length: u64::try_from(source_bytes.len()).unwrap(),
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
        let pack_root = runtime_root.join("truth").join(pack.id().as_str());
        let snapshot_root = pack_root
            .join("snapshots")
            .join(manifest.snapshot_id().as_str());
        fs::create_dir_all(snapshot_root.join("sources")).unwrap();
        fs::create_dir_all(snapshot_root.join("indexes")).unwrap();
        fs::write(snapshot_root.join("sources/fixture.cs"), source_bytes).unwrap();
        fs::write(snapshot_root.join("indexes/fixture.json"), index_bytes).unwrap();
        fs::write(
            snapshot_root.join("truth-snapshot.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(
            pack_root.join("current.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": 1,
                "snapshotId": manifest.snapshot_id(),
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn sha256(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
    }

    fn has_staging(root: &Path) -> bool {
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
}
