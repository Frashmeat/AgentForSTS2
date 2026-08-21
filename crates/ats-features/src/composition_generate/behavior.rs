use std::collections::BTreeMap;

use ats_game_context::{
    BEHAVIOR_PROPOSAL_SCHEMA_VERSION, BehaviorIssue, BehaviorProposal, CapabilityInvocation,
    CapabilityParameterType, LoadedGamePack,
};
use ats_kernel::{FailureCode, SchemaId, SchemaRef, SchemaVersion, Sha256Digest};
use ats_runtime::{
    CancellationToken, FinishReason, ModelClient, ModelError, ModelGamePackRef,
    ModelOutputContract, ModelRequestError, ModelRequestSnapshot, RunFailure, TokenUsage,
};
use ats_workspace::StoredItemDefinition;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use super::CompositionGenerateFeature;
use crate::FeatureSpec;
use crate::mod_plan::PlanItem;
use crate::prompt::{FeatureRecipe, FeatureRecipeError, FeatureRecipeLoader};

const RECIPE_BYTES: &[u8] = include_bytes!("../../recipes/composition-behavior.json");
const RECIPE_SHA256: &str = "557c3dbb1f1d09f65e3ca9b8d774a710d37751310e8fcfd71de9c89af4ba4232";

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BehaviorModelOutput {
    invocations: Vec<CapabilityInvocation>,
}

#[derive(Debug, Clone)]
pub(super) struct BehaviorGeneration {
    pub proposal: BehaviorProposal,
    pub request_snapshot: ModelRequestSnapshot,
    pub response_model: String,
    pub usage: TokenUsage,
}

pub(super) struct BehaviorGenerationContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub truth_snapshot_id: &'a Sha256Digest,
    pub definition: &'a StoredItemDefinition,
    pub plan: &'a PlanItem,
    pub project_context: &'a str,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
    pub model_request_limits: ats_runtime::ModelRequestLimits,
    pub feedback: Option<&'a BehaviorFeedback>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BehaviorFeedback {
    pub reason: BehaviorFeedbackReason,
    pub issues: Vec<BehaviorIssue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(super) enum BehaviorFeedbackReason {
    OutputTruncated,
    JsonDecode,
    IrInvalid,
}

#[derive(Debug, Clone)]
pub(super) struct BehaviorFeedbackEvidence {
    pub feedback: BehaviorFeedback,
    pub candidate_sha256: Sha256Digest,
}

pub(super) struct BehaviorGenerationService {
    recipe: FeatureRecipe,
}

impl BehaviorGenerationService {
    pub fn built_in() -> Result<Self, BehaviorGenerationError> {
        let expected = Sha256Digest::parse(RECIPE_SHA256)
            .map_err(|_| BehaviorGenerationError::InvalidRecipeContract)?;
        let recipe = FeatureRecipeLoader::load(RECIPE_BYTES, &expected)?;
        if recipe.feature_id() != &CompositionGenerateFeature::id()
            || recipe.output_contract().schema != behavior_model_output_schema()
        {
            return Err(BehaviorGenerationError::InvalidRecipeContract);
        }
        Ok(Self { recipe })
    }

