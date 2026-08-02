use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use ats_kernel::{FeatureId, RecipeId, SchemaVersion, Sha256Digest};
use ats_runtime::{ModelMessage, ModelMessageRole, ModelOutputContract, ModelRequest, RecipeRef};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const FEATURE_RECIPE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawFeatureRecipe {
    schema_version: u32,
    id: RecipeId,
    feature_id: FeatureId,
    version: SchemaVersion,
    messages: Vec<RecipeMessage>,
    slots: Vec<RecipeSlot>,
    output_contract: ModelOutputContract,
    max_output_tokens: u32,
    temperature: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecipeMessage {
    role: ModelMessageRole,
    template: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RecipeSlot {
    id: String,
    required: bool,
    max_chars: usize,
}

#[derive(Debug, Clone)]
pub struct FeatureRecipe {
    id: RecipeId,
    feature_id: FeatureId,
    version: SchemaVersion,
    sha256: Sha256Digest,
    messages: Vec<RecipeMessage>,
    slots: BTreeMap<String, RecipeSlot>,
    output_contract: ModelOutputContract,
    max_output_tokens: u32,
    temperature: Option<f32>,
}

impl FeatureRecipe {
    pub fn render(
        &self,
        values: &BTreeMap<String, String>,
        model: Option<String>,
    ) -> Result<ModelRequest, FeatureRecipeError> {
        if values.keys().any(|id| !self.slots.contains_key(id)) {
            return Err(FeatureRecipeError::UnexpectedSlot);
        }
        for slot in self.slots.values() {
            let value = values.get(&slot.id).map(String::as_str).unwrap_or("");
            if (slot.required && value.trim().is_empty()) || value.chars().count() > slot.max_chars
            {
                return Err(if slot.required && value.trim().is_empty() {
                    FeatureRecipeError::MissingSlot
                } else {
                    FeatureRecipeError::SlotTooLarge
                });
            }
        }

        let messages = self
            .messages
            .iter()
            .map(|message| {
                Ok(ModelMessage {
                    role: message.role,
                    content: render_template(&message.template, values, &self.slots)?,
                })
            })
            .collect::<Result<Vec<_>, FeatureRecipeError>>()?;
        Ok(ModelRequest {
            messages,
            output_contract: self.output_contract.clone(),
            max_output_tokens: self.max_output_tokens,
            temperature: self.temperature,
            model,
        })
    }

    #[must_use]
    pub fn id(&self) -> &RecipeId {
        &self.id
    }

    #[must_use]
    pub fn feature_id(&self) -> &FeatureId {
        &self.feature_id
    }

    #[must_use]
    pub fn version(&self) -> SchemaVersion {
        self.version
    }

    #[must_use]
    pub fn sha256(&self) -> &Sha256Digest {
        &self.sha256
    }

    #[must_use]
    pub fn output_contract(&self) -> &ModelOutputContract {
        &self.output_contract
    }

    #[must_use]
    pub fn recipe_ref(&self) -> RecipeRef {
        RecipeRef {
            id: self.id.clone(),
            version: self.version,
            sha256: self.sha256.clone(),
        }
    }
}

pub struct FeatureRecipeLoader;

impl FeatureRecipeLoader {
    pub fn load(
        bytes: &[u8],
        expected_sha256: &Sha256Digest,
    ) -> Result<FeatureRecipe, FeatureRecipeError> {
        let actual = sha256_bytes(bytes);
        if &actual != expected_sha256 {
            return Err(FeatureRecipeError::ContentHashMismatch);
        }
        let raw: RawFeatureRecipe =
            serde_json::from_slice(bytes).map_err(FeatureRecipeError::InvalidJson)?;
        validate_raw(&raw)?;
        let slots = raw
            .slots
            .into_iter()
            .map(|slot| (slot.id.clone(), slot))
            .collect();
        Ok(FeatureRecipe {
            id: raw.id,
            feature_id: raw.feature_id,
            version: raw.version,
            sha256: actual,
            messages: raw.messages,
            slots,
            output_contract: raw.output_contract,
            max_output_tokens: raw.max_output_tokens,
            temperature: raw.temperature,
        })
    }
}

#[derive(Debug, Default)]
pub struct FeatureRecipeRegistry {
    recipes: BTreeMap<(RecipeId, SchemaVersion), FeatureRecipe>,
}

impl FeatureRecipeRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, recipe: FeatureRecipe) -> Result<(), FeatureRecipeError> {
        let key = (recipe.id.clone(), recipe.version);
        match self.recipes.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(recipe);
                Ok(())
            }
            Entry::Occupied(_) => Err(FeatureRecipeError::DuplicateRecipe),
        }
    }

    pub fn require(
        &self,
        id: &RecipeId,
        version: SchemaVersion,
    ) -> Result<&FeatureRecipe, FeatureRecipeError> {
        self.recipes
            .get(&(id.clone(), version))
            .ok_or(FeatureRecipeError::UnknownRecipe)
    }
}

