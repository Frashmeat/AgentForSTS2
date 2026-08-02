use std::collections::{BTreeMap, BTreeSet};

use ats_game_context::{ContributionResolverError, LoadedGamePack, VerifiedContributionSet};
use ats_kernel::{
    ContributionId, FeatureId, RecipeId, SchemaId, SchemaRef, SchemaVersion, Sha256Digest,
};
use ats_runtime::{
    CancellationToken, FinishReason, ModelClient, ModelError, ModelGamePackRef, ModelRequestError,
    ModelRequestSnapshot, TokenUsage,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;
use crate::prompt::{FeatureRecipe, FeatureRecipeError, FeatureRecipeLoader};

const RECIPE_BYTES: &[u8] = include_bytes!("../recipes/mod-plan.json");
const RECIPE_SHA256: &str = "2a5a1dd5004f0fa1798decc921fa28798155022695c5a4b9191f3680134a83c2";

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
        schema("feature.mod-plan-result")
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
            schema: schema("pack.mod-plan-guidance"),
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
    pub required_evidence: Vec<String>,
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
            || !valid_list(&self.required_evidence, 32, 512, true)
            || !valid_list(&self.required_resource_roles, 32, 128, true)
            || !valid_list(&self.acceptance_criteria, 32, 1_000, false)
            || self
                .required_resource_roles
                .iter()
                .any(|role| !valid_role(role))
        {
            return Err(ModPlanError::InvalidModelOutput);
        }
        Ok(())
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
    supported_item_types: Vec<String>,
    guidance: Vec<String>,
}

impl PlanGuidance {
    fn validate(&self) -> Result<(), ModPlanError> {
        if self.supported_item_types.is_empty()
            || self.supported_item_types.len() > 64
            || self
                .supported_item_types
                .iter()
                .any(|item| !valid_slug(item))
            || self
                .supported_item_types
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != self.supported_item_types.len()
            || !valid_list(&self.guidance, 64, 2_000, false)
        {
            return Err(ModPlanError::InvalidPackGuidance);
        }
        Ok(())
    }
}

pub struct ModPlanContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
    pub project_context: Option<&'a str>,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
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
            || recipe.output_contract().schema != ModPlanFeature::result_schema()
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
        if cancellation.is_cancelled() {
            return Err(ModPlanError::Cancelled);
        }
        let guidance: PlanGuidance = context.contributions.decode(&guidance_slot())?;
        guidance.validate()?;
        if request.item_type.as_ref().is_some_and(|item_type| {
            !guidance
                .supported_item_types
                .iter()
                .any(|supported| supported == item_type)
        }) {
            return Err(ModPlanError::UnsupportedItemType);
        }

        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&self.recipe.output_contract().json_schema)?,
            ),
            ("pack.guidance".into(), serialize(&guidance)?),
            (
                "project.context".into(),
                bounded_optional(context.project_context, 8_000)?,
            ),
            (
                "runtime.custom_instructions".into(),
                bounded_optional(context.custom_instructions, 4_000)?,
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
        let item: PlanItem = serde_json::from_str(&response.content)
            .map_err(|_| ModPlanError::InvalidModelOutput)?;
        item.validate()?;
        if !guidance
            .supported_item_types
            .iter()
            .any(|supported| supported == &item.item_type)
            || request
                .item_type
                .as_ref()
                .is_some_and(|requested| requested != &item.item_type)
        {
            return Err(ModPlanError::UnsupportedItemType);
        }
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

fn schema(id: &str) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(1).expect("built-in schema version is valid"),
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
    use ats_kernel::PrimitiveId;
    use ats_runtime::{ModelResponse, ModelStream};
    use futures_util::stream;
    use sha2::{Digest, Sha256};

    use super::*;

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
        let value = serde_json::json!({
            "schemaVersion":2,
            "id":format!("fixture-{label}"),
            "displayName":format!("Fixture {label}"),
            "contributions":[{
                "slotId":"mod.plan.guidance",
                "featureId":"mod.plan",
                "schema":{"id":"pack.mod-plan-guidance","version":1},
                "payload":{"supportedItemTypes":[item_type],"guidance":[format!("{label} guidance")]}
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
                    "requiredEvidence":["Fixture.Symbol"],
                    "requiredResourceRoles":[],
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
                        model: None
                    },
                    &CancellationToken::new()
                )
                .await,
            Err(ModPlanError::UnsupportedItemType)
        ));
    }
}
