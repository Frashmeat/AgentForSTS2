use std::path::{Path, PathBuf};
use std::sync::Arc;

use ats_adapters::{
    FileArtifactStore, FileCompositionDraftRepository, FileExecutionGraphRepository,
    FileItemRepository, FileProjectStager, FileProjectWriter, FileResourceRepository,
    FileTruthSnapshotRepository, HttpMediaClient, HttpModelClient, ModelRequestQueue,
    PngResourceMediaProcessor, RegisteredBuildRunner, RegisteredValidationRunner, ZipPackageWriter,
};
use ats_features::FeatureSpec;
use ats_features::composition_generate::{
    CompositionGenerateContext, CompositionGenerateDependencies, CompositionGenerateFeature,
    CompositionGenerateRequest, CompositionGenerateService, StagedCompositionGenerateStart,
};
use ats_features::composition_plan::{
    CompositionPlanContext, CompositionPlanFeature, CompositionPlanRequest, CompositionPlanService,
    CompositionRetryNodeContext, CompositionRetryNodeFeature, CompositionRetryNodeService,
    StagedCompositionStart,
};
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
    BehaviorAdapterRegistry, ContributionRequirement, ContributionResolver, EvidenceQuery,
    GamePackLoader, GamePipelineRegistry, LoadedGamePack, TruthSnapshotRepository,
    VerifiedContributionSet, VerifiedTruthSnapshot, built_in_game_pack_asset,
};
use ats_game_sts2::{Sts2BehaviorAdapter, Sts2PipelineProvider};
use ats_kernel::{FailureCode, FeatureId, PrimitiveId, SchemaVersion};
use ats_runtime::{
    CancellationToken, MediaError, ModelClient, ModelRequestLimits, RunFailure, RunRecord,
    RunRepository, RunStatus, RunTransition, VersionedPayload,
};
use ats_workspace::{LocalBuildPaths, ProjectMeta, sync_or_validate_project_local_props};
use chrono::Utc;

use crate::AppConfig;