#[derive(Debug, Error)]
pub enum FeatureRecipeError {
    #[error("feature recipe content hash does not match its pinned identity")]
    ContentHashMismatch,
    #[error("feature recipe JSON is invalid")]
    InvalidJson(#[source] serde_json::Error),
    #[error("feature recipe schema version is unsupported")]
    UnsupportedSchema,
    #[error("feature recipe contract is invalid")]
    InvalidContract,
    #[error("feature recipe template is invalid")]
    InvalidTemplate,
    #[error("feature recipe declares a duplicate slot")]
    DuplicateSlot,
    #[error("feature recipe rendering is missing a required slot")]
    MissingSlot,
    #[error("feature recipe rendering supplied an unknown slot")]
    UnexpectedSlot,
    #[error("feature recipe slot value exceeds its bound")]
    SlotTooLarge,
    #[error("feature recipe is already registered")]
    DuplicateRecipe,
    #[error("feature recipe is not registered")]
    UnknownRecipe,
}

fn validate_raw(raw: &RawFeatureRecipe) -> Result<(), FeatureRecipeError> {
    if raw.schema_version != FEATURE_RECIPE_SCHEMA_VERSION {
        return Err(FeatureRecipeError::UnsupportedSchema);
    }
    if raw.messages.is_empty()
        || raw.messages.len() > 8
        || raw.slots.is_empty()
        || raw.slots.len() > 32
        || raw.max_output_tokens == 0
        || raw.max_output_tokens > 65_536
        || raw
            .temperature
            .is_some_and(|value| !value.is_finite() || !(0.0..=2.0).contains(&value))
        || !raw.output_contract.json_schema.is_object()
    {
        return Err(FeatureRecipeError::InvalidContract);
    }

    let mut slot_ids = BTreeSet::new();
    for slot in &raw.slots {
        if !valid_slot_id(&slot.id)
            || slot.max_chars == 0
            || slot.max_chars > 200_000
            || !slot_ids.insert(slot.id.as_str())
        {
            return Err(if slot_ids.contains(slot.id.as_str()) {
                FeatureRecipeError::DuplicateSlot
            } else {
                FeatureRecipeError::InvalidContract
            });
        }
    }

    let mut occurrences = BTreeMap::<&str, usize>::new();
    for message in &raw.messages {
        if message.template.trim().is_empty() || message.template.chars().count() > 100_000 {
            return Err(FeatureRecipeError::InvalidTemplate);
        }
        for token in template_tokens(&message.template)? {
            if !slot_ids.contains(token) {
                return Err(FeatureRecipeError::InvalidTemplate);
            }
            *occurrences.entry(token).or_default() += 1;
        }
    }
    if slot_ids
        .iter()
        .any(|slot| occurrences.get(slot).copied() != Some(1))
    {
        return Err(FeatureRecipeError::InvalidTemplate);
    }
    Ok(())
}

fn render_template(
    template: &str,
    values: &BTreeMap<String, String>,
    slots: &BTreeMap<String, RecipeSlot>,
) -> Result<String, FeatureRecipeError> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let (prefix, after_start) = rest.split_at(start);
        if prefix.contains("}}") {
            return Err(FeatureRecipeError::InvalidTemplate);
        }
        rendered.push_str(prefix);
        let after_start = &after_start[2..];
        let end = after_start
            .find("}}")
            .ok_or(FeatureRecipeError::InvalidTemplate)?;
        let (token, after_token) = after_start.split_at(end);
        if !slots.contains_key(token) {
            return Err(FeatureRecipeError::InvalidTemplate);
        }
        if let Some(value) = values.get(token) {
            rendered.push_str(value);
        }
        rest = &after_token[2..];
    }
    if rest.contains("}}") {
        return Err(FeatureRecipeError::InvalidTemplate);
    }
    rendered.push_str(rest);
    Ok(rendered)
}

fn template_tokens(template: &str) -> Result<Vec<&str>, FeatureRecipeError> {
    let mut tokens = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let (prefix, after_start) = rest.split_at(start);
        if prefix.contains("}}") {
            return Err(FeatureRecipeError::InvalidTemplate);
        }
        let after_start = &after_start[2..];
        let end = after_start
            .find("}}")
            .ok_or(FeatureRecipeError::InvalidTemplate)?;
        let (token, after_token) = after_start.split_at(end);
        if !valid_slot_id(token) {
            return Err(FeatureRecipeError::InvalidTemplate);
        }
        tokens.push(token);
        rest = &after_token[2..];
    }
    if rest.contains("}}") {
        return Err(FeatureRecipeError::InvalidTemplate);
    }
    Ok(tokens)
}

