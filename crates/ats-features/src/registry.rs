use std::collections::BTreeMap;
use std::marker::PhantomData;

use ats_kernel::{FeatureId, SchemaRef};
use ats_runtime::{PayloadError, VersionedPayload};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

pub trait FeatureSpec: Send + Sync + 'static {
    type Request: Serialize + DeserializeOwned;
    type Result: Serialize + DeserializeOwned;
    type ArtifactExtension: Serialize + DeserializeOwned;

    fn id() -> FeatureId;
    fn request_schema() -> SchemaRef;
    fn result_schema() -> SchemaRef;
    fn artifact_extension_schema() -> SchemaRef;
}

trait ErasedFeatureValidator: Send + Sync {
    fn validate_request(&self, payload: &VersionedPayload) -> Result<(), PayloadError>;
    fn validate_result(&self, payload: &VersionedPayload) -> Result<(), PayloadError>;
    fn validate_artifact_extension(&self, payload: &VersionedPayload) -> Result<(), PayloadError>;
}

struct TypedFeatureValidator<S>(PhantomData<S>);

impl<S> ErasedFeatureValidator for TypedFeatureValidator<S>
where
    S: FeatureSpec,
{
    fn validate_request(&self, payload: &VersionedPayload) -> Result<(), PayloadError> {
        payload.decode::<S::Request>(&S::request_schema()).map(drop)
    }

    fn validate_result(&self, payload: &VersionedPayload) -> Result<(), PayloadError> {
        payload.decode::<S::Result>(&S::result_schema()).map(drop)
    }

    fn validate_artifact_extension(&self, payload: &VersionedPayload) -> Result<(), PayloadError> {
        payload
            .decode::<S::ArtifactExtension>(&S::artifact_extension_schema())
            .map(drop)
    }
}

#[derive(Debug, Error)]
pub enum FeatureRegistryError {
    #[error("feature is already registered")]
    DuplicateFeature,
    #[error("feature is not registered")]
    UnknownFeature,
    #[error("feature payload failed schema validation")]
    InvalidPayload(#[source] PayloadError),
}

#[derive(Default)]
pub struct FeatureRegistry {
    validators: BTreeMap<FeatureId, Box<dyn ErasedFeatureValidator>>,
}

impl FeatureRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<S>(&mut self) -> Result<(), FeatureRegistryError>
    where
        S: FeatureSpec,
    {
        let id = S::id();
        if self.validators.contains_key(&id) {
            return Err(FeatureRegistryError::DuplicateFeature);
        }
        self.validators
            .insert(id, Box::new(TypedFeatureValidator::<S>(PhantomData)));
        Ok(())
    }

    pub fn validate_request(
        &self,
        feature_id: &FeatureId,
        payload: &VersionedPayload,
    ) -> Result<(), FeatureRegistryError> {
        self.validator(feature_id)?
            .validate_request(payload)
            .map_err(FeatureRegistryError::InvalidPayload)
    }

    pub fn validate_result(
        &self,
        feature_id: &FeatureId,
        payload: &VersionedPayload,
    ) -> Result<(), FeatureRegistryError> {
        self.validator(feature_id)?
            .validate_result(payload)
            .map_err(FeatureRegistryError::InvalidPayload)
    }

    pub fn validate_artifact_extension(
        &self,
        feature_id: &FeatureId,
        payload: &VersionedPayload,
    ) -> Result<(), FeatureRegistryError> {
        self.validator(feature_id)?
            .validate_artifact_extension(payload)
            .map_err(FeatureRegistryError::InvalidPayload)
    }

    pub fn decode_request<S>(
        &self,
        payload: &VersionedPayload,
    ) -> Result<S::Request, FeatureRegistryError>
    where
        S: FeatureSpec,
    {
        self.validate_request(&S::id(), payload)?;
        payload
            .decode::<S::Request>(&S::request_schema())
            .map_err(FeatureRegistryError::InvalidPayload)
    }

    pub fn decode_result<S>(
        &self,
        payload: &VersionedPayload,
    ) -> Result<S::Result, FeatureRegistryError>
    where
        S: FeatureSpec,
    {
        self.validate_result(&S::id(), payload)?;
        payload
            .decode::<S::Result>(&S::result_schema())
            .map_err(FeatureRegistryError::InvalidPayload)
    }

    pub fn decode_artifact_extension<S>(
        &self,
        payload: &VersionedPayload,
    ) -> Result<S::ArtifactExtension, FeatureRegistryError>
    where
        S: FeatureSpec,
    {
        self.validate_artifact_extension(&S::id(), payload)?;
        payload
            .decode::<S::ArtifactExtension>(&S::artifact_extension_schema())
            .map_err(FeatureRegistryError::InvalidPayload)
    }

    fn validator(
        &self,
        feature_id: &FeatureId,
    ) -> Result<&dyn ErasedFeatureValidator, FeatureRegistryError> {
        self.validators
            .get(feature_id)
            .map(Box::as_ref)
            .ok_or(FeatureRegistryError::UnknownFeature)
    }
}