    pub fn prepare(
        &self,
        context: &BehaviorGenerationContext<'_>,
    ) -> Result<ModelRequestSnapshot, BehaviorGenerationError> {
        validate_context(context)?;
        let output_contract =
            output_contract(context.pack, &context.definition.definition.item_type)?;
        let capability_set = context
            .pack
            .capability_catalog()
            .item_capabilities(&context.definition.definition.item_type)
            .ok_or(BehaviorGenerationError::UnsupportedItemType)?;
        let capabilities = capability_set
            .allowed_capabilities
            .iter()
            .map(|id| {
                context
                    .pack
                    .capability_catalog()
                    .capability(id)
                    .ok_or(BehaviorGenerationError::InvalidCatalog)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize(&output_contract.json_schema)?,
            ),
            (
                "capability.catalog".into(),
                serialize(&serde_json::json!({
                    "identity": context.pack.capability_catalog_identity(),
                    "itemType": context.definition.definition.item_type,
                    "minInvocations": capability_set.min_invocations,
                    "maxInvocations": capability_set.max_invocations,
                    "capabilities": capabilities,
                }))?,
            ),
            ("item.definition".into(), serialize(context.definition)?),
            ("request.plan".into(), serialize(context.plan)?),
            (
                "project.context".into(),
                bounded(context.project_context, 12_000)?,
            ),
            (
                "runtime.custom_instructions".into(),
                bounded(context.custom_instructions.unwrap_or(""), 4_000)?,
            ),
            (
                "behavior.feedback".into(),
                context
                    .feedback
                    .map_or_else(|| Ok(String::new()), serialize)?,
            ),
        ]);
        let request = self.recipe.render_with_output_contract(
            &slots,
            context.model.clone(),
            output_contract,
            &context.model_request_limits,
        )?;
        Ok(ModelRequestSnapshot::new(
            CompositionGenerateFeature::id(),
            self.recipe.recipe_ref(),
            ModelGamePackRef {
                id: context.pack.id().clone(),
                sha256: context.pack.content_sha256().clone(),
            },
            Some(context.truth_snapshot_id.clone()),
            Vec::new(),
            request,
        )?)
    }

    pub async fn generate<C: ModelClient + ?Sized>(
        &self,
        client: &C,
        context: BehaviorGenerationContext<'_>,
        request_snapshot: ModelRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<BehaviorGeneration, BehaviorGenerationError> {
        if cancellation.is_cancelled() {
            return Err(BehaviorGenerationError::Cancelled);
        }
        let expected = self.prepare(&context)?;
        if expected != request_snapshot {
            return Err(BehaviorGenerationError::InvalidContext);
        }
        let snapshot = request_snapshot;
        let response = client.complete(snapshot.clone(), cancellation).await?;
        if cancellation.is_cancelled() {
            return Err(BehaviorGenerationError::Cancelled);
        }
        if response.finish_reason == FinishReason::MaxTokens {
            return Err(BehaviorGenerationError::TruncatedModelOutput(
                candidate_sha256(&response.content)?,
            ));
        }
        let response_sha256 = candidate_sha256(&response.content)?;
        let output: BehaviorModelOutput = serde_json::from_str(&response.content)
            .map_err(|_| BehaviorGenerationError::InvalidModelOutput(response_sha256.clone()))?;
        let proposal = BehaviorProposal {
            schema_version: BEHAVIOR_PROPOSAL_SCHEMA_VERSION,
            item_id: context.definition.definition.item_id.clone(),
            item_type: context.definition.definition.item_type.clone(),
            definition_hash: context.definition.definition_hash.clone(),
            catalog: context.pack.capability_catalog_identity().clone(),
            adapter: context.pack.behavior_adapter().clone(),
            invocations: output.invocations,
        };
        proposal
            .validate(context.pack.capability_catalog())
            .map_err(|issues| BehaviorGenerationError::InvalidBehavior {
                issues,
                candidate_sha256: response_sha256,
            })?;
        Ok(BehaviorGeneration {
            proposal,
            request_snapshot: snapshot,
            response_model: response.model,
            usage: response.usage,
        })
    }
}

