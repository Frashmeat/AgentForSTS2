use std::collections::BTreeSet;
use std::pin::Pin;

use async_trait::async_trait;
use ats_kernel::{
    FeatureId, GamePackId, RecipeId, ResourceId, SchemaRef, SchemaVersion, Sha256Digest,
};
use futures_util::Stream;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MODEL_REQUEST_SNAPSHOT_SCHEMA_VERSION: u32 = 2;
pub const MAX_MODEL_OUTPUT_TOKENS: u32 = 65_536;

#[derive(Debug, Clone, Copy, Default, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRequestLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModelRequestLimitsWire {
    #[serde(default)]
    max_output_tokens: Option<u32>,
}

impl<'de> Deserialize<'de> for ModelRequestLimits {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ModelRequestLimitsWire::deserialize(deserializer)?;
        Self::new(wire.max_output_tokens).map_err(serde::de::Error::custom)
    }
}

impl ModelRequestLimits {
    pub fn new(max_output_tokens: Option<u32>) -> Result<Self, ModelRequestError> {
        let limits = Self { max_output_tokens };
        limits.validate()?;
        Ok(limits)
    }

    pub fn resolve_max_output_tokens(
        &self,
        recipe_max_output_tokens: u32,
    ) -> Result<u32, ModelRequestError> {
        self.validate()?;
        if recipe_max_output_tokens == 0 || recipe_max_output_tokens > MAX_MODEL_OUTPUT_TOKENS {
            return Err(ModelRequestError::InvalidRequest);
        }
        Ok(self
            .max_output_tokens
            .map_or(recipe_max_output_tokens, |configured| {
                recipe_max_output_tokens.min(configured)
            }))
    }

