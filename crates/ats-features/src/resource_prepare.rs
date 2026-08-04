use std::collections::BTreeSet;
use std::path::PathBuf;

use ats_game_context::{ContributionResolverError, LoadedGamePack, VerifiedContributionSet};
use ats_kernel::{
    ContributionId, FeatureId, ResourceId, SchemaId, SchemaRef, SchemaVersion, Sha256Digest,
};
use ats_runtime::{CancellationToken, MediaClient, MediaError, MediaRequest, MediaRequestSnapshot};
use ats_workspace::{
    PreparedResourceMedia, ResourceAsset, ResourceBlob, ResourceBytesIngestRequest,
    ResourceMediaProcessor, ResourceOrigin, ResourceRepository, ResourceTransformOperation,
    ResourceVersionProvenance,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::FeatureSpec;

const MAX_PREVIEW_BYTES: usize = 8 * 1024 * 1024;

pub struct ResourcePrepareFeature;

impl FeatureSpec for ResourcePrepareFeature {
    type Request = ResourcePrepareRequest;
    type Result = ResourcePrepareResult;
    type ArtifactExtension = ResourcePrepareArtifactExtension;

    fn id() -> FeatureId {
        FeatureId::parse("resource.prepare").expect("built-in Feature ID is valid")
    }

    fn request_schema() -> SchemaRef {
        schema_version("feature.resource-prepare-request", 2)
    }

    fn result_schema() -> SchemaRef {
        schema_version("feature.resource-prepare-result", 2)
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
            schema: schema_version("pack.resource-specs", 2),
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    PackDefault,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePrepareResult {
    pub candidates: Vec<ResourceCandidateResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceCandidateResult {
    pub resource_id: ResourceId,
    pub logical_role: String,
    pub origin: ResourceOrigin,
    pub candidate_version: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_version: Option<Sha256Digest>,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceCatalog {
    pub game_pack_id: ats_kernel::GamePackId,
    pub game_pack_sha256: Sha256Digest,
    pub roles: Vec<ResourceRoleDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceRoleDescriptor {
    pub id: String,
    pub media_types: Vec<String>,
    pub width: u32,
    pub height: u32,
    pub require_alpha: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    pub source: ResourceRoleSourceDescriptor,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ResourceRoleSourceDescriptor {
    Master,
    Derived { source_role: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePreview {
    pub resource_id: ResourceId,
    pub logical_role: String,
    pub version: Sha256Digest,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourcePrepareArtifactExtension {
    pub candidate_count: u32,
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
    pub(crate) require_alpha: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) target_path: Option<String>,
    pub(crate) source: ResourceRoleSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum ResourceRoleSource {
    Master,
    Derived {
        source_role: String,
        transform: ResourceTransformSpec,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum ResourceTransformSpec {
    Resize {
        primitive: ats_kernel::PrimitiveId,
        version: u32,
    },
    Outline {
        primitive: ats_kernel::PrimitiveId,
        version: u32,
        radius: u32,
    },
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
                || !ids.insert(role.id.as_str())
            {
                return Err(ResourcePrepareError::InvalidPackSpecs);
            }
            match &role.source {
                ResourceRoleSource::Master => {
                    if role.target_path.is_some() {
                        return Err(ResourcePrepareError::InvalidPackSpecs);
                    }
                }
                ResourceRoleSource::Derived {
                    source_role,
                    transform,
                } => {
                    if !valid_role(source_role)
                        || source_role == &role.id
                        || role
                            .target_path
                            .as_deref()
                            .is_none_or(|path| !valid_target_template(path))
                        || !transform.validate()
                    {
                        return Err(ResourcePrepareError::InvalidPackSpecs);
                    }
                }
            }
        }
        for role in &self.roles {
            if let ResourceRoleSource::Derived { source_role, .. } = &role.source
                && !self.roles.iter().any(|candidate| {
                    candidate.id == *source_role
                        && matches!(candidate.source, ResourceRoleSource::Master)
                })
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

    pub(crate) fn validate_blob(
        &self,
        role: &str,
        blob: &ResourceBlob,
    ) -> Result<(), ResourcePrepareError> {
        let spec = self.require_role(role, &blob.media_type)?;
        if spec.accepts_media(&blob.media_type, blob.width, blob.height, blob.has_alpha) {
            Ok(())
        } else {
            Err(ResourcePrepareError::InvalidMedia)
        }
    }

    fn derived_from<'a>(
        &'a self,
        source_role: &'a str,
    ) -> impl Iterator<Item = &'a ResourceRoleSpec> {
        self.roles.iter().filter(move |role| {
            matches!(
                &role.source,
                ResourceRoleSource::Derived { source_role: source, .. } if source == source_role
            )
        })
    }
}

impl ResourceTransformSpec {
    fn validate(&self) -> bool {
        match self {
            Self::Resize { version, .. } => *version == 1,
            Self::Outline {
                version, radius, ..
            } => *version == 1 && (1..=64).contains(radius),
        }
    }

    fn primitive(&self) -> &ats_kernel::PrimitiveId {
        match self {
            Self::Resize { primitive, .. } | Self::Outline { primitive, .. } => primitive,
        }
    }

    fn version(&self) -> u32 {
        match self {
            Self::Resize { version, .. } | Self::Outline { version, .. } => *version,
        }
    }

    fn operation(&self, width: u32, height: u32) -> ResourceTransformOperation {
        match self {
            Self::Resize { .. } => ResourceTransformOperation::Resize { width, height },
            Self::Outline { radius, .. } => ResourceTransformOperation::Outline {
                width,
                height,
                radius: *radius,
            },
        }
    }
}

impl ResourceRoleSpec {
    fn accepts_media(&self, media_type: &str, width: u32, height: u32, has_alpha: bool) -> bool {
        self.media_types
            .iter()
            .any(|supported| supported == media_type)
            && width == self.width
            && height == self.height
            && (!self.require_alpha || has_alpha)
    }
}

pub struct ResourcePrepareContext<'a> {
    pub pack: &'a LoadedGamePack,
    pub contributions: &'a VerifiedContributionSet,
}

pub struct ResourcePrepareService;

impl ResourcePrepareService {
    pub fn catalog(
        &self,
        context: ResourcePrepareContext<'_>,
    ) -> Result<ResourceCatalog, ResourcePrepareError> {
        validate_context(&context)?;
        let specs = validate_specs_and_primitives(&context)?;
        Ok(ResourceCatalog {
            game_pack_id: context.pack.id().clone(),
            game_pack_sha256: context.pack.content_sha256().clone(),
            roles: specs
                .roles
                .into_iter()
                .map(|role| ResourceRoleDescriptor {
                    id: role.id,
                    media_types: role.media_types,
                    width: role.width,
                    height: role.height,
                    require_alpha: role.require_alpha,
                    target_path: role.target_path,
                    source: match role.source {
                        ResourceRoleSource::Master => ResourceRoleSourceDescriptor::Master,
                        ResourceRoleSource::Derived { source_role, .. } => {
                            ResourceRoleSourceDescriptor::Derived { source_role }
                        }
                    },
                })
                .collect(),
        })
    }

    pub fn list<R>(
        &self,
        repository: &R,
        context: ResourcePrepareContext<'_>,
    ) -> Result<Vec<ResourceAsset>, ResourcePrepareError>
    where
        R: ResourceRepository,
    {
        validate_context(&context)?;
        validate_specs_and_primitives(&context)?;
        repository
            .list()
            .map_err(|_| ResourcePrepareError::Repository)
    }

    pub fn preview<R>(
        &self,
        repository: &R,
        resource_id: &ResourceId,
        version: &Sha256Digest,
        context: ResourcePrepareContext<'_>,
    ) -> Result<ResourcePreview, ResourcePrepareError>
    where
        R: ResourceRepository,
    {
        validate_context(&context)?;
        let specs = validate_specs_and_primitives(&context)?;
        let asset = repository
            .load(resource_id)
            .map_err(|_| ResourcePrepareError::Repository)?;
        let candidate = asset
            .versions()
            .iter()
            .find(|candidate| &candidate.id == version)
            .ok_or(ResourcePrepareError::Repository)?;
        if !specs
            .roles
            .iter()
            .any(|role| role.id == asset.logical_role())
        {
            return Err(ResourcePrepareError::UnsupportedResource);
        }
        let bytes = repository
            .read_version_bytes(resource_id, version)
            .map_err(|_| ResourcePrepareError::Repository)?;
        if bytes.is_empty()
            || bytes.len() > MAX_PREVIEW_BYTES
            || u64::try_from(bytes.len()).ok() != Some(candidate.blob.byte_length)
            || sha256_bytes(&bytes) != candidate.blob.sha256
        {
            return Err(ResourcePrepareError::InvalidMedia);
        }
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        Ok(ResourcePreview {
            resource_id: resource_id.clone(),
            logical_role: asset.logical_role().to_owned(),
            version: version.clone(),
            media_type: candidate.blob.media_type.clone(),
            width: candidate.blob.width,
            height: candidate.blob.height,
            has_alpha: candidate.blob.has_alpha,
            data_url: format!("data:{};base64,{encoded}", candidate.blob.media_type),
        })
    }

    pub fn prepare_file<P, R>(
        &self,
        processor: &P,
        repository: &R,
        request: ResourcePrepareRequest,
        source_path: PathBuf,
        context: ResourcePrepareContext<'_>,
    ) -> Result<ResourcePrepareResult, ResourcePrepareError>
    where
        P: ResourceMediaProcessor,
        R: ResourceRepository,
    {
        let specs = validate_request_and_context(&request, &context)?;
        let spec = specs.require_role(&request.logical_role, &request.media_type)?;
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
        let media = processor
            .prepare_file(&source_path, &request.media_type)
            .map_err(|_| ResourcePrepareError::InvalidMedia)?;
        prepare_candidates(
            processor,
            repository,
            &specs,
            spec,
            media,
            origin,
            context.pack,
        )
    }

    pub async fn prepare_ai<M, P, R>(
        &self,
        media: &M,
        processor: &P,
        repository: &R,
        request: ResourcePrepareRequest,
        context: ResourcePrepareContext<'_>,
        cancellation: &CancellationToken,
    ) -> Result<ResourcePrepareResult, ResourcePrepareError>
    where
        M: MediaClient + ?Sized,
        P: ResourceMediaProcessor,
        R: ResourceRepository,
    {
        let specs = validate_request_and_context(&request, &context)?;
        let spec = specs.require_role(&request.logical_role, &request.media_type)?;
        let ResourcePrepareSource::AiGenerated { prompt, model } = request.source else {
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
        let origin = ResourceOrigin::AiGenerated {
            provider: response.provider,
            model: response.model,
            request_sha256: snapshot.request_sha256().clone(),
        };
        let prepared = processor
            .prepare_bytes(response.bytes, &request.media_type)
            .map_err(|_| ResourcePrepareError::InvalidMedia)?;
        prepare_candidates(
            processor,
            repository,
            &specs,
            spec,
            prepared,
            origin,
            context.pack,
        )
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
        let specs = validate_specs_and_primitives(&context)?;
        let current = repository
            .load(resource_id)
            .map_err(|_| ResourcePrepareError::Repository)?;
        let candidate = current
            .versions()
            .iter()
            .find(|candidate| &candidate.id == version)
            .ok_or(ResourcePrepareError::Repository)?;
        specs.validate_blob(current.logical_role(), &candidate.blob)?;
        let asset = repository
            .select(resource_id, version)
            .map_err(|_| ResourcePrepareError::Repository)?;
        Ok(ResourcePrepareResult {
            candidates: vec![result_from_asset(&asset, version)?],
        })
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
    #[error("resource media bytes or dimensions do not match the Pack contract")]
    InvalidMedia,
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
    let specs = validate_specs_and_primitives(context)?;
    Ok(specs)
}

fn validate_specs_and_primitives(
    context: &ResourcePrepareContext<'_>,
) -> Result<ResourceSpecs, ResourcePrepareError> {
    let specs: ResourceSpecs = context.contributions.decode(&resource_specs_slot())?;
    specs.validate()?;
    for role in &specs.roles {
        if let ResourceRoleSource::Derived { transform, .. } = &role.source
            && !context
                .contributions
                .declares_primitive(&resource_specs_slot(), transform.primitive())?
        {
            return Err(ResourcePrepareError::InvalidPackSpecs);
        }
    }
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

fn prepare_candidates<P, R>(
    processor: &P,
    repository: &R,
    specs: &ResourceSpecs,
    requested_spec: &ResourceRoleSpec,
    media: PreparedResourceMedia,
    origin: ResourceOrigin,
    pack: &LoadedGamePack,
) -> Result<ResourcePrepareResult, ResourcePrepareError>
where
    P: ResourceMediaProcessor,
    R: ResourceRepository,
{
    validate_media(requested_spec, &media)?;
    let source_version = sha256_bytes(&media.bytes);
    let mut requests = vec![ResourceBytesIngestRequest {
        logical_role: requested_spec.id.clone(),
        origin: origin.clone(),
        file_name: file_name_for_role(&requested_spec.id),
        media: media.clone(),
        provenance: ResourceVersionProvenance::Original,
    }];
    if matches!(requested_spec.source, ResourceRoleSource::Master) {
        for derived in specs.derived_from(&requested_spec.id) {
            let ResourceRoleSource::Derived { transform, .. } = &derived.source else {
                continue;
            };
            let operation = transform.operation(derived.width, derived.height);
            let transformed = processor
                .transform(&media, &operation)
                .map_err(|_| ResourcePrepareError::InvalidMedia)?;
            validate_media(derived, &transformed)?;
            let parameters = serde_json::to_vec(transform)
                .map_err(|_| ResourcePrepareError::InvalidPackSpecs)?;
            requests.push(ResourceBytesIngestRequest {
                logical_role: derived.id.clone(),
                origin: origin.clone(),
                file_name: file_name_for_role(&derived.id),
                media: transformed,
                provenance: ResourceVersionProvenance::Derived {
                    source_role: requested_spec.id.clone(),
                    source_version: source_version.clone(),
                    transform: transform.primitive().clone(),
                    transform_version: transform.version(),
                    parameters_sha256: sha256_bytes(&parameters),
                    game_pack_id: pack.id().clone(),
                    game_pack_sha256: pack.content_sha256().clone(),
                },
            });
        }
    }
    let assets = repository
        .ingest_batch(requests)
        .map_err(|_| ResourcePrepareError::Repository)?;
    let candidates = assets
        .iter()
        .map(|asset| {
            let version = asset
                .versions()
                .first()
                .ok_or(ResourcePrepareError::Repository)?;
            result_from_asset(asset, &version.id)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ResourcePrepareResult { candidates })
}

fn validate_media(
    spec: &ResourceRoleSpec,
    media: &PreparedResourceMedia,
) -> Result<(), ResourcePrepareError> {
    if !spec.accepts_media(
        &media.media_type,
        media.width,
        media.height,
        media.has_alpha,
    ) || media.bytes.is_empty()
    {
        Err(ResourcePrepareError::InvalidMedia)
    } else {
        Ok(())
    }
}

fn file_name_for_role(role: &str) -> String {
    format!("{}.png", role.replace('.', "-"))
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter is a valid digest")
}

fn result_from_asset(
    asset: &ResourceAsset,
    candidate_version: &Sha256Digest,
) -> Result<ResourceCandidateResult, ResourcePrepareError> {
    let candidate = asset
        .versions()
        .iter()
        .find(|version| &version.id == candidate_version)
        .ok_or(ResourcePrepareError::Repository)?;
    Ok(ResourceCandidateResult {
        resource_id: asset.resource_id().clone(),
        logical_role: asset.logical_role().to_owned(),
        origin: asset.origin().clone(),
        candidate_version: candidate.id.clone(),
        selected_version: asset.selected_version().cloned(),
        media_type: candidate.blob.media_type.clone(),
        width: candidate.blob.width,
        height: candidate.blob.height,
        has_alpha: candidate.blob.has_alpha,
    })
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
    schema_version(id, 1)
}

fn schema_version(id: &str, version: u32) -> SchemaRef {
    SchemaRef {
        id: SchemaId::parse(id).expect("built-in schema ID is valid"),
        version: SchemaVersion::new(version).expect("built-in schema version is valid"),
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

        fn ingest_bytes(
            &self,
            request: ResourceBytesIngestRequest,
        ) -> Result<ResourceAsset, Self::Error> {
            self.ingest_batch(vec![request])?
                .pop()
                .ok_or(WorkspaceError::InvalidManifest)
        }

        fn ingest_batch(
            &self,
            requests: Vec<ResourceBytesIngestRequest>,
        ) -> Result<Vec<ResourceAsset>, Self::Error> {
            let mut stored = self.assets.lock().unwrap();
            let mut pending = Vec::with_capacity(requests.len());
            for (offset, request) in requests.into_iter().enumerate() {
                let digest =
                    Sha256Digest::parse(format!("{:x}", Sha256::digest(&request.media.bytes)))
                        .unwrap();
                let id = ResourceId::parse(format!("resource.fixture-{}", stored.len() + offset))
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
                            media_type: request.media.media_type,
                            byte_length: request.media.bytes.len() as u64,
                            sha256: digest,
                            width: request.media.width,
                            height: request.media.height,
                            has_alpha: request.media.has_alpha,
                        },
                        provenance: request.provenance,
                    },
                )?;
                pending.push((id, asset, request.media.bytes));
            }
            let assets = pending.iter().map(|(_, asset, _)| asset.clone()).collect();
            for (id, asset, bytes) in pending {
                stored.insert(id, (asset, bytes));
            }
            Ok(assets)
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
                .filter(|(asset, _)| asset.selected_version() == Some(selected_version))
                .map(|(_, bytes)| bytes.clone())
                .ok_or(WorkspaceError::VersionNotFound)
        }
        fn read_version_bytes(
            &self,
            resource_id: &ResourceId,
            version: &Sha256Digest,
        ) -> Result<Vec<u8>, Self::Error> {
            let assets = self.assets.lock().unwrap();
            let (asset, bytes) = assets
                .get(resource_id)
                .ok_or(WorkspaceError::ResourceNotFound)?;
            if asset
                .versions()
                .iter()
                .any(|candidate| &candidate.id == version)
            {
                Ok(bytes.clone())
            } else {
                Err(WorkspaceError::VersionNotFound)
            }
        }

        fn list(&self) -> Result<Vec<ResourceAsset>, Self::Error> {
            Ok(self
                .assets
                .lock()
                .unwrap()
                .values()
                .map(|(asset, _)| asset.clone())
                .collect())
        }
    }

    struct MockProcessor;

    impl ResourceMediaProcessor for MockProcessor {
        type Error = WorkspaceError;

        fn prepare_file(
            &self,
            source_path: &std::path::Path,
            declared_media_type: &str,
        ) -> Result<PreparedResourceMedia, Self::Error> {
            let bytes = std::fs::read(source_path).map_err(|_| WorkspaceError::PathInvalid)?;
            self.prepare_bytes(bytes, declared_media_type)
        }

        fn prepare_bytes(
            &self,
            bytes: Vec<u8>,
            declared_media_type: &str,
        ) -> Result<PreparedResourceMedia, Self::Error> {
            Ok(PreparedResourceMedia {
                media_type: declared_media_type.into(),
                width: 512,
                height: 512,
                has_alpha: true,
                bytes,
            })
        }

        fn transform(
            &self,
            source: &PreparedResourceMedia,
            operation: &ResourceTransformOperation,
        ) -> Result<PreparedResourceMedia, Self::Error> {
            let (width, height) = match operation {
                ResourceTransformOperation::Resize { width, height }
                | ResourceTransformOperation::Outline { width, height, .. } => (*width, *height),
            };
            let mut bytes = source.bytes.clone();
            bytes.extend_from_slice(format!("{width}x{height}:{operation:?}").as_bytes());
            Ok(PreparedResourceMedia {
                media_type: "image/png".into(),
                width,
                height,
                has_alpha: true,
                bytes,
            })
        }
    }

    struct FixedProcessor {
        width: u32,
        height: u32,
        has_alpha: bool,
        bytes: Vec<u8>,
    }

    impl ResourceMediaProcessor for FixedProcessor {
        type Error = WorkspaceError;

        fn prepare_file(
            &self,
            _: &std::path::Path,
            declared_media_type: &str,
        ) -> Result<PreparedResourceMedia, Self::Error> {
            self.prepare_bytes(self.bytes.clone(), declared_media_type)
        }

        fn prepare_bytes(
            &self,
            bytes: Vec<u8>,
            declared_media_type: &str,
        ) -> Result<PreparedResourceMedia, Self::Error> {
            Ok(PreparedResourceMedia {
                media_type: declared_media_type.into(),
                width: self.width,
                height: self.height,
                has_alpha: self.has_alpha,
                bytes,
            })
        }

        fn transform(
            &self,
            _: &PreparedResourceMedia,
            _: &ResourceTransformOperation,
        ) -> Result<PreparedResourceMedia, Self::Error> {
            Err(WorkspaceError::InvalidManifest)
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
                &MockProcessor,
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.master".into(),
                    media_type: "image/png".into(),
                    source: ResourcePrepareSource::UserUpload,
                },
                source.clone(),
                make_context(),
            )
            .unwrap();
        let default = service
            .prepare_file(
                &MockProcessor,
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.master".into(),
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
                &MockProcessor,
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.master".into(),
                    media_type: "image/png".into(),
                    source: ResourcePrepareSource::AiGenerated {
                        prompt: "fixture".into(),
                        model: None,
                    },
                },
                make_context(),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(matches!(
            upload.candidates[0].origin,
            ResourceOrigin::UserUpload
        ));
        assert!(matches!(
            default.candidates[0].origin,
            ResourceOrigin::PackDefault { .. }
        ));
        assert!(matches!(
            ai.candidates[0].origin,
            ResourceOrigin::AiGenerated { .. }
        ));
        assert_eq!(upload.candidates.len(), 4);
        assert!(
            upload
                .candidates
                .iter()
                .all(|candidate| candidate.selected_version.is_none())
        );
        assert_eq!(repository.assets.lock().unwrap().len(), 12);
    }

    #[test]
    fn master_derivation_is_deterministic_selectable_and_records_exact_provenance() {
        let (pack, contributions) = context();
        let repository = MemoryRepository::default();
        let temp = tempfile::TempDir::new().unwrap();
        let source = temp.path().join("fixture.png");
        std::fs::write(&source, b"upload").unwrap();
        let prepare = || {
            ResourcePrepareService.prepare_file(
                &MockProcessor,
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.master".into(),
                    media_type: "image/png".into(),
                    source: ResourcePrepareSource::UserUpload,
                },
                source.clone(),
                ResourcePrepareContext {
                    pack: &pack,
                    contributions: &contributions,
                },
            )
        };
        let first = prepare().unwrap();
        let second = prepare().unwrap();
        assert_eq!(first.candidates.len(), 4);
        assert_eq!(
            first
                .candidates
                .iter()
                .map(|candidate| (&candidate.logical_role, &candidate.candidate_version))
                .collect::<BTreeMap<_, _>>(),
            second
                .candidates
                .iter()
                .map(|candidate| (&candidate.logical_role, &candidate.candidate_version))
                .collect::<BTreeMap<_, _>>()
        );
        assert!(
            first
                .candidates
                .iter()
                .all(|candidate| candidate.selected_version.is_none())
        );

        let master = first
            .candidates
            .iter()
            .find(|candidate| candidate.logical_role == "relic.master")
            .unwrap();
        let normal = first
            .candidates
            .iter()
            .find(|candidate| candidate.logical_role == "relic.normal")
            .unwrap();
        let normal_asset = repository.load(&normal.resource_id).unwrap();
        let ResourceVersionProvenance::Derived {
            source_role,
            source_version,
            transform,
            transform_version,
            parameters_sha256,
            game_pack_id,
            game_pack_sha256,
        } = &normal_asset.versions()[0].provenance
        else {
            panic!("normal candidate must retain derived provenance");
        };
        let specs: ResourceSpecs = contributions.decode(&resource_specs_slot()).unwrap();
        let ResourceRoleSource::Derived {
            transform: expected_transform,
            ..
        } = &specs
            .roles
            .iter()
            .find(|role| role.id == "relic.normal")
            .unwrap()
            .source
        else {
            panic!("normal role must be derived");
        };
        assert_eq!(source_role, "relic.master");
        assert_eq!(source_version, &master.candidate_version);
        assert_eq!(transform, expected_transform.primitive());
        assert_eq!(*transform_version, expected_transform.version());
        assert_eq!(
            parameters_sha256,
            &sha256_bytes(&serde_json::to_vec(expected_transform).unwrap())
        );
        assert_eq!(game_pack_id, pack.id());
        assert_eq!(game_pack_sha256, pack.content_sha256());

        let selected = ResourcePrepareService
            .select(
                &repository,
                &normal.resource_id,
                &normal.candidate_version,
                ResourcePrepareContext {
                    pack: &pack,
                    contributions: &contributions,
                },
            )
            .unwrap();
        assert_eq!(
            selected.candidates[0].selected_version.as_ref(),
            Some(&normal.candidate_version)
        );

        let catalog = ResourcePrepareService
            .catalog(ResourcePrepareContext {
                pack: &pack,
                contributions: &contributions,
            })
            .unwrap();
        assert_eq!(catalog.game_pack_sha256, *pack.content_sha256());
        assert_eq!(catalog.roles.len(), specs.roles.len());
        assert!(matches!(
            catalog
                .roles
                .iter()
                .find(|role| role.id == "relic.normal")
                .unwrap()
                .source,
            ResourceRoleSourceDescriptor::Derived { ref source_role }
                if source_role == "relic.master"
        ));
        assert_eq!(
            ResourcePrepareService
                .list(
                    &repository,
                    ResourcePrepareContext {
                        pack: &pack,
                        contributions: &contributions,
                    },
                )
                .unwrap()
                .len(),
            8
        );
        let preview = ResourcePrepareService
            .preview(
                &repository,
                &normal.resource_id,
                &normal.candidate_version,
                ResourcePrepareContext {
                    pack: &pack,
                    contributions: &contributions,
                },
            )
            .unwrap();
        assert!(preview.data_url.starts_with("data:image/png;base64,"));
        assert!(!preview.data_url.contains("versions/"));
    }

    #[test]
    fn invalid_dimensions_and_alpha_fail_before_repository_mutation() {
        let (pack, contributions) = context();
        for processor in [
            FixedProcessor {
                width: 511,
                height: 512,
                has_alpha: true,
                bytes: b"wrong-size".to_vec(),
            },
            FixedProcessor {
                width: 512,
                height: 512,
                has_alpha: false,
                bytes: b"no-alpha".to_vec(),
            },
            FixedProcessor {
                width: 512,
                height: 512,
                has_alpha: true,
                bytes: Vec::new(),
            },
        ] {
            let repository = MemoryRepository::default();
            let result = ResourcePrepareService.prepare_file(
                &processor,
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.master".into(),
                    media_type: "image/png".into(),
                    source: ResourcePrepareSource::UserUpload,
                },
                PathBuf::from("unused"),
                ResourcePrepareContext {
                    pack: &pack,
                    contributions: &contributions,
                },
            );
            assert!(matches!(result, Err(ResourcePrepareError::InvalidMedia)));
            assert!(repository.assets.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn pack_transform_version_and_primitive_drift_are_rejected() {
        for (field, value) in [
            ("version", serde_json::json!(2)),
            ("primitive", serde_json::json!("image.undeclared")),
        ] {
            let mut pack_value: serde_json::Value = serde_json::from_slice(include_bytes!(
                "../../../game_packs/sts2/stage2-game-pack.json"
            ))
            .unwrap();
            let contribution = pack_value["contributions"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|candidate| candidate["slotId"] == "resource.prepare.specs")
                .unwrap();
            let role = contribution["payload"]["roles"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|candidate| candidate["id"] == "relic.normal")
                .unwrap();
            role["source"]["transform"][field] = value;
            let bytes = serde_json::to_vec(&pack_value).unwrap();
            let digest = Sha256Digest::parse(format!("{:x}", Sha256::digest(&bytes))).unwrap();
            let pack = GamePackLoader::load(&bytes, &digest).unwrap();
            let contributions =
                ContributionResolver::new([PrimitiveId::parse("image.role-transform").unwrap()])
                    .resolve(
                        &pack,
                        &ResourcePrepareFeature::id(),
                        &[ResourcePrepareFeature::contribution_requirement()],
                    )
                    .unwrap();
            let repository = MemoryRepository::default();
            let result = ResourcePrepareService.prepare_file(
                &MockProcessor,
                &repository,
                ResourcePrepareRequest {
                    logical_role: "relic.master".into(),
                    media_type: "image/png".into(),
                    source: ResourcePrepareSource::UserUpload,
                },
                PathBuf::from("unused"),
                ResourcePrepareContext {
                    pack: &pack,
                    contributions: &contributions,
                },
            );
            assert!(matches!(
                result,
                Err(ResourcePrepareError::InvalidPackSpecs)
            ));
            assert!(repository.assets.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn unsupported_role_fails_before_repository_mutation() {
        let (pack, contributions) = context();
        let repository = MemoryRepository::default();
        let result = ResourcePrepareService.prepare_file(
            &MockProcessor,
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
