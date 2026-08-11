use std::collections::{BTreeMap, BTreeSet};

use ats_game_context::{
    ContributionResolverError, ItemTypeDescriptor, LoadedGamePack, VerifiedContributionSet,
};
use ats_kernel::{
    ContributionId, FailureCode, FeatureId, ItemTypeId, RecipeId, SchemaId, SchemaRef,
    SchemaVersion, Sha256Digest,
};
use ats_runtime::{
    CancellationToken, FinishReason, ModelClient, ModelError, ModelGamePackRef, ModelRequestError,
    ModelRequestSnapshot, RunFailure, TokenUsage,
};
use ats_workspace::StoredItemDefinition;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;
use crate::prompt::{FeatureRecipe, FeatureRecipeError, FeatureRecipeLoader};

const RECIPE_BYTES: &[u8] = include_bytes!("../recipes/mod-plan.json");
const RECIPE_SHA256: &str = "f177a49d4f7d7bdeb091469f54d2c48dacb6d8a7d7c94ddfb32d8ae7f11f0bfb";

pub struct ModPlanFeature;

impl FeatureSpec for ModPlanFeature {
    type Request = ModPlanRequest;
    type Result = PlanItem;
    type ArtifactExtension = ModPlanArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("mod.plan").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema("feature.mod-plan-request")
    }

    fn result_schema() -> SchemaRef {
        schema_version("feature.mod-plan-result", 2)
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.mod-plan-artifact-extension")
    }
}

