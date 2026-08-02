use std::collections::BTreeMap;

use ats_game_context::{
    ContributionResolverError, EvidenceQuery, EvidenceQueryError, LoadedGamePack,
    VerifiedContributionSet, VerifiedTruthSnapshot,
};
use ats_kernel::{
    ContributionId, FeatureId, RecipeId, SchemaId, SchemaRef, SchemaVersion, Sha256Digest,
};
use ats_runtime::{
    CancellationToken, FinishReason, ModelClient, ModelError, ModelGamePackRef, ModelRequestError,
    ModelRequestSnapshot, ModelResourceRef, TokenUsage,
};
use ats_workspace::ResourceAsset;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;
use crate::prompt::{FeatureRecipe, FeatureRecipeError, FeatureRecipeLoader};

const LOG_ANALYZE_RECIPE_BYTES: &[u8] = include_bytes!("../recipes/log-analyze.json");
const LOG_ANALYZE_RECIPE_SHA256: &str =
    "2c745f0ab0dffd260373ff9fe86a3b9d1ea488ba7e7c3e39fe33c4503b1973e2";
const DEFAULT_MAX_LOG_CHARS: usize = 30_000;
const MAX_LOG_CHARS: usize = 100_000;
const MAX_EVIDENCE_RECORDS: usize = 20;
const MAX_SELECTED_RESOURCES: usize = 16;

pub struct LogAnalyzeFeature;

impl FeatureSpec for LogAnalyzeFeature {
    type Request = LogAnalyzeRequest;
    type Result = LogAnalyzeResult;
    type ArtifactExtension = LogAnalyzeArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("log.analyze").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema("feature.log-analyze-request")
    }

    fn result_schema() -> SchemaRef {
        schema("feature.log-analyze-result")
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.log-analyze-artifact-extension")
    }
}