#[derive(Debug, Error)]
pub enum BehaviorGenerationError {
    #[error("behavior generation context is invalid")]
    InvalidContext,
    #[error("behavior capability catalog is invalid")]
    InvalidCatalog,
    #[error("behavior generation item type is unsupported")]
    UnsupportedItemType,
    #[error("behavior generation Recipe does not match its contract")]
    InvalidRecipeContract,
    #[error("behavior model output was truncated")]
    TruncatedModelOutput(Sha256Digest),
    #[error("behavior model output failed strict decoding")]
    InvalidModelOutput(Sha256Digest),
    #[error("behavior proposal failed local validation")]
    InvalidBehavior {
        issues: Vec<BehaviorIssue>,
        candidate_sha256: Sha256Digest,
    },
    #[error("behavior generation was cancelled")]
    Cancelled,
    #[error(transparent)]
    Recipe(#[from] FeatureRecipeError),
    #[error(transparent)]
    Request(#[from] ModelRequestError),
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl BehaviorGenerationError {
    pub fn run_failure(&self) -> RunFailure {
        let (code, stage) = match self {
            Self::InvalidContext => ("truth.context_mismatch", "composition.behavior.context"),
            Self::InvalidCatalog => ("pack.behavior_invalid", "composition.behavior.catalog"),
            Self::UnsupportedItemType => (
                "feature.item_type_unsupported",
                "composition.behavior.catalog",
            ),
            Self::InvalidRecipeContract | Self::Recipe(_) => {
                ("feature.recipe_invalid", "composition.behavior.recipe")
            }
            Self::TruncatedModelOutput(_) => {
                ("model.output_truncated", "composition.behavior.model")
            }
            Self::InvalidModelOutput(_) => ("model.output_invalid", "composition.behavior.model"),
            Self::InvalidBehavior { .. } => {
                ("behavior.ir_invalid", "composition.behavior.validate")
            }
            Self::Cancelled | Self::Model(ModelError::Cancelled) => {
                ("run.cancelled", "composition.behavior.execute")
            }
            Self::Request(_) => ("model.request_invalid", "composition.behavior.model"),
            Self::Model(ModelError::Authentication) => {
                ("model.authentication", "composition.behavior.model")
            }
            Self::Model(ModelError::RateLimited { .. }) => {
                ("model.rate_limited", "composition.behavior.model")
            }
            Self::Model(ModelError::Configuration) => {
                ("model.configuration", "composition.behavior.model")
            }
            Self::Model(ModelError::Transport) => {
                ("model.transport_failed", "composition.behavior.model")
            }
            Self::Model(ModelError::Rejected) => {
                ("model.request_rejected", "composition.behavior.model")
            }
            Self::Model(ModelError::InvalidResponse) => {
                ("model.response_invalid", "composition.behavior.model")
            }
        };
        RunFailure::new(
            FailureCode::parse(code).expect("built-in failure code is valid"),
            stage,
            None,
        )
        .expect("built-in Run failure is valid")
    }

    pub(super) fn feedback_evidence(&self) -> Option<BehaviorFeedbackEvidence> {
        match self {
            Self::TruncatedModelOutput(candidate_sha256) => Some(BehaviorFeedbackEvidence {
                feedback: BehaviorFeedback {
                    reason: BehaviorFeedbackReason::OutputTruncated,
                    issues: Vec::new(),
                },
                candidate_sha256: candidate_sha256.clone(),
            }),
            Self::InvalidModelOutput(candidate_sha256) => Some(BehaviorFeedbackEvidence {
                feedback: BehaviorFeedback {
                    reason: BehaviorFeedbackReason::JsonDecode,
                    issues: Vec::new(),
                },
                candidate_sha256: candidate_sha256.clone(),
            }),
            Self::InvalidBehavior {
                issues,
                candidate_sha256,
            } => Some(BehaviorFeedbackEvidence {
                feedback: BehaviorFeedback {
                    reason: BehaviorFeedbackReason::IrInvalid,
                    issues: issues.clone(),
                },
                candidate_sha256: candidate_sha256.clone(),
            }),
            _ => None,
        }
    }
}

fn validate_context(
    context: &BehaviorGenerationContext<'_>,
) -> Result<(), BehaviorGenerationError> {
    context
        .definition
        .validate()
        .map_err(|_| BehaviorGenerationError::InvalidContext)?;
    if context.definition.definition.item_id.as_str() != context.plan.item_id
        || context.definition.definition.item_type.as_str() != context.plan.item_type
        || context.definition.definition.behavior_intent != context.plan.behavior_intent
        || context.pack.capability_catalog_identity()
            != &context
                .pack
                .capability_catalog()
                .identity()
                .map_err(|_| BehaviorGenerationError::InvalidCatalog)?
        || context.pack.behavior_adapter() != &context.pack.capability_catalog().adapter
    {
        return Err(BehaviorGenerationError::InvalidContext);
    }
    Ok(())
}

fn output_contract(
    pack: &LoadedGamePack,
    item_type: &ats_kernel::ItemTypeId,
) -> Result<ModelOutputContract, BehaviorGenerationError> {
    let catalog = pack.capability_catalog();
    let set = catalog
        .item_capabilities(item_type)
        .ok_or(BehaviorGenerationError::UnsupportedItemType)?;
    let variants = set
        .allowed_capabilities
        .iter()
        .map(|id| {
            let capability = catalog
                .capability(id)
                .ok_or(BehaviorGenerationError::InvalidCatalog)?;
            let properties = capability
                .parameters
                .iter()
                .map(|parameter| (parameter.id.to_string(), parameter_schema(&parameter.value)))
                .collect::<serde_json::Map<_, _>>();
            let required = capability
                .parameters
                .iter()
                .filter(|parameter| parameter.required)
                .map(|parameter| parameter.id.to_string())
                .collect::<Vec<_>>();
            Ok::<serde_json::Value, BehaviorGenerationError>(serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["capabilityId", "arguments"],
                "properties": {
                    "capabilityId": { "const": capability.id },
                    "arguments": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": required,
                        "properties": properties,
                    }
                }
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let items = if variants.is_empty() {
        serde_json::json!({"type": "object"})
    } else {
        serde_json::json!({"oneOf": variants})
    };
    Ok(ModelOutputContract {
        schema: behavior_model_output_schema(),
        json_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["invocations"],
            "properties": {
                "invocations": {
                    "type": "array",
                    "minItems": set.min_invocations,
                    "maxItems": set.max_invocations,
                    "items": items,
                }
            }
        }),
    })
}

fn parameter_schema(value: &CapabilityParameterType) -> serde_json::Value {
    match value {
        CapabilityParameterType::Text {
            min_length,
            max_length,
            ..
        } => serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["kind", "value"],
            "properties": {
                "kind": {"const": "text"},
                "value": {"type": "string", "minLength": min_length, "maxLength": max_length}
            }
        }),
        CapabilityParameterType::Integer { min, max } => tagged_value_schema(
            "integer",
            serde_json::json!({"type": "integer", "minimum": min, "maximum": max}),
        ),
        CapabilityParameterType::Boolean => {
            tagged_value_schema("boolean", serde_json::json!({"type": "boolean"}))
        }
        CapabilityParameterType::Choice { options } => tagged_value_schema(
            "choice",
            serde_json::json!({"type": "string", "enum": options}),
        ),
        CapabilityParameterType::ItemReference { .. } => tagged_value_schema(
            "item_reference",
            serde_json::json!({"type": "string", "minLength": 1, "maxLength": 128}),
        ),
        CapabilityParameterType::ResourceReference => tagged_value_schema(
            "resource_reference",
            serde_json::json!({"type": "string", "minLength": 1, "maxLength": 128}),
        ),
        CapabilityParameterType::TextList {
            min_items,
            max_items,
            item_max_length,
        } => tagged_value_schema(
            "text_list",
            serde_json::json!({
                "type": "array",
                "minItems": min_items,
                "maxItems": max_items,
                "items": {"type": "string", "minLength": 1, "maxLength": item_max_length}
            }),
        ),
    }
}

fn tagged_value_schema(kind: &str, value: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "value"],
        "properties": {"kind": {"const": kind}, "value": value}
    })
}