    fn validate(&self) -> Result<(), ModelRequestError> {
        if self
            .max_output_tokens
            .is_some_and(|value| value == 0 || value > MAX_MODEL_OUTPUT_TOKENS)
        {
            return Err(ModelRequestError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ModelMessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMessage {
    pub role: ModelMessageRole,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelRequest {
    pub messages: Vec<ModelMessage>,
    pub output_contract: ModelOutputContract,
    pub max_output_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl ModelRequest {
    fn validate(&self) -> Result<(), ModelRequestError> {
        if self.messages.is_empty()
            || self.messages.len() > 32
            || self.messages.iter().any(|message| {
                message.content.is_empty() || message.content.chars().count() > 200_000
            })
            || self.max_output_tokens == 0
            || self.max_output_tokens > MAX_MODEL_OUTPUT_TOKENS
            || self
                .temperature
                .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
            || self.model.as_ref().is_some_and(|value| {
                value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control)
            })
        {
            return Err(ModelRequestError::InvalidRequest);
        }
        self.output_contract.validate()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelOutputContract {
    pub schema: SchemaRef,
    pub json_schema: serde_json::Value,
}

impl ModelOutputContract {
    fn validate(&self) -> Result<(), ModelRequestError> {
        let Some(object) = self.json_schema.as_object() else {
            return Err(ModelRequestError::InvalidOutputContract);
        };
        if object.is_empty()
            || serde_json::to_vec(&self.json_schema)
                .map_err(|_| ModelRequestError::InvalidOutputContract)?
                .len()
                > 32_000
        {
            return Err(ModelRequestError::InvalidOutputContract);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeRef {
    pub id: RecipeId,
    pub version: SchemaVersion,
    pub sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelGamePackRef {
    pub id: GamePackId,
    pub sha256: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelResourceRef {
    pub resource_id: ResourceId,
    pub logical_role: String,
    pub selected_version: Sha256Digest,
    pub media_type: String,
}

impl ModelResourceRef {
    fn validate(&self) -> Result<(), ModelRequestError> {
        if !valid_label(&self.logical_role, 128) || !valid_media_type(&self.media_type) {
            return Err(ModelRequestError::InvalidResource);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelContextBinding {
    pub role: String,
    pub schema: SchemaRef,
    pub sha256: Sha256Digest,
}

impl ModelContextBinding {
    fn validate(&self) -> Result<(), ModelRequestError> {
        if !valid_label(&self.role, 128) {
            return Err(ModelRequestError::InvalidContextBinding);
        }
        Ok(())
    }
}

pub type ModelRequestContextBinding = ModelContextBinding;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelRequestCommitmentIdentity {
    pub schema_version: u32,
    pub feature_id: FeatureId,
    pub recipe: RecipeRef,
    pub game_pack: ModelGamePackRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truth_snapshot_id: Option<Sha256Digest>,
    pub selected_resources: Vec<ModelResourceRef>,
    pub context_bindings: Vec<ModelContextBinding>,
    pub rendered_messages_sha256: Sha256Digest,
    pub output_contract_schema: SchemaRef,
    pub output_contract_sha256: Sha256Digest,
    pub max_output_tokens: u32,
    pub temperature_bits: Option<u32>,
    pub requested_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelRequestCommitment {
    pub identity: ModelRequestCommitmentIdentity,
    pub request_sha256: Sha256Digest,
}

impl ModelRequestCommitment {
    pub fn verify(&self) -> Result<(), ModelRequestError> {
        validate_identity(&self.identity)?;
        if compute_commitment_hash(&self.identity)? != self.request_sha256 {
            return Err(ModelRequestError::IdentityMismatch);
        }
        Ok(())
    }

    #[must_use]
    pub fn request_sha256(&self) -> &Sha256Digest {
        &self.request_sha256
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelRequestSnapshot {
    schema_version: u32,
    feature_id: FeatureId,
    recipe: RecipeRef,
    game_pack: ModelGamePackRef,
    truth_snapshot_id: Option<Sha256Digest>,
    selected_resources: Vec<ModelResourceRef>,
    context_bindings: Vec<ModelContextBinding>,
    request: ModelRequest,
    commitment: ModelRequestCommitment,
    request_sha256: Sha256Digest,
}

impl ModelRequestSnapshot {
    pub fn new(
        feature_id: FeatureId,
        recipe: RecipeRef,
        game_pack: ModelGamePackRef,
        truth_snapshot_id: Option<Sha256Digest>,
        selected_resources: Vec<ModelResourceRef>,
        request: ModelRequest,
    ) -> Result<Self, ModelRequestError> {
        Self::new_with_bindings(
            feature_id,
            recipe,
            game_pack,
            truth_snapshot_id,
            selected_resources,
            Vec::new(),
            request,
        )
    }

    pub fn new_with_bindings(
        feature_id: FeatureId,
        recipe: RecipeRef,
        game_pack: ModelGamePackRef,
        truth_snapshot_id: Option<Sha256Digest>,
        mut selected_resources: Vec<ModelResourceRef>,
        mut context_bindings: Vec<ModelContextBinding>,
        request: ModelRequest,
    ) -> Result<Self, ModelRequestError> {
        selected_resources.sort();
        context_bindings.sort_by(|left, right| {
            left.role
                .cmp(&right.role)
                .then_with(|| left.schema.id.cmp(&right.schema.id))
                .then_with(|| left.schema.version.cmp(&right.schema.version))
                .then_with(|| left.sha256.cmp(&right.sha256))
        });
        let commitment = commitment_for(
            MODEL_REQUEST_SNAPSHOT_SCHEMA_VERSION,
            &feature_id,
            &recipe,
            &game_pack,
            &truth_snapshot_id,
            &selected_resources,
            &context_bindings,
            &request,
        )?;
        let request_sha256 = commitment.request_sha256.clone();
        let snapshot = Self {
            schema_version: MODEL_REQUEST_SNAPSHOT_SCHEMA_VERSION,
            feature_id,
            recipe,
            game_pack,
            truth_snapshot_id,
            selected_resources,
            context_bindings,
            request,
            commitment,
            request_sha256,
        };
        snapshot.validate_structure()?;
        Ok(snapshot)
    }

    pub fn verify(&self) -> Result<(), ModelRequestError> {
        self.validate_structure()?;
        let commitment = self.compute_commitment()?;
        if commitment != self.commitment || commitment.request_sha256 != self.request_sha256 {
            return Err(ModelRequestError::IdentityMismatch);
        }
        Ok(())
    }

    fn validate_structure(&self) -> Result<(), ModelRequestError> {
        if self.schema_version != MODEL_REQUEST_SNAPSHOT_SCHEMA_VERSION
            || self.selected_resources.len() > 64
            || self.context_bindings.len() > 64
        {
            return Err(ModelRequestError::InvalidSnapshot);
        }
        self.request.validate()?;
        let mut ids = BTreeSet::new();
        for resource in &self.selected_resources {
            resource.validate()?;
            if !ids.insert(&resource.resource_id) {
                return Err(ModelRequestError::DuplicateResource);
            }
        }
        if !self
            .selected_resources
            .windows(2)
            .all(|items| items[0] < items[1])
        {
            return Err(ModelRequestError::InvalidSnapshot);
        }
        let mut binding_keys = BTreeSet::new();
        for binding in &self.context_bindings {
            binding.validate()?;
            let key = (&binding.role, &binding.schema.id, binding.schema.version);
            if !binding_keys.insert(key) {
                return Err(ModelRequestError::DuplicateContextBinding);
            }
        }
        if !self.context_bindings.windows(2).all(|items| {
            let left = &items[0];
            let right = &items[1];
            (
                left.role.as_str(),
                &left.schema.id,
                left.schema.version,
                &left.sha256,
            ) <= (
                right.role.as_str(),
                &right.schema.id,
                right.schema.version,
                &right.sha256,
            )
        }) {
            return Err(ModelRequestError::InvalidSnapshot);
        }
        Ok(())
    }

    fn compute_commitment(&self) -> Result<ModelRequestCommitment, ModelRequestError> {
        commitment_for(
            self.schema_version,
            &self.feature_id,
            &self.recipe,
            &self.game_pack,
            &self.truth_snapshot_id,
            &self.selected_resources,
            &self.context_bindings,
            &self.request,
        )
    }

    #[must_use]
    pub fn feature_id(&self) -> &FeatureId {
        &self.feature_id
    }

    #[must_use]
    pub fn recipe(&self) -> &RecipeRef {
        &self.recipe
    }

    #[must_use]
    pub fn game_pack(&self) -> &ModelGamePackRef {
        &self.game_pack
    }

    #[must_use]
    pub fn truth_snapshot_id(&self) -> Option<&Sha256Digest> {
        self.truth_snapshot_id.as_ref()
    }

    #[must_use]
    pub fn selected_resources(&self) -> &[ModelResourceRef] {
        &self.selected_resources
    }

    #[must_use]
    pub fn context_bindings(&self) -> &[ModelContextBinding] {
        &self.context_bindings
    }

    #[must_use]
    pub fn request(&self) -> &ModelRequest {
        &self.request
    }

    #[must_use]
    pub fn request_sha256(&self) -> &Sha256Digest {
        &self.request_sha256
    }

    #[must_use]
    pub fn commitment(&self) -> &ModelRequestCommitment {
        &self.commitment
    }
}

#[allow(clippy::too_many_arguments)]
fn commitment_for(
    schema_version: u32,
    feature_id: &FeatureId,
    recipe: &RecipeRef,
    game_pack: &ModelGamePackRef,
    truth_snapshot_id: &Option<Sha256Digest>,
    selected_resources: &[ModelResourceRef],
    context_bindings: &[ModelContextBinding],
    request: &ModelRequest,
) -> Result<ModelRequestCommitment, ModelRequestError> {
    let identity = ModelRequestCommitmentIdentity {
        schema_version,
        feature_id: feature_id.clone(),
        recipe: recipe.clone(),
        game_pack: game_pack.clone(),
        truth_snapshot_id: truth_snapshot_id.clone(),
        selected_resources: selected_resources.to_vec(),
        context_bindings: context_bindings.to_vec(),
        rendered_messages_sha256: digest_json(&request.messages)?,
        output_contract_schema: request.output_contract.schema.clone(),
        output_contract_sha256: digest_json(&request.output_contract.json_schema)?,
        max_output_tokens: request.max_output_tokens,
        temperature_bits: request.temperature.map(f32::to_bits),
        requested_model: request.model.clone(),
    };
    validate_identity(&identity)?;
    Ok(ModelRequestCommitment {
        request_sha256: compute_commitment_hash(&identity)?,
        identity,
    })
}

fn validate_identity(identity: &ModelRequestCommitmentIdentity) -> Result<(), ModelRequestError> {
    if identity.schema_version != MODEL_REQUEST_SNAPSHOT_SCHEMA_VERSION
        || identity.selected_resources.len() > 64
        || identity.context_bindings.len() > 64
        || identity.max_output_tokens == 0
        || identity.max_output_tokens > MAX_MODEL_OUTPUT_TOKENS
        || identity.temperature_bits.is_some_and(|bits| {
            let value = f32::from_bits(bits);
            !value.is_finite() || !(0.0..=2.0).contains(&value)
        })
        || identity.requested_model.as_ref().is_some_and(|value| {
            value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        })
    {
        return Err(ModelRequestError::InvalidSnapshot);
    }

    let mut resource_ids = BTreeSet::new();
    for resource in &identity.selected_resources {
        resource.validate()?;
        if !resource_ids.insert(&resource.resource_id) {
            return Err(ModelRequestError::DuplicateResource);
        }
    }
    if !identity
        .selected_resources
        .windows(2)
        .all(|items| items[0] < items[1])
    {
        return Err(ModelRequestError::InvalidSnapshot);
    }

    let mut binding_keys = BTreeSet::new();
    for binding in &identity.context_bindings {
        binding.validate()?;
        let key = (&binding.role, &binding.schema.id, binding.schema.version);
        if !binding_keys.insert(key) {
            return Err(ModelRequestError::DuplicateContextBinding);
        }
    }
    if !identity.context_bindings.windows(2).all(|items| {
        let left = &items[0];
        let right = &items[1];
        (
            left.role.as_str(),
            &left.schema.id,
            left.schema.version,
            &left.sha256,
        ) <= (
            right.role.as_str(),
            &right.schema.id,
            right.schema.version,
            &right.sha256,
        )
    }) {
        return Err(ModelRequestError::InvalidSnapshot);
    }
    Ok(())
}

fn compute_commitment_hash(
    identity: &ModelRequestCommitmentIdentity,
) -> Result<Sha256Digest, ModelRequestError> {
    let bytes = serde_json::to_vec(identity).map_err(|_| ModelRequestError::InvalidSnapshot)?;
    Ok(sha256_bytes(&bytes))
}

fn digest_json<T: Serialize>(value: &T) -> Result<Sha256Digest, ModelRequestError> {
    let bytes = serde_json::to_vec(value).map_err(|_| ModelRequestError::InvalidSnapshot)?;
    Ok(sha256_bytes(&bytes))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    EndTurn,
    MaxTokens,
    StopSequence,
    ToolUse,
    #[default]
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelResponse {
    pub model: String,
    pub content: String,
    pub finish_reason: FinishReason,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelStreamEvent {
    Start {
        model: String,
    },
    Delta {
        text: String,
    },
    End {
        finish_reason: FinishReason,
        usage: TokenUsage,
    },
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum ModelError {
    #[error("model authentication failed")]
    Authentication,
    #[error("model request was rate limited")]
    RateLimited { retry_after_ms: Option<u64> },
    #[error("model transport failed")]
    Transport,
    #[error("model provider rejected the request")]
    Rejected,
    #[error("model response was invalid")]
    InvalidResponse,
    #[error("model request was cancelled")]
    Cancelled,
    #[error("model client configuration is invalid")]
    Configuration,
}

impl ModelError {
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited { .. } | Self::Transport)
    }
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum ModelRequestError {
    #[error("model request is invalid")]
    InvalidRequest,
    #[error("model output contract is invalid")]
    InvalidOutputContract,
    #[error("model request snapshot is invalid")]
    InvalidSnapshot,
    #[error("model request contains an invalid resource reference")]
    InvalidResource,
    #[error("model request contains a duplicate resource reference")]
    DuplicateResource,
    #[error("model request contains an invalid context binding")]
    InvalidContextBinding,
    #[error("model request contains a duplicate context binding")]
    DuplicateContextBinding,
    #[error("model request snapshot identity does not match its content")]
    IdentityMismatch,
}

pub type ModelStream =
    Pin<Box<dyn Stream<Item = Result<ModelStreamEvent, ModelError>> + Send + 'static>>;

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn complete(
        &self,
        request: ModelRequestSnapshot,
        cancellation: &crate::CancellationToken,
    ) -> Result<ModelResponse, ModelError>;

    async fn stream(
        &self,
        request: ModelRequestSnapshot,
        cancellation: &crate::CancellationToken,
    ) -> Result<ModelStream, ModelError>;
}

fn valid_label(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && !value.chars().any(char::is_control)
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn valid_media_type(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.chars().any(char::is_control)
        && value.split_once('/').is_some_and(|(kind, subtype)| {
            !kind.is_empty()
                && !subtype.is_empty()
                && kind
                    .bytes()
                    .chain(subtype.bytes())
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
        })
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter always returns a valid digest")
}

#[cfg(test)]
mod tests {
    use ats_kernel::SchemaId;

    use super::*;

    fn digest(value: char) -> Sha256Digest {
        Sha256Digest::parse(value.to_string().repeat(64)).unwrap()
    }

    fn request(resources: Vec<ModelResourceRef>) -> ModelRequestSnapshot {
        request_with_budget(resources, 512)
    }

    fn request_with_budget(
        resources: Vec<ModelResourceRef>,
        max_output_tokens: u32,
    ) -> ModelRequestSnapshot {
        ModelRequestSnapshot::new(
            FeatureId::parse("fixture.analyze").unwrap(),
            RecipeRef {
                id: RecipeId::parse("recipe.fixture-analyze").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
                sha256: digest('a'),
            },
            ModelGamePackRef {
                id: GamePackId::parse("fixture-game").unwrap(),
                sha256: digest('b'),
            },
            Some(digest('c')),
            resources,
            ModelRequest {
                messages: vec![ModelMessage {
                    role: ModelMessageRole::User,
                    content: "bounded input".into(),
                }],
                output_contract: ModelOutputContract {
                    schema: SchemaRef {
                        id: SchemaId::parse("feature.fixture-result").unwrap(),
                        version: SchemaVersion::new(1).unwrap(),
                    },
                    json_schema: serde_json::json!({"type":"object"}),
                },
                max_output_tokens,
                temperature: Some(0.0),
                model: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn snapshot_identity_is_canonical_and_commitment_is_verified_on_decode() {
        let first = ModelResourceRef {
            resource_id: ResourceId::parse("resource.zeta").unwrap(),
            logical_role: "log.attachment".into(),
            selected_version: digest('d'),
            media_type: "text/plain".into(),
        };
        let second = ModelResourceRef {
            resource_id: ResourceId::parse("resource.alpha").unwrap(),
            logical_role: "log.context".into(),
            selected_version: digest('e'),
            media_type: "application/json".into(),
        };
        let left = request(vec![first.clone(), second.clone()]);
        let right = request(vec![second, first]);
        assert_eq!(left.request_sha256(), right.request_sha256());
        assert_eq!(left.selected_resources(), right.selected_resources());

        let json = serde_json::to_string(left.commitment()).unwrap();
        let decoded: ModelRequestCommitment = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, *left.commitment());
        decoded.verify().unwrap();

        let mut tampered = left.clone();
        tampered.request.messages[0].content = "tampered".into();
        assert_eq!(tampered.verify(), Err(ModelRequestError::IdentityMismatch));

        let mut tampered_commitment = left.commitment().clone();
        tampered_commitment.identity.rendered_messages_sha256 = digest('f');
        assert_eq!(
            tampered_commitment.verify(),
            Err(ModelRequestError::IdentityMismatch)
        );
    }

    #[test]
    fn invalid_requests_and_duplicate_resources_are_rejected() {
        let duplicate = ModelResourceRef {
            resource_id: ResourceId::parse("resource.same").unwrap(),
            logical_role: "log.context".into(),
            selected_version: digest('d'),
            media_type: "text/plain".into(),
        };
        assert_eq!(
            ModelRequestSnapshot::new(
                FeatureId::parse("fixture.analyze").unwrap(),
                RecipeRef {
                    id: RecipeId::parse("recipe.fixture-analyze").unwrap(),
                    version: SchemaVersion::new(1).unwrap(),
                    sha256: digest('a'),
                },
                ModelGamePackRef {
                    id: GamePackId::parse("fixture-game").unwrap(),
                    sha256: digest('b'),
                },
                None,
                vec![duplicate.clone(), duplicate],
                request(Vec::new()).request,
            ),
            Err(ModelRequestError::DuplicateResource)
        );
    }

    #[test]
    fn model_request_limits_only_reduce_recipe_budgets() {
        let configured = ModelRequestLimits::new(Some(4_096)).unwrap();
        assert_eq!(configured.resolve_max_output_tokens(16_384), Ok(4_096));
        assert_eq!(configured.resolve_max_output_tokens(3_072), Ok(3_072));
        assert_eq!(
            ModelRequestLimits::default().resolve_max_output_tokens(16_384),
            Ok(16_384)
        );
    }

    #[test]
    fn request_snapshot_hash_binds_the_effective_output_budget() {
        let lower = request_with_budget(Vec::new(), 4_096);
        let higher = request_with_budget(Vec::new(), 8_192);

        assert_eq!(lower.request().max_output_tokens, 4_096);
        assert_eq!(higher.request().max_output_tokens, 8_192);
        assert_ne!(lower.request_sha256(), higher.request_sha256());
    }

    #[test]
    fn context_bindings_are_sorted_and_duplicate_keys_are_rejected() {
        let schema = SchemaRef {
            id: SchemaId::parse("feature.fixture-context").unwrap(),
            version: SchemaVersion::new(1).unwrap(),
        };
        let first = ModelContextBinding {
            role: "human.semantic_feedback".into(),
            schema: schema.clone(),
            sha256: digest('d'),
        };
        let second = ModelContextBinding {
            role: "runtime.custom_instructions".into(),
            schema,
            sha256: digest('e'),
        };
        let snapshot = ModelRequestSnapshot::new_with_bindings(
            FeatureId::parse("fixture.analyze").unwrap(),
            RecipeRef {
                id: RecipeId::parse("recipe.fixture-analyze").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
                sha256: digest('a'),
            },
            ModelGamePackRef {
                id: GamePackId::parse("fixture-game").unwrap(),
                sha256: digest('b'),
            },
            None,
            Vec::new(),
            vec![first.clone(), second.clone()],
            request(Vec::new()).request,
        )
        .unwrap();
        assert_eq!(snapshot.context_bindings()[0], first);
        assert_eq!(snapshot.context_bindings()[1], second);

        assert_eq!(
            ModelRequestSnapshot::new_with_bindings(
                FeatureId::parse("fixture.analyze").unwrap(),
                RecipeRef {
                    id: RecipeId::parse("recipe.fixture-analyze").unwrap(),
                    version: SchemaVersion::new(1).unwrap(),
                    sha256: digest('a'),
                },
                ModelGamePackRef {
                    id: GamePackId::parse("fixture-game").unwrap(),
                    sha256: digest('b'),
                },
                None,
                Vec::new(),
                vec![first.clone(), first],
                request(Vec::new()).request,
            ),
            Err(ModelRequestError::DuplicateContextBinding)
        );
    }

    #[test]
    fn model_request_limits_reject_invalid_bounds() {
        assert_eq!(
            ModelRequestLimits::new(Some(0)),
            Err(ModelRequestError::InvalidRequest)
        );
        assert_eq!(
            ModelRequestLimits::new(Some(MAX_MODEL_OUTPUT_TOKENS + 1)),
            Err(ModelRequestError::InvalidRequest)
        );
        assert_eq!(
            ModelRequestLimits::default().resolve_max_output_tokens(0),
            Err(ModelRequestError::InvalidRequest)
        );
        assert!(
            serde_json::from_value::<ModelRequestLimits>(serde_json::json!({
                "maxOutputTokens": 0
            }))
            .is_err()
        );
    }
}
