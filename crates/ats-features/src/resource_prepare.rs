use std::collections::BTreeSet;
use std::path::PathBuf;

use ats_game_context::{ContributionResolverError, LoadedGamePack, VerifiedContributionSet};
use ats_kernel::{
    ContributionId, FeatureId, ResourceId, SchemaId, SchemaRef, SchemaVersion, Sha256Digest,
};
use ats_runtime::{CancellationToken, MediaClient, MediaError, MediaRequest, MediaRequestSnapshot};
use ats_workspace::{
    ResourceAsset, ResourceBytesIngestRequest, ResourceIngestRequest, ResourceOrigin,
    ResourceRepository,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::FeatureSpec;

pub struct ResourcePrepareFeature;

impl FeatureSpec for ResourcePrepareFeature {
    type Request = ResourcePrepareRequest;
    type Result = ResourcePrepareResult;
    type ArtifactExtension = ResourcePrepareArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("resource.prepare").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema("feature.resource-prepare-request")
    }

    fn result_schema() -> SchemaRef {
        schema("feature.resource-prepare-result")
    }

    fn artifact_extension_schema() -> SchemaRef {
        schema("feature.resource-prepare-artifact-extension")
    }
}

impl ResourcePrepareFeature {
    #[must_use]
    pub fn contribution_requirement() -> ats_game_context::ContributionRequirement {
        ats_game_context::ContributionRequirement {
            slot_id: resource_specs_slot(),
            schema: schema("pack.resource-specs"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePrepareRequest {
    pub logical_role: String,
    pub media_type: String,
    pub source: ResourcePrepareSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourcePrepareSource {
    UserUpload,
    AiGenerated {
        prompt: String,
        file_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    PackDefault,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePrepareResult {
    pub resource_id: ResourceId,
    pub logical_role: String,
    pub origin: ResourceOrigin,
    pub selected_version: Sha256Digest,
    pub media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePrepareArtifactExtension {
    pub resource_id: ResourceId,
    pub selected_version: Sha256Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResourceSpecs {
    pub(crate) roles: Vec<ResourceRoleSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResourceRoleSpec {
    pub(crate) id: String,
    pub(crate) media_types: Vec<String>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) target_path: String,
}

impl ResourceSpecs {
    pub(crate) fn validate(&self) -> Result<(), ResourcePrepareError> {
        if self.roles.is_empty() || self.roles.len() > 128 {
            return Err(ResourcePrepareError::InvalidPackSpecs);
        }
        let mut ids = BTreeSet::new();
        for role in &self.roles {
            if !valid_role(&role.id)
                || role.media_types.is_empty()
                || role.media_types.len() > 16
                || role
                    .media_types
                    .iter()
                    .any(|media| !valid_media_type(media))
                || role.width == 0
                || role.width > 16_384
                || role.height == 0
                || role.height > 16_384
                || !valid_target_template(&role.target_path)
                || !ids.insert(role.id.as_str())
            {
                return Err(ResourcePrepareError::InvalidPackSpecs);
            }
        }
        Ok(())
    }

    pub(crate) fn require_role(
        &self,
        role: &str,
        media_type: &str,
    ) -> Result<&ResourceRoleSpec, ResourcePrepareError> {
        self.roles
            .iter()
            .find(|spec| {
                spec.id == role
                    && spec
                        .media_types
                        .iter()
                        .any(|supported| supported == media_type)
            })
            .ok_or(ResourcePrepareError::UnsupportedResource)
    }
}

pub struct ResourcePrepareContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
}

pub struct ResourcePrepareService;

impl ResourcePrepareService {
    pub fn prepare_file<R>(
        &self,
        repository: &R,
        request: ResourcePrepareRequest,
        source_path: PathBuf,
        context: ResourcePrepareContext<'_>,
    ) -> Result<ResourcePrepareResult, ResourcePrepareError>
    where
        R: ResourceRepository,
    {
        let specs = validate_request_and_context(&request, &context)?;
        specs.require_role(&request.logical_role, &request.media_type)?;
        let origin = match request.source {
            ResourcePrepareSource::UserUpload => ResourceOrigin::UserUpload,
            ResourcePrepareSource::PackDefault => ResourceOrigin::PackDefault {
                game_pack_id: context.pack.id().clone(),
                game_pack_sha256: context.pack.content_sha256().clone(),
                contribution_slot: resource_specs_slot(),
            },
            ResourcePrepareSource::AiGenerated { .. } => {
                return Err(ResourcePrepareError::WrongSourceMode);
            }
        };
        let asset = repository
            .ingest(ResourceIngestRequest {
                logical_role: request.logical_role,
                origin,
                media_type: request.media_type,
                source_path,
            })
            .map_err(|_| ResourcePrepareError::Repository)?;
        Ok(result_from_asset(&asset))
    }

    pub async fn prepare_ai<M, R>(
        &self,
        media: &M,
        repository: &R,
        request: ResourcePrepareRequest,
        context: ResourcePrepareContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<ResourcePrepareResult, ResourcePrepareError>
    where
        M: MediaClient + ?Sized,
        R: ResourceRepository,
    {
        let specs = validate_request_and_context(&request, &context)?;
        let spec = specs.require_role(&request.logical_role, &request.media_type)?;
        let ResourcePrepareSource::AiGenerated {
            prompt,
            file_name,
            model,
        } = request.source
        else {
            return Err(ResourcePrepareError::WrongSourceMode);
        };
        let snapshot = MediaRequestSnapshot::new(MediaRequest {
            prompt,
            logical_role: request.logical_role.clone(),
            media_type: request.media_type.clone(),
            width: Some(spec.width),
            height: Some(spec.height),
            model,
        })?;
        let response = media.generate(snapshot.clone(), cancellation).await?;
        response.validate()?;
        if response.media_type != request.media_type || cancellation.is_cancelled() {
            return Err(if cancellation.is_cancelled() {
                ResourcePrepareError::Cancelled
            } else {
                ResourcePrepareError::InvalidMediaResponse
            });
        }
        let asset = repository
            .ingest_bytes(ResourceBytesIngestRequest {
                logical_role: request.logical_role,
                origin: ResourceOrigin::AiGenerated {
                    provider: response.provider,
                    model: response.model,
                    request_sha256: snapshot.request_sha256().clone(),
                },
                media_type: request.media_type,
                file_name,
                bytes: response.bytes,
            })
            .map_err(|_| ResourcePrepareError::Repository)?;
        Ok(result_from_asset(&asset))
    }

    pub fn select<R>(
        &self,
        repository: &R,
        resource_id: &ResourceId,
        version: &Sha256Digest,
        context: ResourcePrepareContext<'_>,
    ) -> Result<ResourcePrepareResult, ResourcePrepareError>
    where
        R: ResourceRepository,
    {
        validate_context(&context)?;
        let specs: ResourceSpecs = context.contributions.decode(&resource_specs_slot())?;
        specs.validate()?;
        let current = repository
            .load(resource_id)
            .map_err(|_| ResourcePrepareError::Repository)?;
        specs.require_role(current.logical_role(), &current.selected().blob.media_type)?;
        let asset = repository
            .select(resource_id, version)
            .map_err(|_| ResourcePrepareError::Repository)?;
        Ok(result_from_asset(&asset))
    }
}

#[derive(Debug, Error)]
pub enum ResourcePrepareError {
    #[error("resource preparation input is invalid")]
    InvalidInput,
    #[error("resource preparation context identities do not match")]
    ContextIdentityMismatch,
    #[error("resource Pack specification is invalid")]
    InvalidPackSpecs,
    #[error("resource role or media type is unsupported by the Pack")]
    UnsupportedResource,
    #[error("resource preparation source requires a different execution entry")]
    WrongSourceMode,
    #[error("resource media response does not match the request")]
    InvalidMediaResponse,
    #[error("resource preparation was cancelled")]
    Cancelled,
    #[error("resource repository operation failed")]
    Repository,
    #[error(transparent)]
    Contribution(#[from] ContributionResolverError),
    #[error(transparent)]
    Media(#[from] MediaError),
}

fn validate_request_and_context(
    request: &ResourcePrepareRequest,
    context: &ResourcePrepareContext<'_>,
) -> Result<ResourceSpecs, ResourcePrepareError> {
    if !valid_role(&request.logical_role) || !valid_media_type(&request.media_type) {
        return Err(ResourcePrepareError::InvalidInput);
    }
    validate_context(context)?;
    let specs: ResourceSpecs = context.contributions.decode(&resource_specs_slot())?;
    specs.validate()?;
    Ok(specs)
}

fn validate_context(context: &ResourcePrepareContext<'_>) -> Result<(), ResourcePrepareError> {
    if context.contributions.feature_id() != &ResourcePrepareFeature::id()
        || context.contributions.game_pack_id() != context.pack.id()
        || context.contributions.game_pack_sha256() != context.pack.content_sha256()
    {
        return Err(ResourcePrepareError::ContextIdentityMismatch);
    }
    Ok(())
}

fn result_from_asset(asset: &ResourceAsset) -> ResourcePrepareResult {
    ResourcePrepareResult {
        resource_id: asset.resource_id().clone(),
        logical_role: asset.logical_role().to_owned(),
        origin: asset.origin().clone(),
        selected_version: asset.selected_version().clone(),
        media_type: asset.selected().blob.media_type.clone(),
    }
}

pub(crate) fn expand_target_template(
    template: &str,
    mod_id: &str,
    item_id: &str,
) -> Result<String, ResourcePrepareError> {
    if !valid_target_template(template) || !valid_segment(mod_id) || !valid_segment(item_id) {
        return Err(ResourcePrepareError::InvalidPackSpecs);
    }
    Ok(template
        .replace("{mod_id}", mod_id)
        .replace("{item_id}", item_id))
}

fn valid_target_template(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 512
        || value.starts_with('/')
        || value.contains('\\')
        || value
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return false;
    }
    let stripped = value
        .replace("{mod_id}", "value")
        .replace("{item_id}", "value");
    !stripped.contains('{') && !stripped.contains('}') && value.contains("{item_id}")
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
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

fn valid_media_type(value: &str) -> bool {
    value.split_once('/').is_some_and(|(kind, subtype)| {
        !kind.is_empty()
            && !subtype.is_empty()
            && value.len() <= 128
            && kind
                .bytes()
                .chain(subtype.bytes())
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
    })
}

fn resource_specs_slot() -> ContributionId {
    ContributionId::parse("resource.prepare.specs").expect("built-in contribution ID is valid")
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
    use ats_game_context::{ContributionResolver, GamePackLoader};
    use ats_kernel::PrimitiveId;
    use ats_runtime::{MediaRequestSnapshot, MediaResponse};
    use ats_workspace::{ResourceDeriveRequest, WorkspaceError};
    use sha2::{Digest, Sha256};

    use super::*;

    #[derive(Default)]
    struct MemoryRepository {
        assets: Mutex<BTreeMap<ResourceId, (ResourceAsset, Vec<u8>)>>,
    }

    impl ResourceRepository for MemoryRepository {
        type Error = WorkspaceError;

        fn ingest(&self, request: ResourceIngestRequest) -> Result<ResourceAsset, Self::Error> {
            let bytes =
                std::fs::read(request.source_path).map_err(|_| WorkspaceError::PathInvalid)?;
            self.ingest_bytes(ResourceBytesIngestRequest {
                logical_role: request.logical_role,
                origin: request.origin,
                media_type: request.media_type,
                file_name: "fixture.bin".into(),
                bytes,
            })
        }

        fn ingest_bytes(
            &self,
            request: ResourceBytesIngestRequest,
        ) -> Result<ResourceAsset, Self::Error> {
            let digest =
                Sha256Digest::parse(format!("{:x}", Sha256::digest(&request.bytes))).unwrap();
            let id = ResourceId::parse(format!(
                "resource.fixture-{}",
                self.assets.lock().unwrap().len()
            ))
            .unwrap();
            let asset = ResourceAsset::new(
                id.clone(),
                request.logical_role,
                request.origin,
                ats_workspace::ResourceVersion {
                    id: digest.clone(),
                    parent_version: None,
                    blob: ats_workspace::ResourceBlob {
                        relative_path: format!("versions/{digest}/original.bin"),
                        media_type: request.media_type,
                        byte_length: request.bytes.len() as u64,
                        sha256: digest,
                    },
                    provenance: ats_workspace::ResourceVersionProvenance::Original,
                },
            )?;
            self.assets
                .lock()
                .unwrap()
                .insert(id, (asset.clone(), request.bytes));
            Ok(asset)
        }

        fn add_version(&self, _: ResourceDeriveRequest) -> Result<ResourceAsset, Self::Error> {
            Err(WorkspaceError::InvalidProvenance)
        }
        fn select(
            &self,
            resource_id: &ResourceId,
            version: &Sha256Digest,
        ) -> Result<ResourceAsset, Self::Error> {
            let mut assets = self.assets.lock().unwrap();
            let (asset, _) = assets
                .get_mut(resource_id)
                .ok_or(WorkspaceError::ResourceNotFound)?;
            asset.select(version)?;
            Ok(asset.clone())
        }
        fn load(&self, resource_id: &ResourceId) -> Result<ResourceAsset, Self::Error> {
            self.assets
                .lock()
                .unwrap()
                .get(resource_id)
                .map(|(asset, _)| asset.clone())
                .ok_or(WorkspaceError::ResourceNotFound)
        }
        fn read_selected_bytes(
            &self,
            resource_id: &ResourceId,
            selected_version: &Sha256Digest,
        ) -> Result<Vec<u8>, Self::Error> {
            self.assets
                .lock()
                .unwrap()
                .get(resource_id)
                .filter(|(asset, _)| asset.selected_version() == selected_version)
                .map(|(_, bytes)| bytes.clone())
                .ok_or(WorkspaceError::VersionNotFound)
        }
    }

    struct MockMedia;
    #[async_trait]
    impl MediaClient for MockMedia {
        async fn generate(
            &self,
            request: MediaRequestSnapshot,
            _: &CancellationToken,
        ) -> Result<MediaResponse, MediaError> {
            Ok(MediaResponse {
                provider: PrimitiveId::parse("image.fixture").unwrap(),
                model: "fixture".into(),
                media_type: request.request().media_type.clone(),
                bytes: b"generated".to_vec(),
            })
        }
    }

    fn context() -> (LoadedGamePack, VerifiedContributionSet) {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let contributions =
            ContributionResolver::new([PrimitiveId::parse("image.role-transform").unwrap()])
                .resolve(
                    &pack,
                    &ResourcePrepareFeature::id(),
                    &[ResourcePrepareFeature::contribution_requirement()],
                )
                .unwrap();
        (pack, contributions)
    }

    #[tokio::test]
    async fn all_origins_share_pack_validation_and_preserve_provenance() {
        let (pack, contributions) = context();
        let repository = MemoryRepository::default();
        let service = ResourcePrepareService;
        let temp = tempfile::TempDir::new().unwrap();
        let source = temp.path().join("fixture.png");
        std::fs::write(&source, b"upload").unwrap();
        let make_context = || ResourcePrepareContext {
            pack: &pack,
            contributions: &contributions,
        };
        let upload = service
            .prepare_file(
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.normal".into(),
                    media_type: "image/png".into(),
                    source: ResourcePrepareSource::UserUpload,
                },
                source.clone(),
                make_context(),
            )
            .unwrap();
        let default = service
            .prepare_file(
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.outline".into(),
                    media_type: "image/png".into(),
                    source: ResourcePrepareSource::PackDefault,
                },
                source,
                make_context(),
            )
            .unwrap();
        let ai = service
            .prepare_ai(
                &MockMedia,
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.big".into(),
                    media_type: "image/png".into(),
                    source: ResourcePrepareSource::AiGenerated {
                        prompt: "fixture".into(),
                        file_name: "fixture.png".into(),
                        model: None,
                    },
                },
                make_context(),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(matches!(upload.origin, ResourceOrigin::UserUpload));
        assert!(matches!(default.origin, ResourceOrigin::PackDefault { .. }));
        assert!(matches!(ai.origin, ResourceOrigin::AiGenerated { .. }));
        assert_eq!(repository.assets.lock().unwrap().len(), 3);
    }

    #[test]
    fn unsupported_role_fails_before_repository_mutation() {
        let (pack, contributions) = context();
        let repository = MemoryRepository::default();
        let result = ResourcePrepareService.prepare_file(
            &repository,
            ResourcePrepareRequest {
                logical_role: "unknown.role".into(),
                media_type: "image/png".into(),
                source: ResourcePrepareSource::UserUpload,
            },
            PathBuf::from("missing"),
            ResourcePrepareContext {
                pack: &pack,
                contributions: &contributions,
            },
        );
        assert!(matches!(
            result,
            Err(ResourcePrepareError::UnsupportedResource)
        ));
        assert!(repository.assets.lock().unwrap().is_empty());
    }
}