fn valid_slot_id(value: &str) -> bool {
    let mut segments = value.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    let Some(second) = segments.next() else {
        return false;
    };
    valid_segment(first) && valid_segment(second) && segments.all(valid_segment)
}

fn valid_segment(value: &str) -> bool {
    value
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter always returns a valid digest")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(bytes: &[u8]) -> FeatureRecipe {
        FeatureRecipeLoader::load(bytes, &sha256_bytes(bytes)).unwrap()
    }

    #[test]
    fn exact_slots_render_once_without_reinterpreting_inserted_tokens() {
        let bytes = br#"{
          "schemaVersion":1,
          "id":"recipe.fixture",
          "featureId":"fixture.analyze",
          "version":1,
          "messages":[{"role":"user","template":"A={{request.alpha}} B={{request.beta}}"}],
          "slots":[
            {"id":"request.alpha","required":true,"maxChars":100},
            {"id":"request.beta","required":false,"maxChars":100}
          ],
          "outputContract":{"schema":{"id":"feature.fixture-result","version":1},"jsonSchema":{"type":"object"}},
          "maxOutputTokens":128,
          "temperature":0.0
        }"#;
        let recipe = load(bytes);
        let values = BTreeMap::from([
            ("request.alpha".into(), "{{request.beta}}".into()),
            ("request.beta".into(), "literal".into()),
        ]);
        let request = recipe.render(&values, None).unwrap();
        assert_eq!(request.messages[0].content, "A={{request.beta}} B=literal");
    }

    #[test]
    fn loader_and_renderer_reject_tampering_missing_and_unknown_slots() {
        let bytes = br#"{
          "schemaVersion":1,
          "id":"recipe.fixture",
          "featureId":"fixture.analyze",
          "version":1,
          "messages":[{"role":"user","template":"{{request.value}}"}],
          "slots":[{"id":"request.value","required":true,"maxChars":4}],
          "outputContract":{"schema":{"id":"feature.fixture-result","version":1},"jsonSchema":{"type":"object"}},
          "maxOutputTokens":128,
          "temperature":null
        }"#;
        let wrong = Sha256Digest::parse("0".repeat(64)).unwrap();
        assert!(matches!(
            FeatureRecipeLoader::load(bytes, &wrong),
            Err(FeatureRecipeError::ContentHashMismatch)
        ));
        let recipe = load(bytes);
        assert!(matches!(
            recipe.render(&BTreeMap::new(), None),
            Err(FeatureRecipeError::MissingSlot)
        ));
        assert!(matches!(
            recipe.render(
                &BTreeMap::from([("request.other".into(), "x".into())]),
                None
            ),
            Err(FeatureRecipeError::UnexpectedSlot)
        ));
        assert!(matches!(
            recipe.render(
                &BTreeMap::from([("request.value".into(), "12345".into())]),
                None
            ),
            Err(FeatureRecipeError::SlotTooLarge)
        ));
    }

    #[test]
    fn duplicate_or_repeated_slots_are_rejected() {
        let duplicate = br#"{
          "schemaVersion":1,
          "id":"recipe.fixture",
          "featureId":"fixture.analyze",
          "version":1,
          "messages":[{"role":"user","template":"{{request.value}}"}],
          "slots":[
            {"id":"request.value","required":true,"maxChars":4},
            {"id":"request.value","required":true,"maxChars":4}
          ],
          "outputContract":{"schema":{"id":"feature.fixture-result","version":1},"jsonSchema":{"type":"object"}},
          "maxOutputTokens":128,
          "temperature":null
        }"#;
        assert!(matches!(
            FeatureRecipeLoader::load(duplicate, &sha256_bytes(duplicate)),
            Err(FeatureRecipeError::DuplicateSlot)
        ));
    }

    #[test]
    fn duplicate_registry_insert_preserves_the_original_recipe() {
        let bytes = br#"{
          "schemaVersion":1,
          "id":"recipe.fixture",
          "featureId":"fixture.analyze",
          "version":1,
          "messages":[{"role":"user","template":"{{request.value}}"}],
          "slots":[{"id":"request.value","required":true,"maxChars":4}],
          "outputContract":{"schema":{"id":"feature.fixture-result","version":1},"jsonSchema":{"type":"object"}},
          "maxOutputTokens":128,
          "temperature":null
        }"#;
        let recipe = load(bytes);
        let mut registry = FeatureRecipeRegistry::new();
        registry.register(recipe.clone()).unwrap();
        assert!(matches!(
            registry.register(recipe),
            Err(FeatureRecipeError::DuplicateRecipe)
        ));
        assert_eq!(
            registry
                .require(
                    &RecipeId::parse("recipe.fixture").unwrap(),
                    SchemaVersion::new(1).unwrap()
                )
                .unwrap()
                .sha256(),
            &sha256_bytes(bytes)
        );
    }
}