fn behavior_model_output_schema() -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse("feature.composition-behavior-model-output")
            .expect("built-in schema ID is valid"),
        version: SchemaVersion::new(1).expect("built-in schema version is valid"),
    }
}

fn serialize<T: Serialize + ?Sized>(value: &T) -> Result<String, BehaviorGenerationError> {
    serde_json::to_string_pretty(value).map_err(|_| BehaviorGenerationError::InvalidContext)
}

fn bounded(value: &str, max: usize) -> Result<String, BehaviorGenerationError> {
    if value.chars().count() > max || value.contains('\0') {
        return Err(BehaviorGenerationError::InvalidContext);
    }
    Ok(value.to_owned())
}

fn candidate_sha256(value: &str) -> Result<Sha256Digest, BehaviorGenerationError> {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(value.as_bytes())))
        .map_err(|_| BehaviorGenerationError::InvalidContext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ats_game_context::GamePackLoader;
    use ats_kernel::ItemTypeId;

    #[test]
    fn built_in_recipe_and_card_contract_are_strict_and_provider_neutral() {
        let service = BehaviorGenerationService::built_in().unwrap();
        assert_eq!(
            service.recipe.output_contract().schema,
            behavior_model_output_schema()
        );
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let contract = output_contract(&pack, &ItemTypeId::parse("card").unwrap()).unwrap();
        let root = contract.json_schema["properties"].as_object().unwrap();
        assert_eq!(contract.json_schema["additionalProperties"], false);
        assert_eq!(root["invocations"]["minItems"], 1);
        assert_eq!(root["invocations"]["maxItems"], 4);
        let variants = root["invocations"]["items"]["oneOf"].as_array().unwrap();
        assert_eq!(variants.len(), 4);
        assert!(variants.iter().all(|variant| {
            variant["additionalProperties"] == false
                && variant["properties"]["arguments"]["additionalProperties"] == false
                && variant["properties"]["arguments"]["properties"]["amount"]["properties"]["kind"]
                    ["const"]
                    == "integer"
        }));
        for forbidden in [
            "source",
            "relativePath",
            "namespace",
            "className",
            "localizations",
            "resourcePath",
        ] {
            assert!(!root.contains_key(forbidden));
        }
    }
}