#[cfg(test)]
mod tests {
    use ats_kernel::{SchemaId, SchemaVersion};
    use ats_runtime::{RunRecord, RunTransition};
    use chrono::Utc;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct GenerateRequest {
        prompt: String,
    }

    #[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct GenerateResult {
        artifact_ref: String,
    }

    #[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct GenerateArtifactExtension {
        summary: String,
    }

    struct GenerateFixture;

    impl FeatureSpec for GenerateFixture {
        type Request = GenerateRequest;
        type Result = GenerateResult;
        type ArtifactExtension = GenerateArtifactExtension;

        fn id() -> FeatureId {
            FeatureId::parse("fixture.generate").unwrap()
        }

        fn request_schema() -> SchemaRef {
            schema("fixture.generate-request")
        }

        fn result_schema() -> SchemaRef {
            schema("fixture.generate-result")
        }

        fn artifact_extension_schema() -> SchemaRef {
            schema("fixture.generate-artifact-extension")
        }
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct AnalyzeRequest {
        log: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct AnalyzeResult {
        report: String,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct AnalyzeArtifactExtension {
        log_count: u32,
    }

    struct AnalyzeFixture;

    impl FeatureSpec for AnalyzeFixture {
        type Request = AnalyzeRequest;
        type Result = AnalyzeResult;
        type ArtifactExtension = AnalyzeArtifactExtension;

        fn id() -> FeatureId {
            FeatureId::parse("fixture.analyze").unwrap()
        }

        fn request_schema() -> SchemaRef {
            schema("fixture.analyze-request")
        }

        fn result_schema() -> SchemaRef {
            schema("fixture.analyze-result")
        }

        fn artifact_extension_schema() -> SchemaRef {
            schema("fixture.analyze-artifact-extension")
        }
    }

    fn schema(id: &str) -> SchemaRef {
        SchemaRef {
            id: SchemaId::parse(id).unwrap(),
            version: SchemaVersion::new(1).unwrap(),
        }
    }

    #[test]
    fn new_feature_registers_without_runtime_enum_changes() {
        let mut registry = FeatureRegistry::new();
        registry.register::<GenerateFixture>().unwrap();
        registry.register::<AnalyzeFixture>().unwrap();

        let request = VersionedPayload::from_typed(
            GenerateFixture::request_schema(),
            &GenerateRequest {
                prompt: "fixture".into(),
            },
        )
        .unwrap();
        registry
            .validate_request(&GenerateFixture::id(), &request)
            .unwrap();
        assert_eq!(
            registry
                .decode_request::<GenerateFixture>(&request)
                .unwrap(),
            GenerateRequest {
                prompt: "fixture".into()
            }
        );

        let result = VersionedPayload::from_typed(
            GenerateFixture::result_schema(),
            &GenerateResult {
                artifact_ref: "artifacts/fixture".into(),
            },
        )
        .unwrap();
        registry
            .validate_result(&GenerateFixture::id(), &result)
            .unwrap();

        let extension = VersionedPayload::from_typed(
            GenerateFixture::artifact_extension_schema(),
            &GenerateArtifactExtension {
                summary: "published".into(),
            },
        )
        .unwrap();
        assert_eq!(
            registry
                .decode_artifact_extension::<GenerateFixture>(&extension)
                .unwrap(),
            GenerateArtifactExtension {
                summary: "published".into()
            }
        );

        let mut run = RunRecord::new(GenerateFixture::id(), request);
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        run.apply_transition(RunTransition::Succeed { result }, Utc::now())
            .unwrap();
        registry
            .validate_result(run.feature_id(), run.result().unwrap())
            .unwrap();
    }

    #[test]
    fn unknown_mismatched_and_malformed_payloads_are_rejected() {
        let mut registry = FeatureRegistry::new();
        registry.register::<GenerateFixture>().unwrap();
        assert!(matches!(
            registry.register::<GenerateFixture>(),
            Err(FeatureRegistryError::DuplicateFeature)
        ));

        let request = VersionedPayload::from_typed(
            AnalyzeFixture::request_schema(),
            &AnalyzeRequest { log: "x".into() },
        )
        .unwrap();
        assert!(matches!(
            registry.validate_request(&AnalyzeFixture::id(), &request),
            Err(FeatureRegistryError::UnknownFeature)
        ));
        assert!(matches!(
            registry.validate_request(&GenerateFixture::id(), &request),
            Err(FeatureRegistryError::InvalidPayload(
                PayloadError::SchemaMismatch
            ))
        ));

        let wrong_shape = VersionedPayload::from_typed(
            GenerateFixture::request_schema(),
            &AnalyzeResult {
                report: "wrong fields".into(),
            },
        )
        .unwrap();
        assert!(matches!(
            registry.validate_request(&GenerateFixture::id(), &wrong_shape),
            Err(FeatureRegistryError::InvalidPayload(PayloadError::Decode(
                _
            )))
        ));
        assert!(matches!(
            registry.validate_artifact_extension(&GenerateFixture::id(), &wrong_shape),
            Err(FeatureRegistryError::InvalidPayload(_))
        ));
    }
}