impl ModPlanFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: guidance_slot(),
            schema: schema_version("pack.mod-plan-guidance", 3),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModPlanRequest {
    pub requirements: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanItem {
    pub item_id: String,
    pub item_type: String,
    pub name: String,
    pub summary: String,
    pub behavior_intent: Vec<String>,
    pub implementation_constraints: Vec<String>,
    pub evidence_requirements: Vec<String>,
    pub required_resource_roles: Vec<String>,
    pub acceptance_criteria: Vec<String>,
}

impl PlanItem {
    pub fn validate(&self) -> Result<(), ModPlanError> {
        if !valid_slug(&self.item_id)
            || !valid_slug(&self.item_type)
            || !valid_text(&self.name, 256)
            || !valid_text(&self.summary, 2_000)
            || !valid_list(&self.behavior_intent, 32, 1_000, false)
            || !valid_list(&self.implementation_constraints, 32, 1_000, true)
            || !valid_list(&self.evidence_requirements, 32, 512, true)
            || !valid_list(&self.required_resource_roles, 32, 128, true)
            || !valid_list(&self.acceptance_criteria, 32, 1_000, false)
            || self
                .required_resource_roles
                .iter()
                .any(|role| !valid_role(role))
            || self
                .required_resource_roles
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.required_resource_roles.len()
        {
            return Err(ModPlanError::InvalidModelOutput);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelPlanItem {
    item_id: String,
    item_type: String,
    name: String,
    summary: String,
    behavior_intent: Vec<String>,
    implementation_constraints: Vec<String>,
    evidence_requirements: Vec<String>,
    acceptance_criteria: Vec<String>,
}

impl ModelPlanItem {
    fn into_plan_item(self, required_resource_roles: Vec<String>) -> PlanItem {
        PlanItem {
            item_id: self.item_id,
            item_type: self.item_type,
            name: self.name,
            summary: self.summary,
            behavior_intent: self.behavior_intent,
            implementation_constraints: self.implementation_constraints,
            evidence_requirements: self.evidence_requirements,
            required_resource_roles,
            acceptance_criteria: self.acceptance_criteria,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModPlanArtifactExtension {
    pub model_request_sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PlanGuidance {
    guidance: Vec<String>,
}

impl PlanGuidance {
    fn validate(&self) -> Result<(), ModPlanError> {
        if !valid_list(&self.guidance, 64, 2_000, false) {
            return Err(ModPlanError::InvalidPackGuidance);
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanPromptPack<'a> {
    item_types: Vec<&'a ItemTypeDescriptor>,
    guidance: &'a [String],
}

pub struct ModPlanContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
    pub project_context: Option<&'a str>,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
    pub authoritative_definition: Option<&'a StoredItemDefinition>,
}

#[derive(Debug, Clone)]
pub struct ModPlanExecution {
    pub item: PlanItem,
    pub request_snapshot: ModelRequestSnapshot,
    pub response_model: String,
    pub usage: TokenUsage,
}

pub struct ModPlanService {
    recipe: FeatureRecipe,
}

impl ModPlanService {
    pub fn built_in() -> Result<Self, ModPlanError> {
        let expected =
            Sha256Digest::parse(RECIPE_SHA256).map_err(|_| ModPlanError::InvalidRecipeContract)?;
        Self::from_recipe(FeatureRecipeLoader::load(RECIPE_BYTES, &expected)?)
    }

    pub fn from_recipe(recipe: FeatureRecipe) -> Result<Self, ModPlanError> {
        if recipe.feature_id() != &ModPlanFeature::id()
            || recipe.output_contract().schema != model_output_schema()
        {
            return Err(ModPlanError::InvalidRecipeContract);
        }
        Ok(Self { recipe })
    }

    pub async fn execute<C>(
        &self,
        client: &C,
        request: ModPlanRequest,
        context: ModPlanContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<ModPlanExecution, ModPlanError>
    where
        C: ModelClient + ?Sized,
    {
        if request.requirements.trim().is_empty()
            || request.requirements.chars().count() > 16_000
            || request.requirements.contains('\0')
        {
            return Err(ModPlanError::InvalidInput);
        }
        validate_context(&context)?;
        let authoritative_definition = context.authoritative_definition;
        if let Some(definition) = authoritative_definition {
            definition
                .validate()
                .map_err(|_| ModPlanError::InvalidItemDefinition)?;
            if request.item_type.as_deref() != Some(definition.definition.item_type.as_str())
                || request.requirements != definition.definition.behavior_intent.join("\n")
                || context
                    .pack
                    .item_type(&definition.definition.item_type)
                    .is_none()
            {
                return Err(ModPlanError::InvalidItemDefinition);
            }
        }
        if cancellation.is_cancelled() {
            return Err(ModPlanError::Cancelled);
        }
        let guidance: PlanGuidance = context.contributions.decode(&guidance_slot())?;
        guidance.validate()?;
        let requested_item_type = request
            .item_type
            .as_deref()
            .map(ItemTypeId::parse)
            .transpose()
            .map_err(|_| ModPlanError::UnsupportedItemType)?;
        if requested_item_type
            .as_ref()
            .is_some_and(|item_type| context.pack.item_type(item_type).is_none())
        {
            return Err(ModPlanError::UnsupportedItemType);
        }
        let prompt_pack = PlanPromptPack {
            item_types: context.pack.item_types().values().collect(),
            guidance: &guidance.guidance,
        };

        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&self.recipe.output_contract().json_schema)?,
            ),
            ("pack.guidance".into(), serialize(&prompt_pack)?),
            (
                "project.context".into(),
                bounded_optional(context.project_context, 8_000)?,
            ),
            (
                "runtime.custom_instructions".into(),
                bounded_optional(context.custom_instructions, 4_000)?,
            ),
            (
                "item.definition".into(),
                authoritative_definition.map_or_else(|| Ok(String::new()), serialize)?,
            ),
            (
                "request.item_type".into(),
                request.item_type.clone().unwrap_or_default(),
            ),
            ("request.requirements".into(), request.requirements.clone()),
        ]);
        let model_request = self.recipe.render(&slots, context.model)?;
        let snapshot = ModelRequestSnapshot::new(
            ModPlanFeature::id(),
            self.recipe.recipe_ref(),
            ModelGamePackRef {
                id: context.pack.id().clone(),
                sha256: context.pack.content_sha256().clone(),
            },
            None,
            Vec::new(),
            model_request,
        )?;
        let response = client.complete(snapshot.clone(), cancellation).await?;
        if cancellation.is_cancelled() {
            return Err(ModPlanError::Cancelled);
        }
        if response.finish_reason == FinishReason::MaxTokens {
            return Err(ModPlanError::TruncatedModelOutput);
        }
        let model_item: ModelPlanItem = serde_json::from_str(&response.content)
            .map_err(|_| ModPlanError::InvalidModelOutput)?;
        let model_item_type = ItemTypeId::parse(model_item.item_type.as_str())
            .map_err(|_| ModPlanError::UnsupportedItemType)?;
        let item_descriptor = context
            .pack
            .item_type(&model_item_type)
            .ok_or(ModPlanError::UnsupportedItemType)?;
        if request
            .item_type
            .as_ref()
            .is_some_and(|requested| requested != &model_item.item_type)
        {
            return Err(ModPlanError::UnsupportedItemType);
        }
        let mut item = model_item.into_plan_item(
            item_descriptor
                .resource_profiles()
                .iter()
                .flat_map(|profile| profile.required_resource_roles())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .map(ToString::to_string)
                .collect(),
        );
        if let Some(definition) = authoritative_definition {
            item.item_id = definition.definition.item_id.to_string();
            item.item_type = definition.definition.item_type.to_string();
            item.behavior_intent = definition.definition.behavior_intent.clone();
        }
        item.validate()?;
        Ok(ModPlanExecution {
            item,
            request_snapshot: snapshot,
            response_model: response.model,
            usage: response.usage,
        })
    }

    #[must_use]
    pub fn recipe_id(&self) -> &RecipeId {
        self.recipe.id()
    }
}

#[derive(Debug, Error)]
pub enum ModPlanError {
    #[error("Mod plan input is invalid")]
    InvalidInput,
    #[error("Mod plan authoritative ItemDefinition is invalid")]
    InvalidItemDefinition,
    #[error("Mod plan context identities do not match")]
    ContextIdentityMismatch,
    #[error("Mod plan Pack guidance is invalid")]
    InvalidPackGuidance,
    #[error("Mod plan item type is unsupported by the Pack")]
    UnsupportedItemType,
    #[error("Mod plan Recipe does not match its typed contract")]
    InvalidRecipeContract,
    #[error("Mod plan model output was truncated")]
    TruncatedModelOutput,
    #[error("Mod plan model output failed typed validation")]
    InvalidModelOutput,
    #[error("Mod plan was cancelled")]
    Cancelled,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Recipe(#[from] FeatureRecipeError),
    #[error(transparent)]
    Request(#[from] ModelRequestError),
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl ModPlanError {
    #[must_use]
    pub fn run_failure(&self) -> RunFailure {
        let (code, stage) = match self {
            Self::InvalidInput => ("run.input_invalid", "mod.plan.request"),
            Self::InvalidItemDefinition => ("item.definition_invalid", "mod.plan.definition"),
            Self::ContextIdentityMismatch => ("truth.context_mismatch", "mod.plan.context"),
            Self::InvalidPackGuidance | Self::Contribution(_) => {
                ("pack.contribution_invalid", "mod.plan.pack")
            }
            Self::UnsupportedItemType => ("feature.item_type_unsupported", "mod.plan.request"),
            Self::InvalidRecipeContract | Self::Recipe(_) => {
                ("feature.recipe_invalid", "mod.plan.recipe")
            }
            Self::TruncatedModelOutput => ("model.output_truncated", "mod.plan.model"),
            Self::InvalidModelOutput => ("model.output_invalid", "mod.plan.model"),
            Self::Cancelled => ("run.cancelled", "mod.plan.execute"),
            Self::Request(_) => ("model.request_invalid", "mod.plan.model"),
            Self::Model(ModelError::Authentication) => ("model.authentication", "mod.plan.model"),
            Self::Model(ModelError::RateLimited { .. }) => ("model.rate_limited", "mod.plan.model"),
            Self::Model(ModelError::Configuration) => ("model.configuration", "mod.plan.model"),
            Self::Model(ModelError::Transport) => ("model.transport_failed", "mod.plan.model"),
            Self::Model(ModelError::Rejected) => ("model.request_rejected", "mod.plan.model"),
            Self::Model(ModelError::InvalidResponse) => {
                ("model.response_invalid", "mod.plan.model")
            }
            Self::Model(ModelError::Cancelled) => ("run.cancelled", "mod.plan.model"),
        };
        RunFailure::new(
            FailureCode::parse(code).expect("built-in failure code is valid"),
            stage,
            None,
        )
        .expect("built-in Run failure is valid")
    }
}

fn validate_context(context: &ModPlanContext<'_>) -> Result<(), ModPlanError> {
    if context.contributions.feature_id() != &ModPlanFeature::id()
        || context.contributions.game_pack_id() != context.pack.id()
        || context.contributions.game_pack_sha256() != context.pack.content_sha256()
    {
        return Err(ModPlanError::ContextIdentityMismatch);
    }
    Ok(())
}

fn guidance_slot() -> ContributionId {
    ContributionId::parse("mod.plan.guidance").expect("built-in contribution ID is valid")
}

fn model_output_schema() -> SchemaRef {
    schema("feature.mod-plan-model-output")
}

fn schema(id: &str) -> SchemaRef {
    schema_version(id, 1)
}

fn schema_version(id: &str, version: u32) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(version).expect("built-in schema version is valid"),
    }
}

fn serialize<T: Serialize + ?Sized>(value: &T) -> Result<String, ModPlanError> {
    serde_json::to_string_pretty(value).map_err(|_| ModPlanError::InvalidInput)
}

fn bounded_optional(value: Option<&str>, max: usize) -> Result<String, ModPlanError> {
    let value = value.unwrap_or("");
    if value.chars().count() > max || value.contains('\0') {
        return Err(ModPlanError::InvalidInput);
    }
    Ok(value.to_owned())
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_role(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max && !value.contains('\0')
}

fn valid_list(values: &[String], max_items: usize, max_chars: usize, empty_ok: bool) -> bool {
    (empty_ok || !values.is_empty())
        && values.len() <= max_items
        && values.iter().all(|value| valid_text(value, max_chars))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ats_game_context::{ContributionResolver, GamePackLoader};
    use ats_kernel::{ItemId, PrimitiveId};
    use ats_runtime::{ModelResponse, ModelStream};
    use ats_workspace::ItemDefinition;
    use futures_util::stream;
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::mod_generate_single::SingleGenerateFeature;

    struct MockModel {
        snapshots: Mutex<Vec<ModelRequestSnapshot>>,
        response: ModelResponse,
    }

    #[async_trait]
    impl ModelClient for MockModel {
        async fn complete(
            &self,
            request: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelResponse, ModelError> {
            self.snapshots.lock().unwrap().push(request);
            Ok(self.response.clone())
        }

        async fn stream(
            &self,
            _: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelStream, ModelError> {
            Ok(Box::pin(stream::empty()))
        }
    }

    fn digest(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
    }

    fn pack(label: &str, item_type: &str) -> LoadedGamePack {
        pack_with_roles(label, item_type, &[])
    }

    fn pack_with_roles(label: &str, item_type: &str, required_roles: &[&str]) -> LoadedGamePack {
        let value = serde_json::json!({
            "schemaVersion":4,
            "id":format!("fixture-{label}"),
            "displayName":format!("Fixture {label}"),
            "itemTypes":[{
                "id":item_type,
                "displayNames":{"eng":"Fixture type"},
                "requiredLocales":[],
                "fields":[],
                "evidenceQueries":[{"symbols":["FixtureType"],"terms":[]}],
                "resourceProfiles":[{
                    "id":"default",
                    "displayNames":{"eng":"Default"},
                    "requiredResourceRoles":required_roles
                }]
            }],
            "contributions":[{
                "slotId":"mod.plan.guidance",
                "featureId":"mod.plan",
                "schema":{"id":"pack.mod-plan-guidance","version":3},
                "payload":{
                    "guidance":[format!("{label} guidance")]
                }
            }]
        });
        let bytes = serde_json::to_vec(&value).unwrap();
        GamePackLoader::load(&bytes, &digest(&bytes)).unwrap()
    }

    fn contributions(pack: &LoadedGamePack) -> VerifiedContributionSet {
        ContributionResolver::new(Vec::<PrimitiveId>::new())
            .resolve(
                pack,
                &ModPlanFeature::id(),
                &[ModPlanFeature::contribution_requirement()],
            )
            .unwrap()
    }

    fn stored_definition(item_id: &str, item_type: &str) -> StoredItemDefinition {
        let mut definition = ItemDefinition::new(
            ItemId::parse(item_id).unwrap(),
            ItemTypeId::parse(item_type).unwrap(),
        );
        definition.behavior_intent = vec!["Use the confirmed behavior exactly".into()];
        StoredItemDefinition {
            definition_hash: definition.definition_hash().unwrap(),
            definition,
        }
    }

    fn model(item_type: &str) -> MockModel {
        MockModel {
            snapshots: Mutex::new(Vec::new()),
            response: ModelResponse {
                model: "fixture".into(),
                content: serde_json::json!({
                    "itemId":"fixture_item",
                    "itemType":item_type,
                    "name":"Fixture Item",
                    "summary":"A bounded fixture item",
                    "behaviorIntent":["Produce one observable behavior"],
                    "implementationConstraints":[],
                    "evidenceRequirements":["A verified fixture type declaration"],
                    "acceptanceCriteria":["The fixture compiles"]
                })
                .to_string(),
                finish_reason: FinishReason::EndTurn,
                usage: TokenUsage::default(),
            },
        }
    }

    #[tokio::test]
    async fn different_pack_guidance_changes_request_and_strict_result_decodes() {
        let service = ModPlanService::built_in().unwrap();
        let mut hashes = Vec::new();
        for label in ["alpha", "beta"] {
            let pack = pack(label, "fixture_type");
            let contributions = contributions(&pack);
            let execution = service
                .execute(
                    &model("fixture_type"),
                    ModPlanRequest {
                        requirements: "fixture requirements".into(),
                        item_type: Some("fixture_type".into()),
                    },
                    ModPlanContext {
                        pack: &pack,
                        contributions: &contributions,
                        project_context: None,
                        custom_instructions: Some("CUSTOM-CANARY"),
                        model: None,
                        authoritative_definition: None,
                    },
                    &CancellationToken::new(),
                )
                .await
                .unwrap();
            execution.item.validate().unwrap();
            let rendered = execution
                .request_snapshot
                .request()
                .messages
                .iter()
                .map(|message| message.content.as_str())
                .collect::<String>();
            assert_eq!(rendered.matches("CUSTOM-CANARY").count(), 1);
            hashes.push(execution.request_snapshot.request_sha256().clone());
        }
        assert_ne!(hashes[0], hashes[1]);
    }

    #[tokio::test]
    async fn unsupported_and_missing_typed_fields_are_rejected() {
        let pack = pack("alpha", "supported");
        let contributions = contributions(&pack);
        let service = ModPlanService::built_in().unwrap();
        assert!(matches!(
            service
                .execute(
                    &model("other"),
                    ModPlanRequest {
                        requirements: "fixture".into(),
                        item_type: Some("supported".into())
                    },
                    ModPlanContext {
                        pack: &pack,
                        contributions: &contributions,
                        project_context: None,
                        custom_instructions: None,
                        model: None,
                        authoritative_definition: None
                    },
                    &CancellationToken::new()
                )
                .await,
            Err(ModPlanError::UnsupportedItemType)
        ));
    }

    #[tokio::test]
    async fn pack_roles_are_attached_without_model_authorship() {
        let pack = pack_with_roles("roles", "fixture_type", &["fixture.icon"]);
        let contributions = contributions(&pack);
        let execution = ModPlanService::built_in()
            .unwrap()
            .execute(
                &model("fixture_type"),
                ModPlanRequest {
                    requirements: "fixture".into(),
                    item_type: Some("fixture_type".into()),
                },
                ModPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    authoritative_definition: None,
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(execution.item.required_resource_roles, ["fixture.icon"]);
        let rendered = execution
            .request_snapshot
            .request()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<String>();
        assert!(rendered.contains("fixture.icon"));
    }

    #[tokio::test]
    async fn definition_bound_plan_uses_the_complete_definition_and_rebinds_authoritative_fields() {
        let pack = pack("definition", "fixture_type");
        let contributions = contributions(&pack);
        let definition = stored_definition("confirmed_item", "fixture_type");
        let model = model("fixture_type");
        let execution = ModPlanService::built_in()
            .unwrap()
            .execute(
                &model,
                ModPlanRequest {
                    requirements: definition.definition.behavior_intent.join("\n"),
                    item_type: Some("fixture_type".into()),
                },
                ModPlanContext {
                    pack: &pack,
                    contributions: &contributions,
                    project_context: None,
                    custom_instructions: None,
                    model: None,
                    authoritative_definition: Some(&definition),
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(execution.item.item_id, "confirmed_item");
        assert_eq!(execution.item.item_type, "fixture_type");
        assert_eq!(
            execution.item.behavior_intent,
            definition.definition.behavior_intent
        );
        let rendered = model.snapshots.lock().unwrap()[0]
            .request()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<String>();
        assert!(rendered.contains(definition.definition_hash.as_str()));
        assert!(rendered.contains("authoritative-item-definition"));
        assert!(rendered.contains("sole source of item identity"));
    }

    #[test]
    fn built_in_generation_contribution_covers_item_catalog() {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SingleRoles {
            item_types: Vec<SingleRoleItem>,
        }

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct SingleRoleItem {
            id: String,
        }

        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let resolver =
            ContributionResolver::new([PrimitiveId::parse("code.dotnet-validate").unwrap()]);
        let plan: PlanGuidance = resolver
            .resolve(
                &pack,
                &ModPlanFeature::id(),
                &[ModPlanFeature::contribution_requirement()],
            )
            .unwrap()
            .decode(&guidance_slot())
            .unwrap();
        let single: SingleRoles = resolver
            .resolve(
                &pack,
                &SingleGenerateFeature::id(),
                &[SingleGenerateFeature::contribution_requirement()],
            )
            .unwrap()
            .decode(&ContributionId::parse("mod.generate.single").unwrap())
            .unwrap();
        let catalog_ids = pack
            .item_types()
            .keys()
            .map(ToString::to_string)
            .collect::<BTreeSet<_>>();
        let single_ids = single
            .item_types
            .into_iter()
            .map(|item| item.id)
            .collect::<BTreeSet<_>>();

        plan.validate().unwrap();
        assert_eq!(catalog_ids, single_ids);
    }

    #[test]
    fn recipe_exposes_plan_item_validation_constraints() {
        let service = ModPlanService::built_in().unwrap();
        let properties = service.recipe.output_contract().json_schema["properties"]
            .as_object()
            .unwrap();
        assert_eq!(
            service.recipe.output_contract().schema,
            model_output_schema()
        );
        assert_eq!(
            properties["itemId"]["pattern"],
            serde_json::json!("^[a-z][a-z0-9_-]*$")
        );
        assert_eq!(properties["itemId"]["maxLength"], serde_json::json!(128));
        assert_eq!(
            properties["behaviorIntent"]["minItems"],
            serde_json::json!(1)
        );
        assert_eq!(
            properties["behaviorIntent"]["items"]["pattern"],
            serde_json::json!(r"^[^\u0000]*\S[^\u0000]*$")
        );
        assert_eq!(
            properties["evidenceRequirements"]["items"]["maxLength"],
            serde_json::json!(512)
        );
        assert!(!properties.contains_key("requiredResourceRoles"));
        assert_eq!(
            properties["acceptanceCriteria"]["minItems"],
            serde_json::json!(1)
        );
    }

    #[test]
    fn plan_failures_retain_stable_classification() {
        let invalid_output = ModPlanError::InvalidModelOutput.run_failure();
        assert_eq!(invalid_output.code.as_str(), "model.output_invalid");
        assert_eq!(invalid_output.stage, "mod.plan.model");

        let authentication = ModPlanError::Model(ModelError::Authentication).run_failure();
        assert_eq!(authentication.code.as_str(), "model.authentication");
        assert_eq!(authentication.stage, "mod.plan.model");

        let cancelled = ModPlanError::Cancelled.run_failure();
        assert_eq!(cancelled.code.as_str(), "run.cancelled");
        assert_eq!(cancelled.stage, "mod.plan.execute");
    }
}