pub struct Stage2Composition {
    pack: LoadedGamePack,
    contributions: ContributionResolver,
    registry: FeatureRegistry,
    runtime_root: PathBuf,
    model_queue: Arc<ModelRequestQueue>,
    behavior_adapters: BehaviorAdapterRegistry,
    pipelines: GamePipelineRegistry,
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
        let pipeline_primitives = [
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
                PrimitiveId::parse(id).expect("built-in pipeline Primitive ID is valid"),
                SchemaVersion::new(1).expect("built-in pipeline version is valid"),
            )
        });
        let mut pipelines = GamePipelineRegistry::new(pipeline_primitives).map_err(|_| ())?;
        pipelines
            .register(Sts2PipelineProvider::new())
            .map_err(|_| ())?;
        let mut behavior_adapters = BehaviorAdapterRegistry::new();
        behavior_adapters
            .register(Sts2BehaviorAdapter::new())
            .map_err(|_| ())?;
        behavior_adapters
            .resolve_exact(pack.behavior_adapter())
            .map_err(|_| ())?;
        Ok(Self {
            pack,
            contributions,
            registry,
            runtime_root,
            model_queue: Arc::new(ModelRequestQueue::new()),
            behavior_adapters,
            pipelines,
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

    fn resolve_optional(
        &self,
        feature_id: &FeatureId,
        requirement: ContributionRequirement,
    ) -> Result<Option<VerifiedContributionSet>, RunFailure> {
        if self.pack.has_contribution(&requirement.slot_id) {
            self.resolve(feature_id, &[requirement]).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn current_truth(&self) -> Result<VerifiedTruthSnapshot, RunFailure> {
        let truth = FileTruthSnapshotRepository::new(self.runtime_root.clone())
            .open_current(&self.pack)
            .map_err(|_| failure("truth.invalid", "feature.truth"))?
            .ok_or_else(|| failure("truth.missing", "feature.truth"))?;
        self.ensure_behavior_readiness(&truth)?;
        Ok(truth)
    }

    fn ensure_behavior_readiness(&self, truth: &VerifiedTruthSnapshot) -> Result<(), RunFailure> {
        let manifest = truth.manifest();
        if manifest.game_pack_id() != self.pack.id()
            || manifest.game_pack_sha256() != self.pack.content_sha256()
        {
            return Err(failure("truth.invalid", "feature.behavior_readiness"));
        }
        let catalog_identity = self
            .pack
            .capability_catalog()
            .identity()
            .map_err(|_| failure("pack.behavior_invalid", "feature.behavior_readiness"))?;
        if &catalog_identity != self.pack.capability_catalog_identity()
            || &self.pack.capability_catalog().adapter != self.pack.behavior_adapter()
        {
            return Err(failure(
                "pack.behavior_invalid",
                "feature.behavior_readiness",
            ));
        }
        self.behavior_adapters
            .resolve_exact(self.pack.behavior_adapter())
            .map_err(|_| failure("game.adapter_unavailable", "feature.behavior_readiness"))?;
        Ok(())
    }

    pub fn prepare_composition_plan_start(
        &self,
        request: CompositionPlanRequest,
        run_id: ats_runtime::RunId,
        model_request_limits: ModelRequestLimits,
    ) -> Result<StagedCompositionStart, RunFailure> {
        let truth = self.current_truth()?;
        let contributions = self.resolve(
            &CompositionPlanFeature::id(),
            &[CompositionPlanFeature::contribution_requirement()],
        )?;
        CompositionPlanService::built_in()
            .map_err(|_| failure("feature.recipe_invalid", "composition.plan.recipe"))?
            .prepare_staged_start(
                request,
                CompositionPlanContext {
                    pack: &self.pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits,
                },
                run_id,
            )
            .map_err(|error| error.run_failure())
    }

    pub fn prepare_composition_plan_resume(
        &self,
        graph: ats_runtime::ExecutionGraphRecord,
        expected_revision: u64,
        run_id: ats_runtime::RunId,
    ) -> Result<StagedCompositionStart, RunFailure> {
        let truth = self.current_truth()?;
        let contributions = self.resolve(
            &CompositionPlanFeature::id(),
            &[CompositionPlanFeature::contribution_requirement()],
        )?;
        CompositionPlanService::built_in()
            .map_err(|_| failure("feature.recipe_invalid", "composition.plan.recipe"))?
            .prepare_staged_resume(
                graph,
                expected_revision,
                run_id,
                CompositionPlanContext {
                    pack: &self.pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    model_request_limits: ModelRequestLimits::default(),
                },
            )
            .map_err(|error| error.run_failure())
    }

    pub fn prepare_composition_generate_start(
        &self,
        request: CompositionGenerateRequest,
        run_id: ats_runtime::RunId,
        project_root: &Path,
        items: &FileItemRepository,
        resources: &FileResourceRepository,
        model_request_limits: ModelRequestLimits,
    ) -> Result<StagedCompositionGenerateStart, RunFailure> {
        let truth = self.current_truth()?;
        let composition_contributions = self.resolve(
            &CompositionGenerateFeature::id(),
            &[CompositionGenerateFeature::contribution_requirement()],
        )?;
        let plan_contributions = self.resolve(
            &ModPlanFeature::id(),
            &[ModPlanFeature::contribution_requirement()],
        )?;
        let resource_contributions = self.resolve(
            &ResourcePrepareFeature::id(),
            &[ResourcePrepareFeature::contribution_requirement()],
        )?;
        let build_contributions = self.resolve_optional(
            &ProjectBuildFeature::id(),
            ProjectBuildFeature::contribution_requirement(),
        )?;
        let package_contributions = self.resolve_optional(
            &ProjectPackageFeature::id(),
            ProjectPackageFeature::contribution_requirement(),
        )?;
        let plan = ModPlanService::built_in()
            .map_err(|_| failure("feature.recipe_invalid", "composition.generate.plan_recipe"))?;
        CompositionGenerateService::new(
            &plan,
            &ProjectBuildService,
            &ProjectPackageService,
            &self.pipelines,
        )
        .prepare_staged_start(
            request,
            CompositionGenerateContext {
                pack: &self.pack,
                composition_contributions: &composition_contributions,
                plan_contributions: &plan_contributions,
                resource_contributions: &resource_contributions,
                build_contributions: build_contributions.as_ref(),
                package_contributions: package_contributions.as_ref(),
                truth: &truth,
                project_root,
                project_context: "",
                custom_instructions: None,
                model: None,
                model_request_limits,
            },
            items,
            resources,
            run_id,
        )
        .map_err(|error| error.run_failure())
    }

    pub fn prepare_composition_generate_resume(
        &self,
        graph: ats_runtime::ExecutionGraphRecord,
        expected_revision: u64,
        run_id: ats_runtime::RunId,
        project_root: &Path,
    ) -> Result<StagedCompositionGenerateStart, RunFailure> {
        let truth = self.current_truth()?;
        let composition_contributions = self.resolve(
            &CompositionGenerateFeature::id(),
            &[CompositionGenerateFeature::contribution_requirement()],
        )?;
        let plan_contributions = self.resolve(
            &ModPlanFeature::id(),
            &[ModPlanFeature::contribution_requirement()],
        )?;
        let resource_contributions = self.resolve(
            &ResourcePrepareFeature::id(),
            &[ResourcePrepareFeature::contribution_requirement()],
        )?;
        let build_contributions = self.resolve_optional(
            &ProjectBuildFeature::id(),
            ProjectBuildFeature::contribution_requirement(),
        )?;
        let package_contributions = self.resolve_optional(
            &ProjectPackageFeature::id(),
            ProjectPackageFeature::contribution_requirement(),
        )?;
        let plan = ModPlanService::built_in()
            .map_err(|_| failure("feature.recipe_invalid", "composition.generate.plan_recipe"))?;
        CompositionGenerateService::new(
            &plan,
            &ProjectBuildService,
            &ProjectPackageService,
            &self.pipelines,
        )
        .prepare_staged_resume(
            graph,
            expected_revision,
            run_id,
            CompositionGenerateContext {
                pack: &self.pack,
                composition_contributions: &composition_contributions,
                plan_contributions: &plan_contributions,
                resource_contributions: &resource_contributions,
                build_contributions: build_contributions.as_ref(),
                package_contributions: package_contributions.as_ref(),
                truth: &truth,
                project_root,
                project_context: "",
                custom_instructions: None,
                model: None,
                model_request_limits: ModelRequestLimits::default(),
            },
        )
        .map_err(|error| error.run_failure())
    }

    pub fn prepare_composition_generate_adjustment(
        &self,
        graph: ats_runtime::ExecutionGraphRecord,
        expected_revision: u64,
        run_id: ats_runtime::RunId,
        adjustment: ats_features::composition_generate::ItemAdjustment,
        project_root: &Path,
    ) -> Result<StagedCompositionGenerateStart, RunFailure> {
        let truth = self.current_truth()?;
        let composition_contributions = self.resolve(
            &CompositionGenerateFeature::id(),
            &[CompositionGenerateFeature::contribution_requirement()],
        )?;
        let plan_contributions = self.resolve(
            &ModPlanFeature::id(),
            &[ModPlanFeature::contribution_requirement()],
        )?;
        let resource_contributions = self.resolve(
            &ResourcePrepareFeature::id(),
            &[ResourcePrepareFeature::contribution_requirement()],
        )?;
        let build_contributions = self.resolve_optional(
            &ProjectBuildFeature::id(),
            ProjectBuildFeature::contribution_requirement(),
        )?;
        let package_contributions = self.resolve_optional(
            &ProjectPackageFeature::id(),
            ProjectPackageFeature::contribution_requirement(),
        )?;
        let plan = ModPlanService::built_in()
            .map_err(|_| failure("feature.recipe_invalid", "composition.generate.plan_recipe"))?;
        CompositionGenerateService::new(
            &plan,
            &ProjectBuildService,
            &ProjectPackageService,
            &self.pipelines,
        )
        .prepare_staged_adjustment(
            graph,
            expected_revision,
            run_id,
            adjustment,
            CompositionGenerateContext {
                pack: &self.pack,
                composition_contributions: &composition_contributions,
                plan_contributions: &plan_contributions,
                resource_contributions: &resource_contributions,
                build_contributions: build_contributions.as_ref(),
                package_contributions: package_contributions.as_ref(),
                truth: &truth,
                project_root,
                project_context: "",
                custom_instructions: None,
                model: None,
                model_request_limits: ModelRequestLimits::default(),
            },
        )
        .map_err(|error| error.run_failure())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn execute(
        &self,
        config: &AppConfig,
        project_root: &Path,
        project: &ProjectMeta,
        run: RunRecord,
        repository: &dyn RunRepository,
        items: &FileItemRepository,
        drafts: &FileCompositionDraftRepository,
        graphs: &FileExecutionGraphRepository,
        resources: &FileResourceRepository,
        source_path: Option<PathBuf>,
        cancellation: &CancellationToken,
    ) -> Result<RunRecord, RunFailure> {
        self.execute_inner(
            config,
            project_root,
            project,
            run,
            repository,
            items,
            drafts,
            graphs,
            resources,
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
        resources: &FileResourceRepository,
        source_path: Option<PathBuf>,
        cancellation: &CancellationToken,
        model: &dyn ModelClient,
    ) -> Result<RunRecord, RunFailure> {
        let items = FileItemRepository::new(project_root.to_path_buf());
        let drafts = FileCompositionDraftRepository::new(project_root.to_path_buf());
        let graphs = FileExecutionGraphRepository::new(project_root.to_path_buf());
        self.execute_inner(
            config,
            project_root,
            project,
            run,
            repository,
            &items,
            &drafts,
            &graphs,
            resources,
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
        items: &FileItemRepository,
        drafts: &FileCompositionDraftRepository,
        graphs: &FileExecutionGraphRepository,
        resources: &FileResourceRepository,
        source_path: Option<PathBuf>,
        cancellation: &CancellationToken,
        model_override: Option<&dyn ModelClient>,
    ) -> Result<RunRecord, RunFailure> {
        self.registry
            .validate_request(run.feature_id(), run.request())
            .map_err(|_| failure("run.input_invalid", "feature.request"))?;
        let settings = config.settings_snapshot();
        if matches!(
            run.feature_id().as_str(),
            "composition.generate"
                | "mod.generate.single"
                | "mod.generate.batch"
                | "mod.generate.complex"
                | "project.build"
                | "project.package"
        ) {
            sync_or_validate_project_local_props(
                project_root,
                &LocalBuildPaths {
                    sts2_assembly_path: settings.knowledge.sts2_dll_path.clone().into(),
                    godot_executable_path: settings.toolchain.godot_exe_path.clone().into(),
                },
            )
            .map_err(|_| failure("project.local_environment_invalid", "feature.local_props"))?;
        }
        let project_context = format!(
            "Project name: {}; Mod ID: {}; Game Pack: {}.",
            project.name, project.csharp_name, project.game_id
        );
        let custom_instructions = (!settings.llm.custom_prompt.trim().is_empty())
            .then_some(settings.llm.custom_prompt.as_str());
        let model_name =
            (!settings.llm.model.trim().is_empty()).then_some(settings.llm.model.clone());

        match run.feature_id().as_str() {
            "composition.generate" => {
                let request = self.decode::<CompositionGenerateFeature>(&run)?;
                let truth = self.current_truth()?;
                let composition_contributions = self.resolve(
                    &CompositionGenerateFeature::id(),
                    &[CompositionGenerateFeature::contribution_requirement()],
                )?;
                let plan_contributions = self.resolve(
                    &ModPlanFeature::id(),
                    &[ModPlanFeature::contribution_requirement()],
                )?;
                let resource_contributions = self.resolve(
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )?;
                let build_contributions = self.resolve_optional(
                    &ProjectBuildFeature::id(),
                    ProjectBuildFeature::contribution_requirement(),
                )?;
                let package_contributions = self.resolve_optional(
                    &ProjectPackageFeature::id(),
                    ProjectPackageFeature::contribution_requirement(),
                )?;
                let model = select_staged_model(
                    model_override,
                    &settings.llm,
                    Arc::clone(&self.model_queue),
                )?;
                let plan = ModPlanService::built_in().map_err(|_| {
                    failure("feature.recipe_invalid", "composition.generate.plan_recipe")
                })?;
                let build = ProjectBuildService;
                let package = ProjectPackageService;
                let writer = FileProjectWriter;
                let stager = FileProjectStager;
                let validator = RegisteredValidationRunner;
                let artifacts = FileArtifactStore::new(project_root.to_path_buf());
                let build_runner = RegisteredBuildRunner;
                let package_writer = ZipPackageWriter;
                CompositionGenerateService::new(&plan, &build, &package, &self.pipelines)
                    .execute_staged(
                        CompositionGenerateDependencies {
                            model: model.client(),
                            items,
                            resources,
                            writer: &writer,
                            stager: &stager,
                            validator: &validator,
                            artifacts: &artifacts,
                            build_runner: &build_runner,
                            package_writer: &package_writer,
                            behavior_adapters: &self.behavior_adapters,
                        },
                        repository,
                        graphs,
                        &mut run,
                        request,
                        CompositionGenerateContext {
                            pack: &self.pack,
                            composition_contributions: &composition_contributions,
                            plan_contributions: &plan_contributions,
                            resource_contributions: &resource_contributions,
                            build_contributions: build_contributions.as_ref(),
                            package_contributions: package_contributions.as_ref(),
                            truth: &truth,
                            project_root,
                            project_context: &project_context,
                            custom_instructions,
                            model: model_name,
                            model_request_limits: ModelRequestLimits::default(),
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|error| error.run_failure())?;
            }
            "composition.plan" => {
                let request = self.decode::<CompositionPlanFeature>(&run)?;
                let truth = self.current_truth()?;
                let contributions = self.resolve(
                    &CompositionPlanFeature::id(),
                    &[CompositionPlanFeature::contribution_requirement()],
                )?;
                let model = select_staged_model(
                    model_override,
                    &settings.llm,
                    Arc::clone(&self.model_queue),
                )?;
                let service = CompositionPlanService::built_in()
                    .map_err(|_| failure("feature.recipe_invalid", "composition.plan.recipe"))?;
                let plan_context = CompositionPlanContext {
                    pack: &self.pack,
                    contributions: &contributions,
                    truth: &truth,
                    project_context: Some(&project_context),
                    custom_instructions,
                    model: model_name,
                    model_request_limits: ModelRequestLimits::default(),
                };
                if request.execution.is_none() {
                    return Err(failure("run.input_invalid", "composition.plan.execution"));
                }
                let result = service
                    .execute_staged(
                        model.client(),
                        items,
                        drafts,
                        graphs,
                        run.id(),
                        request,
                        plan_context,
                        cancellation,
                    )
                    .await
                    .map_err(|error| error.run_failure())?
                    .result;
                succeed::<CompositionPlanFeature, _>(&mut run, &result)?;
            }
            "composition.retry-node" => {
                let model_request_limits = request_limits(&settings.llm)?;
                let request = self.decode::<CompositionRetryNodeFeature>(&run)?;
                let truth = self.current_truth()?;
                let plan_contributions = self.resolve(
                    &CompositionPlanFeature::id(),
                    &[CompositionPlanFeature::contribution_requirement()],
                )?;
                let retry_contributions = self.resolve(
                    &CompositionRetryNodeFeature::id(),
                    &[CompositionRetryNodeFeature::contribution_requirement()],
                )?;
                let model =
                    select_model(model_override, &settings.llm, Arc::clone(&self.model_queue))?;
                let execution = CompositionRetryNodeService::built_in()
                    .map_err(|_| {
                        failure("feature.recipe_invalid", "composition.retry-node.recipe")
                    })?
                    .execute(
                        model.client(),
                        drafts,
                        request,
                        CompositionRetryNodeContext {
                            pack: &self.pack,
                            plan_contributions: &plan_contributions,
                            retry_contributions: &retry_contributions,
                            truth: &truth,
                            project_context: Some(&project_context),
                            custom_instructions,
                            model: model_name,
                            model_request_limits,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|error| error.run_failure())?;
                succeed::<CompositionRetryNodeFeature, _>(&mut run, &execution.result)?;
            }
            "mod.plan" => {
                let model_request_limits = request_limits(&settings.llm)?;
                let request = self.decode::<ModPlanFeature>(&run)?;
                let contributions = self.resolve(
                    &ModPlanFeature::id(),
                    &[ModPlanFeature::contribution_requirement()],
                )?;
                let model =
                    select_model(model_override, &settings.llm, Arc::clone(&self.model_queue))?;
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
                            model_request_limits,
                            authoritative_definition: None,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|error| error.run_failure())?;
                succeed::<ModPlanFeature, _>(&mut run, &execution.item)?;
            }
            "mod.generate.single" => {
                let model_request_limits = request_limits(&settings.llm)?;
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
                let model =
                    select_model(model_override, &settings.llm, Arc::clone(&self.model_queue))?;
                let adapters = SingleAdapters::new(project_root);
                SingleGenerateService::built_in()
                    .map_err(|_| failure("feature.recipe_invalid", "mod.generate.single.recipe"))?
                    .execute(
                        adapters.dependencies(model.client(), resources),
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
                            model_request_limits,
                        },
                        cancellation,
                    )
                    .await
                    .map_err(|error| error.run_failure())?;
            }
            "mod.generate.batch" => {
                let model_request_limits = request_limits(&settings.llm)?;
                let request = self.decode::<BatchGenerateFeature>(&run)?;
                let truth = self.current_truth()?;
                let batch_contributions = self.resolve(
                    &BatchGenerateFeature::id(),
                    &[BatchGenerateFeature::contribution_requirement()],
                )?;
                let plan_contributions = self.resolve(
                    &ModPlanFeature::id(),
                    &[ModPlanFeature::contribution_requirement()],
                )?;
                let single_contributions = self.resolve(
                    &SingleGenerateFeature::id(),
                    &[SingleGenerateFeature::contribution_requirement()],
                )?;
                let resource_contributions = self.resolve(
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )?;
                let model =
                    select_model(model_override, &settings.llm, Arc::clone(&self.model_queue))?;
                let adapters = SingleAdapters::new(project_root);
                let plan = ModPlanService::built_in().map_err(|_| {
                    failure("feature.recipe_invalid", "mod.generate.batch.plan_recipe")
                })?;
                let single = SingleGenerateService::built_in()
                    .map_err(|_| failure("feature.recipe_invalid", "mod.generate.batch.recipe"))?;
                let execution = BatchGenerateService::new(&plan, &single)
                    .execute(
                        adapters.dependencies(model.client(), resources),
                        &mut run,
                        request,
                        BatchGenerateContext {
                            pack: &self.pack,
                            batch_contributions: &batch_contributions,
                            plan_contributions: &plan_contributions,
                            single_contributions: &single_contributions,
                            resource_contributions: &resource_contributions,
                            truth: &truth,
                            project_root,
                            project_context: &project_context,
                            custom_instructions,
                            model: model_name,
                            model_request_limits,
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
                let model_request_limits = request_limits(&settings.llm)?;
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
                let model =
                    select_model(model_override, &settings.llm, Arc::clone(&self.model_queue))?;
                let plan = ModPlanService::built_in().map_err(|_| {
                    failure("feature.recipe_invalid", "mod.generate.complex.plan_recipe")
                })?;
                let single = SingleGenerateService::built_in().map_err(|_| {
                    failure(
                        "feature.recipe_invalid",
                        "mod.generate.complex.single_recipe",
                    )
                })?;
                let batch = BatchGenerateService::new(&plan, &single);
                let build = ProjectBuildService;
                let package = ProjectPackageService;
                let writer = FileProjectWriter;
                let validator = RegisteredValidationRunner;
                let artifacts = FileArtifactStore::new(project_root.to_path_buf());
                let build_runner = RegisteredBuildRunner;
                let package_writer = ZipPackageWriter;
                let execution = ComplexGenerateService::new(&batch, &build, &package)
                    .execute(
                        ComplexGenerateDependencies {
                            model: model.client(),
                            resources,
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
                            model_request_limits,
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
                let model_request_limits = request_limits(&settings.llm)?;
                let request = self.decode::<LogAnalyzeFeature>(&run)?;
                let truth = self.current_truth()?;
                let contributions = self.resolve(
                    &LogAnalyzeFeature::id(),
                    &[LogAnalyzeFeature::contribution_requirement()],
                )?;
                let model =
                    select_model(model_override, &settings.llm, Arc::clone(&self.model_queue))?;
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
                            model_request_limits,
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
                let context = ResourcePrepareContext {
                    pack: &self.pack,
                    contributions: &contributions,
                };
                let result = match &request.source {
                    ResourcePrepareSource::AiGenerated { .. } => {
                        let media = HttpMediaClient::new(&settings.image_gen).map_err(|_| {
                            failure("resource.media_configuration", "resource.prepare.media")
                        })?;
                        ResourcePrepareService
                            .prepare_ai(
                                &media,
                                &PngResourceMediaProcessor,
                                resources,
                                request,
                                context,
                                cancellation,
                            )
                            .await
                            .map_err(resource_prepare_failure)?
                    }
                    ResourcePrepareSource::PackDefault => ResourcePrepareService
                        .prepare_default(
                            &PngResourceMediaProcessor,
                            resources,
                            request,
                            |pack, asset_id| built_in_game_pack_asset(pack, asset_id).ok(),
                            context,
                        )
                        .map_err(resource_prepare_failure)?,
                    ResourcePrepareSource::UserUpload => {
                        let source_path =
                            source_path.filter(|path| path.is_file()).ok_or_else(|| {
                                failure("resource.source_missing", "resource.prepare.source")
                            })?;
                        ResourcePrepareService
                            .prepare_file(
                                &PngResourceMediaProcessor,
                                resources,
                                request,
                                source_path,
                                context,
                            )
                            .map_err(resource_prepare_failure)?
                    }
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
    queue: Arc<ModelRequestQueue>,
) -> Result<SelectedModel<'a>, RunFailure> {
    model.map_or_else(
        || {
            HttpModelClient::new_with_queue(config, queue)
                .map(SelectedModel::Owned)
                .map_err(|_| failure("model.configuration", "feature.model"))
        },
        |client| Ok(SelectedModel::Borrowed(client)),
    )
}

fn select_staged_model<'a>(
    model: Option<&'a dyn ModelClient>,
    config: &ats_adapters::LlmConfig,
    queue: Arc<ModelRequestQueue>,
) -> Result<SelectedModel<'a>, RunFailure> {
    let mut transport_config = config.clone();
    transport_config.max_output_tokens = None;
    select_model(model, &transport_config, queue)
}

fn request_limits(config: &ats_adapters::LlmConfig) -> Result<ModelRequestLimits, RunFailure> {
    config
        .model_request_limits()
        .map_err(|_| failure("model.configuration", "feature.model"))
}

struct SingleAdapters {
    writer: FileProjectWriter,
    validator: RegisteredValidationRunner,
    artifacts: FileArtifactStore,
}

impl SingleAdapters {
    fn new(project_root: &Path) -> Self {
        Self {
            writer: FileProjectWriter,
            validator: RegisteredValidationRunner,
            artifacts: FileArtifactStore::new(project_root.to_path_buf()),
        }
    }

    fn dependencies<'a>(
        &'a self,
        model: &'a dyn ModelClient,
        resources: &'a FileResourceRepository,
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
            resources,
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
        ResourcePrepareError::InvalidInput => {
            failure("validation.input_invalid", "resource.prepare.input")
        }
        ResourcePrepareError::ContextIdentityMismatch
        | ResourcePrepareError::InvalidPackSpecs
        | ResourcePrepareError::Contribution(_) => {
            failure("pack.contribution_invalid", "resource.prepare.pack")
        }
        ResourcePrepareError::UnsupportedResource => {
            failure("resource.unsupported", "resource.prepare.input")
        }
        ResourcePrepareError::WrongSourceMode => {
            failure("resource.source_invalid", "resource.prepare.source")
        }
        ResourcePrepareError::InvalidMediaResponse | ResourcePrepareError::InvalidMedia => {
            failure("resource.media_invalid", "resource.prepare.media")
        }
        ResourcePrepareError::PackAssetUnavailable => {
            failure("resource.pack_asset_missing", "resource.prepare.pack_asset")
        }
        ResourcePrepareError::PackAssetInvalid => {
            failure("resource.pack_asset_invalid", "resource.prepare.pack_asset")
        }
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
    use ats_features::mod_generate_single::{
        SingleGeneratePublication, SingleGenerateRequest, SingleGenerateResult,
    };
    use ats_features::mod_plan::{ModPlanRequest, PlanItem};
    use ats_game_context::{
        TruthEvidenceRecord, TruthSnapshotIndex, TruthSnapshotManifest, TruthSnapshotSource,
        built_in_project_template,
    };
    use ats_kernel::{ItemId, ItemTypeId, PrimitiveId, Sha256Digest};
    use ats_runtime::{
        ArtifactManifest, CancellationReason, FinishReason, ModelError, ModelRequestSnapshot,
        ModelResponse, ModelStream, RunId, TokenUsage,
    };
    use ats_workspace::{ItemDefinition, ProjectFolder, StoredItemDefinition};
    use futures_util::stream;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    use super::*;
    use crate::project_session::ProjectSession;

    fn custom_code_definition() -> StoredItemDefinition {
        let mut definition = ItemDefinition::new(
            ItemId::parse("fixture_item").unwrap(),
            ItemTypeId::parse("custom_code").unwrap(),
        );
        definition.behavior_intent = vec!["Expose a fixture type.".into()];
        StoredItemDefinition {
            definition_hash: definition.definition_hash().unwrap(),
            definition,
        }
    }

    struct FixtureModel {
        snapshots: Mutex<Vec<ModelRequestSnapshot>>,
        invalid_plan: bool,
    }

    #[async_trait]
    impl ModelClient for FixtureModel {
        async fn complete(
            &self,
            request: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelResponse, ModelError> {
            let content = if request.feature_id() == &ModPlanFeature::id() {
                if self.invalid_plan {
                    serde_json::json!({
                        "itemId": "EvidenceQueryAcceptance20260803",
                        "itemType": "custom_code",
                        "name": "EvidenceQueryAcceptance20260803",
                        "summary": "An invalid facade fixture",
                        "behaviorIntent": ["Expose a fixture type"],
                        "implementationConstraints": [],
                        "evidenceRequirements": ["A verified fixture type declaration"],
                        "acceptanceCriteria": ["The project compiles"]
                    })
                    .to_string()
                } else {
                    serde_json::json!({
                        "itemId": "fixture_item",
                        "itemType": "custom_code",
                        "name": "Fixture Item",
                        "summary": "A deterministic facade fixture",
                        "behaviorIntent": ["Expose a fixture type"],
                        "implementationConstraints": [],
                        "evidenceRequirements": ["A verified fixture type declaration"],
                        "acceptanceCriteria": ["The project compiles"]
                    })
                    .to_string()
                }
            } else {
                serde_json::json!({
                    "files": {
                        "source": "public class FixtureGenerated {}"
                    },
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

    #[test]
    fn built_in_composition_registers_the_pack_behavior_adapter_exactly() {
        let temp = tempdir().unwrap();
        let composition = Stage2Composition::built_in(temp.path().join("runtime")).unwrap();
        assert!(
            composition
                .behavior_adapters
                .resolve_exact(composition.pack().behavior_adapter())
                .is_ok()
        );
    }

    #[test]
    fn resource_prepare_failures_keep_stable_typed_families() {
        let invalid_media = resource_prepare_failure(ResourcePrepareError::InvalidMedia);
        assert_eq!(invalid_media.code.as_str(), "resource.media_invalid");
        assert_eq!(invalid_media.stage, "resource.prepare.media");

        let invalid_pack = resource_prepare_failure(ResourcePrepareError::InvalidPackSpecs);
        assert_eq!(invalid_pack.code.as_str(), "pack.contribution_invalid");
        assert_eq!(invalid_pack.stage, "resource.prepare.pack");

        let unsupported = resource_prepare_failure(ResourcePrepareError::UnsupportedResource);
        assert_eq!(unsupported.code.as_str(), "resource.unsupported");
        assert_eq!(unsupported.stage, "resource.prepare.input");

        let missing = resource_prepare_failure(ResourcePrepareError::PackAssetUnavailable);
        assert_eq!(missing.code.as_str(), "resource.pack_asset_missing");
        assert_eq!(missing.stage, "resource.prepare.pack_asset");

        let invalid = resource_prepare_failure(ResourcePrepareError::PackAssetInvalid);
        assert_eq!(invalid.code.as_str(), "resource.pack_asset_invalid");
        assert_eq!(invalid.stage, "resource.prepare.pack_asset");
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
        let sts2 = temp.path().join("sts2.dll");
        let godot = temp.path().join("godot.exe");
        fs::write(&sts2, b"fixture assembly").unwrap();
        fs::write(&godot, b"fixture tool").unwrap();
        settings.knowledge.sts2_dll_path = sts2.to_string_lossy().into_owned();
        settings.toolchain.godot_exe_path = godot.to_string_lossy().into_owned();
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
            invalid_plan: false,
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
                    definition: custom_code_definition(),
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
        assert_eq!(result.publication, SingleGeneratePublication::Published);
        let manifest_path = project_root.join(
            result
                .artifact_manifest_ref
                .as_deref()
                .expect("published Single result has a manifest ref"),
        );
        let manifest_bytes = fs::read(&manifest_path).unwrap();
        assert_eq!(
            &sha256(&manifest_bytes),
            result
                .manifest_sha256
                .as_ref()
                .expect("published Single result has a manifest hash")
        );
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

    #[tokio::test]
    async fn facade_persists_typed_plan_output_failure() {
        let temp = tempfile::TempDir::new().unwrap();
        let composition = Arc::new(
            Stage2Composition::built_in(temp.path().join("runtime"))
                .expect("fixture composition must load"),
        );
        let template = built_in_project_template(composition.pack(), "sts2.default").unwrap();
        let folder = ProjectFolder::create(
            temp.path(),
            "InvalidPlan",
            composition.pack().id(),
            &template,
        )
        .unwrap();
        let session = ProjectSession::open(folder).unwrap();
        let config = Arc::new(AppConfig::new(
            Settings::default(),
            ConfigStatus {
                path: None,
                file_present: false,
                loaded: false,
                errors: Vec::new(),
            },
        ));
        let model = Arc::new(FixtureModel {
            snapshots: Mutex::new(Vec::new()),
            invalid_plan: true,
        });
        let run = RunRecord::new(
            ModPlanFeature::id(),
            VersionedPayload::from_typed(
                ModPlanFeature::request_schema(),
                &ModPlanRequest {
                    requirements: "Create a C# type named EvidenceQueryAcceptance20260803".into(),
                    item_type: Some("custom_code".into()),
                },
            )
            .unwrap(),
        );

        let id = submit_fixture_run(&session, composition, config, model, run).await;
        let record = wait_for_terminal(&session, &id).await;
        assert_eq!(record.status(), RunStatus::Failed);
        let failure = record.failure().unwrap();
        assert_eq!(failure.code.as_str(), "model.output_invalid");
        assert_eq!(failure.stage, "mod.plan.model");
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
        let resources = session.resource_repository();
        session
            .submit(run, move |run, cancellation, repository| async move {
                composition
                    .execute_with_model(
                        &config,
                        &project_root,
                        &project,
                        run,
                        repository.as_ref(),
                        resources.as_ref(),
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
            symbol: "ICustomModel".into(),
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