impl LogAnalyzeFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: log_rules_slot(),
            schema: schema("pack.log-rules"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogAnalyzeRequest {
    pub log_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_log_chars: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogFinding {
    pub severity: FindingSeverity,
    pub title: String,
    pub evidence: Vec<String>,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogAnalyzeResult {
    pub summary: String,
    pub findings: Vec<LogFinding>,
    pub next_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogAnalyzeArtifactExtension {
    pub model_request_sha256: Sha256Digest,
    pub game_pack_sha256: Sha256Digest,
    pub truth_snapshot_id: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LogAnalysisRules {
    ecosystems: Vec<LogEcosystemRule>,
    priorities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LogEcosystemRule {
    label: String,
    indicators: Vec<String>,
    interpretations: Vec<String>,
    checks: Vec<String>,
}

impl LogAnalysisRules {
    fn validate(&self) -> Result<(), LogAnalyzeError> {
        if self.ecosystems.is_empty()
            || self.ecosystems.len() > 32
            || self.priorities.is_empty()
            || self.priorities.len() > 32
            || self
                .priorities
                .iter()
                .any(|value| !valid_text(value, 1_000))
        {
            return Err(LogAnalyzeError::InvalidPackRules);
        }
        for rule in &self.ecosystems {
            if !valid_text(&rule.label, 128)
                || rule.indicators.is_empty()
                || rule.indicators.len() > 64
                || rule.interpretations.is_empty()
                || rule.interpretations.len() > 32
                || rule.checks.is_empty()
                || rule.checks.len() > 32
                || rule.indicators.iter().any(|value| !valid_text(value, 256))
                || rule
                    .interpretations
                    .iter()
                    .chain(&rule.checks)
                    .any(|value| !valid_text(value, 1_000))
            {
                return Err(LogAnalyzeError::InvalidPackRules);
            }
        }
        Ok(())
    }
}

pub struct LogAnalyzeContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
    pub truth_snapshot: &'a VerifiedTruthSnapshot,
    pub evidence_query: &'a EvidenceQuery,
    pub selected_resources: &'a [ResourceAsset],
    pub project_context: Option<&'a str>,
    pub custom_instructions: Option<&'a str>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LogAnalyzeExecution {
    pub result: LogAnalyzeResult,
    pub request_snapshot: ModelRequestSnapshot,
    pub response_model: String,
    pub usage: TokenUsage,
}

pub struct LogAnalyzeService {
    recipe: FeatureRecipe,
}

impl LogAnalyzeService {
    pub fn built_in() -> Result<Self, LogAnalyzeError> {
        let expected = Sha256Digest::parse(LOG_ANALYZE_RECIPE_SHA256)
            .map_err(|_| LogAnalyzeError::InvalidRecipeContract)?;
        Self::from_recipe(FeatureRecipeLoader::load(
            LOG_ANALYZE_RECIPE_BYTES,
            &expected,
        )?)
    }

    pub fn from_recipe(recipe: FeatureRecipe) -> Result<Self, LogAnalyzeError> {
        if recipe.feature_id() != &LogAnalyzeFeature::id()
            || recipe.output_contract().schema != LogAnalyzeFeature::result_schema()
        {
            return Err(LogAnalyzeError::InvalidRecipeContract);
        }
        Ok(Self { recipe })
    }

    pub async fn execute<C>(
        &self,
        client: &C,
        request: LogAnalyzeRequest,
        context: LogAnalyzeContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<LogAnalyzeExecution, LogAnalyzeError>
    where
        C: ModelClient + ?Sized,
    {
        validate_context(&context)?;
        if cancellation.is_cancelled() {
            return Err(LogAnalyzeError::Cancelled);
        }
        let log = bounded_log(&request)?;
        let rules: LogAnalysisRules = context.contributions.decode(&log_rules_slot())?;
        rules.validate()?;

        let evidence = context.truth_snapshot.query(context.evidence_query)?;
        if evidence.len() > MAX_EVIDENCE_RECORDS {
            return Err(LogAnalyzeError::TooMuchEvidence);
        }
        if context.selected_resources.len() > MAX_SELECTED_RESOURCES {
            return Err(LogAnalyzeError::TooManyResources);
        }

        let resource_refs = context
            .selected_resources
            .iter()
            .map(|resource| ModelResourceRef {
                resource_id: resource.resource_id().clone(),
                logical_role: resource.logical_role().to_owned(),
                selected_version: resource.selected_version().clone(),
                media_type: resource.selected().blob.media_type.clone(),
            })
            .collect::<Vec<_>>();

        let slots = BTreeMap::from([
            (
                "output.contract".into(),
                serialize_slot(&self.recipe.output_contract().json_schema)?,
            ),
            ("pack.guidance".into(), serialize_slot(&rules)?),
            ("truth.evidence".into(), serialize_slot(&evidence)?),
            ("resources.selected".into(), serialize_slot(&resource_refs)?),
            (
                "project.context".into(),
                optional_bounded(context.project_context, 8_000)?,
            ),
            (
                "runtime.custom_instructions".into(),
                optional_bounded(context.custom_instructions, 4_000)?,
            ),
            (
                "request.context".into(),
                optional_bounded(request.context_hint.as_deref(), 4_000)?,
            ),
            ("request.log".into(), log),
        ]);
        let model_request = self.recipe.render(&slots, context.model)?;
        let snapshot = ModelRequestSnapshot::new(
            LogAnalyzeFeature::id(),
            self.recipe.recipe_ref(),
            ModelGamePackRef {
                id: context.pack.id().clone(),
                sha256: context.pack.content_sha256().clone(),
            },
            Some(context.truth_snapshot.manifest().snapshot_id().clone()),
            resource_refs,
            model_request,
        )?;
        let response = client.complete(snapshot.clone(), cancellation).await?;
        if cancellation.is_cancelled() {
            return Err(LogAnalyzeError::Cancelled);
        }
        if response.finish_reason == FinishReason::MaxTokens {
            return Err(LogAnalyzeError::TruncatedModelOutput);
        }
        let result: LogAnalyzeResult = serde_json::from_str(&response.content)
            .map_err(|_| LogAnalyzeError::InvalidModelOutput)?;
        validate_result(&result)?;
        Ok(LogAnalyzeExecution {
            result,
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
pub enum LogAnalyzeError {
    #[error("log analysis input is invalid")]
    InvalidInput,
    #[error("log analysis context does not share one verified Game Pack identity")]
    ContextIdentityMismatch,
    #[error("log analysis Pack rules are invalid")]
    InvalidPackRules,
    #[error("log analysis selected too much Truth Evidence")]
    TooMuchEvidence,
    #[error("log analysis selected too many resources")]
    TooManyResources,
    #[error("log analysis Recipe does not match its typed Feature contract")]
    InvalidRecipeContract,
    #[error("log analysis model output was truncated")]
    TruncatedModelOutput,
    #[error("log analysis model output failed typed decoding")]
    InvalidModelOutput,
    #[error("log analysis was cancelled")]
    Cancelled,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Evidence(#[from] EvidenceQueryError),
    #[error(transparent)]
    Recipe(#[from] FeatureRecipeError),
    #[error(transparent)]
    Request(#[from] ModelRequestError),
    #[error(transparent)]
    Model(#[from] ModelError),
}

fn validate_context(context: &LogAnalyzeContext<'_>) -> Result<(), LogAnalyzeError> {
    let manifest = context.truth_snapshot.manifest();
    if context.contributions.feature_id() != &LogAnalyzeFeature::id()
        || context.contributions.game_pack_id() != context.pack.id()
        || context.contributions.game_pack_sha256() != context.pack.content_sha256()
        || manifest.game_pack_id() != context.pack.id()
        || manifest.game_pack_sha256() != context.pack.content_sha256()
    {
        return Err(LogAnalyzeError::ContextIdentityMismatch);
    }
    Ok(())
}

fn bounded_log(request: &LogAnalyzeRequest) -> Result<String, LogAnalyzeError> {
    if request.log_text.trim().is_empty() || request.log_text.contains('\0') {
        return Err(LogAnalyzeError::InvalidInput);
    }
    let max_chars = request.max_log_chars.unwrap_or(DEFAULT_MAX_LOG_CHARS);
    if max_chars == 0 || max_chars > MAX_LOG_CHARS {
        return Err(LogAnalyzeError::InvalidInput);
    }
    let count = request.log_text.chars().count();
    if count <= max_chars {
        return Ok(request.log_text.clone());
    }
    Ok(request.log_text.chars().skip(count - max_chars).collect())
}

fn validate_result(result: &LogAnalyzeResult) -> Result<(), LogAnalyzeError> {
    if !valid_text(&result.summary, 4_000)
        || result.findings.len() > 50
        || result.next_steps.len() > 50
        || result
            .next_steps
            .iter()
            .any(|value| !valid_text(value, 2_000))
    {
        return Err(LogAnalyzeError::InvalidModelOutput);
    }
    for finding in &result.findings {
        if !valid_text(&finding.title, 512)
            || !valid_text(&finding.recommendation, 2_000)
            || finding.evidence.len() > 20
            || finding
                .evidence
                .iter()
                .any(|value| !valid_text(value, 2_000))
        {
            return Err(LogAnalyzeError::InvalidModelOutput);
        }
    }
    Ok(())
}

fn optional_bounded(value: Option<&str>, max_chars: usize) -> Result<String, LogAnalyzeError> {
    let value = value.unwrap_or("");
    if value.chars().count() > max_chars || value.chars().any(|character| character == '\0') {
        return Err(LogAnalyzeError::InvalidInput);
    }
    Ok(value.to_owned())
}

fn serialize_slot<T>(value: &T) -> Result<String, LogAnalyzeError>
where
    T: Serialize + ?Sized,
{
    serde_json::to_string_pretty(value).map_err(|_| LogAnalyzeError::InvalidInput)
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= max_chars
        && !value.chars().any(|character| character == '\0')
}

fn log_rules_slot() -> ContributionId {
    ContributionId::parse("log.analyze.rules").expect("built-in contribution ID is valid")
}

fn schema(id: &str) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(1).expect("built-in schema version is valid"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use ats_game_context::{
        ContributionResolver, GamePackLoader, TruthEvidenceRecord, TruthSnapshotIndex,
        TruthSnapshotManifest, TruthSnapshotSource,
    };
    use ats_kernel::{GamePackId, PrimitiveId};
    use ats_runtime::{ModelResponse, ModelStream};
    use chrono::Utc;
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
            _request: ModelRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<ModelStream, ModelError> {
            Ok(Box::pin(stream::empty()))
        }
    }

    fn digest(bytes: &[u8]) -> Sha256Digest {
        Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).unwrap()
    }

    fn synthetic_pack(label: &str) -> LoadedGamePack {
        let json = serde_json::json!({
            "schemaVersion": 2,
            "id": format!("fixture-{label}"),
            "displayName": format!("Fixture {label}"),
            "contributions": [{
                "slotId": "log.analyze.rules",
                "featureId": "log.analyze",
                "schema": {"id": "pack.log-rules", "version": 1},
                "requiredPrimitives": ["log.dotnet-parser"],
                "payload": {
                    "ecosystems": [{
                        "label": label,
                        "indicators": [format!("{label}-indicator")],
                        "interpretations": [format!("{label} interpretation")],
                        "checks": [format!("{label} check")]
                    }],
                    "priorities": [format!("{label} priority")]
                }
            }]
        });
        let bytes = serde_json::to_vec(&json).unwrap();
        GamePackLoader::load(&bytes, &digest(&bytes)).unwrap()
    }

    fn snapshot(pack: &LoadedGamePack) -> VerifiedTruthSnapshot {
        let records = vec![TruthEvidenceRecord {
            source_id: "game".into(),
            symbol: "Fixture.Symbol".into(),
            purpose: "fixture lifecycle".into(),
            bounded_excerpt: "fixture signal appears before the failure".into(),
            relative_path: "indexes/game/Symbol.cs".into(),
        }];
        let index_bytes = serde_json::to_vec(&records).unwrap();
        let manifest = TruthSnapshotManifest::new(
            pack,
            vec![TruthSnapshotSource {
                id: "game".into(),
                kind: "local_file".into(),
                version: Some("1".into()),
                relative_path: "sources/game.bin".into(),
                sha256: digest(b"game"),
                byte_length: 4,
            }],
            vec![TruthSnapshotIndex {
                id: "game-code".into(),
                provider: PrimitiveId::parse("truth.code-facts").unwrap(),
                relative_path: "indexes/game.json".into(),
                sha256: digest(&index_bytes),
                record_count: 1,
            }],
            BTreeMap::from([("indexer".into(), "1".into())]),
            Utc::now(),
        )
        .unwrap();
        VerifiedTruthSnapshot::verify(
            pack,
            manifest,
            BTreeMap::from([("game-code".into(), records)]),
        )
        .unwrap()
    }

    fn contributions(pack: &LoadedGamePack) -> VerifiedContributionSet {
        ContributionResolver::new([PrimitiveId::parse("log.dotnet-parser").unwrap()])
            .resolve(
                pack,
                &LogAnalyzeFeature::id(),
                &[LogAnalyzeFeature::contribution_requirement()],
            )
            .unwrap()
    }

    fn model() -> MockModel {
        MockModel {
            snapshots: Mutex::new(Vec::new()),
            response: ModelResponse {
                model: "fixture-model".into(),
                content: serde_json::json!({
                    "summary": "One primary failure",
                    "findings": [{
                        "severity": "error",
                        "title": "Primary failure",
                        "evidence": ["bounded evidence"],
                        "recommendation": "Apply the smallest verified correction"
                    }],
                    "nextSteps": ["Run the targeted validation again"]
                })
                .to_string(),
                finish_reason: FinishReason::EndTurn,
                usage: TokenUsage {
                    input_tokens: 100,
                    output_tokens: 40,
                },
            },
        }
    }

    fn context<'a>(
        pack: &'a LoadedGamePack,
        contributions: &'a VerifiedContributionSet,
        snapshot: &'a VerifiedTruthSnapshot,
        query: &'a EvidenceQuery,
        custom_instructions: Option<&'a str>,
    ) -> LogAnalyzeContext<'a> {
        LogAnalyzeContext {
            pack,
            contributions,
            truth_snapshot: snapshot,
            evidence_query: query,
            selected_resources: &[],
            project_context: Some("sanitized fixture project"),
            custom_instructions,
            model: None,
        }
    }

    #[tokio::test]
    async fn assembles_verified_context_truncates_unicode_and_decodes_typed_result() {
        let pack = ats_game_context::GamePackLoader::load_built_in_sts2().unwrap();
        let contributions = contributions(&pack);
        let snapshot = snapshot(&pack);
        let query = EvidenceQuery {
            symbols: vec!["fixture".into()],
            terms: vec!["signal".into()],
            limit: 5,
        };
        let model = model();
        let service = LogAnalyzeService::built_in().unwrap();
        let execution = service
            .execute(
                &model,
                LogAnalyzeRequest {
                    log_text: "discard-甲乙-primary-tail".into(),
                    context_hint: Some("recent change".into()),
                    max_log_chars: Some(12),
                },
                context(
                    &pack,
                    &contributions,
                    &snapshot,
                    &query,
                    Some("CUSTOM-INSTRUCTION-CANARY"),
                ),
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(execution.result.findings.len(), 1);
        execution.request_snapshot.verify().unwrap();
        assert_eq!(execution.request_snapshot.recipe().id, *service.recipe_id());
        let rendered = execution
            .request_snapshot
            .request()
            .messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<String>();
        assert!(!rendered.contains("discard"));
        assert!(rendered.contains("primary-tail"));
        assert_eq!(rendered.matches("CUSTOM-INSTRUCTION-CANARY").count(), 1);
        assert!(rendered.contains("fixture signal appears"));
    }

    #[tokio::test]
    async fn different_pack_guidance_changes_the_same_recipe_request_identity() {
        let service = LogAnalyzeService::built_in().unwrap();
        let query = EvidenceQuery {
            symbols: vec!["fixture".into()],
            terms: vec!["signal".into()],
            limit: 5,
        };
        let mut hashes = Vec::new();
        for label in ["alpha", "beta"] {
            let pack = synthetic_pack(label);
            let contributions = contributions(&pack);
            let snapshot = snapshot(&pack);
            let execution = service
                .execute(
                    &model(),
                    LogAnalyzeRequest {
                        log_text: "fixture failure".into(),
                        context_hint: None,
                        max_log_chars: None,
                    },
                    context(&pack, &contributions, &snapshot, &query, None),
                    &CancellationToken::new(),
                )
                .await
                .unwrap();
            hashes.push(execution.request_snapshot.request_sha256().clone());
        }
        assert_ne!(hashes[0], hashes[1]);
    }

    #[tokio::test]
    async fn mismatched_context_and_invalid_model_output_fail_without_fabrication() {
        let alpha = synthetic_pack("alpha");
        let beta = synthetic_pack("beta");
        let contributions = contributions(&alpha);
        let beta_snapshot = snapshot(&beta);
        let query = EvidenceQuery {
            symbols: vec!["fixture".into()],
            terms: vec!["signal".into()],
            limit: 5,
        };
        let service = LogAnalyzeService::built_in().unwrap();
        assert!(matches!(
            service
                .execute(
                    &model(),
                    LogAnalyzeRequest {
                        log_text: "failure".into(),
                        context_hint: None,
                        max_log_chars: None,
                    },
                    context(&alpha, &contributions, &beta_snapshot, &query, None),
                    &CancellationToken::new(),
                )
                .await,
            Err(LogAnalyzeError::ContextIdentityMismatch)
        ));

        let snapshot = snapshot(&alpha);
        let invalid = MockModel {
            snapshots: Mutex::new(Vec::new()),
            response: ModelResponse {
                content: "not-json".into(),
                ..model().response
            },
        };
        assert!(matches!(
            service
                .execute(
                    &invalid,
                    LogAnalyzeRequest {
                        log_text: "failure".into(),
                        context_hint: None,
                        max_log_chars: None,
                    },
                    context(&alpha, &contributions, &snapshot, &query, None),
                    &CancellationToken::new(),
                )
                .await,
            Err(LogAnalyzeError::InvalidModelOutput)
        ));
    }

    #[test]
    fn feature_contract_and_builtin_recipe_share_exact_schemas() {
        let service = LogAnalyzeService::built_in().unwrap();
        assert_eq!(service.recipe_id().as_str(), "recipe.log-analyze");
        assert_eq!(
            LogAnalyzeFeature::contribution_requirement().slot_id,
            log_rules_slot()
        );
        assert!(GamePackId::parse("fixture-game").is_ok());
    }
}
