use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use ats_kernel::{ContributionId, GamePackId, PrimitiveId, ResourceId, Sha256Digest};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

pub const RESOURCE_ASSET_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceOrigin {
    UserUpload,
    AiGenerated {
        provider: PrimitiveId,
        model: String,
        request_sha256: Sha256Digest,
    },
    PackDefault {
        game_pack_id: GamePackId,
        game_pack_sha256: Sha256Digest,
        contribution_slot: ContributionId,
    },
}

impl ResourceOrigin {
    fn validate(&self) -> Result<(), WorkspaceError> {
        if let Self::AiGenerated { model, .. } = self
            && (model.trim().is_empty() || model.len() > 128 || model.chars().any(char::is_control))
        {
            return Err(WorkspaceError::InvalidProvenance);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ResourceIngestRequest {
    pub logical_role: String,
    pub origin: ResourceOrigin,
    pub media_type: String,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ResourceBytesIngestRequest {
    pub logical_role: String,
    pub origin: ResourceOrigin,
    pub media_type: String,
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ResourceDeriveRequest {
    pub resource_id: ResourceId,
    pub parent_version: Sha256Digest,
    pub transform: PrimitiveId,
    pub parameters_sha256: Sha256Digest,
    pub media_type: String,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceBlob {
    pub relative_path: String,
    pub media_type: String,
    pub byte_length: u64,
    pub sha256: Sha256Digest,
}

impl ResourceBlob {
    pub fn validate(&self) -> Result<(), WorkspaceError> {
        if normalize_relative_path(Path::new(&self.relative_path))? != self.relative_path
            || !self.relative_path.starts_with("versions/")
            || self.media_type.trim().is_empty()
            || self.media_type.len() > 128
            || self.media_type.chars().any(char::is_control)
            || self.byte_length == 0
        {
            return Err(WorkspaceError::InvalidManifest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceVersionProvenance {
    Original,
    Derived {
        transform: PrimitiveId,
        parameters_sha256: Sha256Digest,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceVersion {
    pub id: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_version: Option<Sha256Digest>,
    pub blob: ResourceBlob,
    pub provenance: ResourceVersionProvenance,
}

impl ResourceVersion {
    fn validate(&self) -> Result<(), WorkspaceError> {
        self.blob.validate()?;
        if self.id != self.blob.sha256
            || !self
                .blob
                .relative_path
                .starts_with(&format!("versions/{}/", self.id))
        {
            return Err(WorkspaceError::InvalidManifest);
        }
        match (&self.parent_version, &self.provenance) {
            (None, ResourceVersionProvenance::Original)
            | (Some(_), ResourceVersionProvenance::Derived { .. }) => Ok(()),
            _ => Err(WorkspaceError::InvalidProvenance),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAsset {
    schema_version: u32,
    resource_id: ResourceId,
    logical_role: String,
    origin: ResourceOrigin,
    original_version: Sha256Digest,
    selected_version: Sha256Digest,
    versions: Vec<ResourceVersion>,
}

impl ResourceAsset {
    pub fn new(
        resource_id: ResourceId,
        logical_role: String,
        origin: ResourceOrigin,
        original: ResourceVersion,
    ) -> Result<Self, WorkspaceError> {
        let asset = Self {
            schema_version: RESOURCE_ASSET_SCHEMA_VERSION,
            resource_id,
            logical_role,
            origin,
            original_version: original.id.clone(),
            selected_version: original.id.clone(),
            versions: vec![original],
        };
        asset.validate()?;
        Ok(asset)
    }

    pub fn add_version(&mut self, version: ResourceVersion) -> Result<bool, WorkspaceError> {
        version.validate()?;
        let parent = version
            .parent_version
            .as_ref()
            .ok_or(WorkspaceError::InvalidProvenance)?;
        if !self
            .versions
            .iter()
            .any(|candidate| &candidate.id == parent)
        {
            return Err(WorkspaceError::VersionNotFound);
        }
        if let Some(existing) = self
            .versions
            .iter()
            .find(|candidate| candidate.id == version.id)
        {
            return if existing == &version {
                Ok(false)
            } else {
                Err(WorkspaceError::ImmutableConflict)
            };
        }
        let mut next = self.clone();
        next.versions.push(version);
        next.versions.sort_by(|left, right| left.id.cmp(&right.id));
        next.validate()?;
        *self = next;
        Ok(true)
    }

    pub fn select(&mut self, version: &Sha256Digest) -> Result<(), WorkspaceError> {
        if !self
            .versions
            .iter()
            .any(|candidate| &candidate.id == version)
        {
            return Err(WorkspaceError::VersionNotFound);
        }
        self.selected_version = version.clone();
        self.validate()
    }

    pub fn validate(&self) -> Result<(), WorkspaceError> {
        if self.schema_version != RESOURCE_ASSET_SCHEMA_VERSION
            || !valid_role(&self.logical_role)
            || self.versions.is_empty()
        {
            return Err(WorkspaceError::InvalidManifest);
        }
        self.origin.validate()?;
        let mut ids = BTreeSet::new();
        for version in &self.versions {
            version.validate()?;
            if !ids.insert(&version.id) {
                return Err(WorkspaceError::InvalidManifest);
            }
        }
        let original = self
            .versions
            .iter()
            .find(|version| version.id == self.original_version)
            .ok_or(WorkspaceError::InvalidManifest)?;
        if original.parent_version.is_some()
            || original.provenance != ResourceVersionProvenance::Original
            || !self
                .versions
                .iter()
                .any(|version| version.id == self.selected_version)
        {
            return Err(WorkspaceError::InvalidManifest);
        }
        for version in &self.versions {
            if let Some(parent) = &version.parent_version
                && !ids.contains(parent)
            {
                return Err(WorkspaceError::InvalidManifest);
            }
            let mut cursor = version;
            let mut visited = BTreeSet::new();
            while let Some(parent) = &cursor.parent_version {
                if !visited.insert(&cursor.id) {
                    return Err(WorkspaceError::InvalidManifest);
                }
                cursor = self
                    .versions
                    .iter()
                    .find(|candidate| &candidate.id == parent)
                    .ok_or(WorkspaceError::InvalidManifest)?;
            }
            if cursor.id != self.original_version {
                return Err(WorkspaceError::InvalidManifest);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn resource_id(&self) -> &ResourceId {
        &self.resource_id
    }

    #[must_use]
    pub fn logical_role(&self) -> &str {
        &self.logical_role
    }

    #[must_use]
    pub fn origin(&self) -> &ResourceOrigin {
        &self.origin
    }

    #[must_use]
    pub fn selected_version(&self) -> &Sha256Digest {
        &self.selected_version
    }

    #[must_use]
    pub fn versions(&self) -> &[ResourceVersion] {
        &self.versions
    }

    #[must_use]
    pub fn selected(&self) -> &ResourceVersion {
        self.versions
            .iter()
            .find(|version| version.id == self.selected_version)
            .expect("validated ResourceAsset always contains selected version")
    }
}

impl<'de> Deserialize<'de> for ResourceAsset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            resource_id: ResourceId,
            logical_role: String,
            origin: ResourceOrigin,
            original_version: Sha256Digest,
            selected_version: Sha256Digest,
            versions: Vec<ResourceVersion>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let asset = Self {
            schema_version: wire.schema_version,
            resource_id: wire.resource_id,
            logical_role: wire.logical_role,
            origin: wire.origin,
            original_version: wire.original_version,
            selected_version: wire.selected_version,
            versions: wire.versions,
        };
        asset.validate().map_err(serde::de::Error::custom)?;
        Ok(asset)
    }
}

pub trait ResourceRepository: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn ingest(&self, request: ResourceIngestRequest) -> Result<ResourceAsset, Self::Error>;
    fn ingest_bytes(
        &self,
        request: ResourceBytesIngestRequest,
    ) -> Result<ResourceAsset, Self::Error>;
    fn add_version(&self, request: ResourceDeriveRequest) -> Result<ResourceAsset, Self::Error>;
    fn select(
        &self,
        resource_id: &ResourceId,
        version: &Sha256Digest,
    ) -> Result<ResourceAsset, Self::Error>;
    fn load(&self, resource_id: &ResourceId) -> Result<ResourceAsset, Self::Error>;
    fn read_selected_bytes(
        &self,
        resource_id: &ResourceId,
        selected_version: &Sha256Digest,
    ) -> Result<Vec<u8>, Self::Error>;
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum WorkspaceError {
    #[error("resource logical role is invalid")]
    InvalidRole,
    #[error("resource provenance is invalid")]
    InvalidProvenance,
    #[error("resource path is invalid")]
    PathInvalid,
    #[error("resource manifest is invalid")]
    InvalidManifest,
    #[error("resource does not exist")]
    ResourceNotFound,
    #[error("resource version does not exist")]
    VersionNotFound,
    #[error("resource immutable content conflicts with an existing version")]
    ImmutableConflict,
}

pub fn normalize_relative_path(path: &Path) -> Result<String, WorkspaceError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(WorkspaceError::PathInvalid);
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_str().ok_or(WorkspaceError::PathInvalid)?;
                if part.is_empty() || part.contains('/') || part.contains('\\') {
                    return Err(WorkspaceError::PathInvalid);
                }
                parts.push(part);
            }
            _ => return Err(WorkspaceError::PathInvalid),
        }
    }
    if parts.is_empty() {
        return Err(WorkspaceError::PathInvalid);
    }
    Ok(parts.join("/"))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(value: char) -> Sha256Digest {
        Sha256Digest::parse(value.to_string().repeat(64)).unwrap()
    }

    fn original() -> ResourceVersion {
        ResourceVersion {
            id: digest('a'),
            parent_version: None,
            blob: ResourceBlob {
                relative_path: format!("versions/{}/original.png", digest('a')),
                media_type: "image/png".into(),
                byte_length: 10,
                sha256: digest('a'),
            },
            provenance: ResourceVersionProvenance::Original,
        }
    }

    #[test]
    fn all_origins_share_the_same_validated_asset_contract() {
        let origins = [
            ResourceOrigin::UserUpload,
            ResourceOrigin::AiGenerated {
                provider: PrimitiveId::parse("image.openai-compatible").unwrap(),
                model: "fixture".into(),
                request_sha256: digest('b'),
            },
            ResourceOrigin::PackDefault {
                game_pack_id: GamePackId::parse("sts2").unwrap(),
                game_pack_sha256: digest('c'),
                contribution_slot: ContributionId::parse("resource.prepare.specs").unwrap(),
            },
        ];
        for (index, origin) in origins.into_iter().enumerate() {
            let asset = ResourceAsset::new(
                ResourceId::parse(format!("resource.fixture-{index}")).unwrap(),
                "relic.normal".into(),
                origin,
                original(),
            )
            .unwrap();
            let json = serde_json::to_string(&asset).unwrap();
            assert_eq!(serde_json::from_str::<ResourceAsset>(&json).unwrap(), asset);
        }
    }

    #[test]
    fn versions_are_immutable_and_selection_is_owned_by_the_asset() {
        let mut asset = ResourceAsset::new(
            ResourceId::parse("resource.fixture").unwrap(),
            "relic.normal".into(),
            ResourceOrigin::UserUpload,
            original(),
        )
        .unwrap();
        let derived = ResourceVersion {
            id: digest('d'),
            parent_version: Some(digest('a')),
            blob: ResourceBlob {
                relative_path: format!("versions/{}/derived.png", digest('d')),
                media_type: "image/png".into(),
                byte_length: 8,
                sha256: digest('d'),
            },
            provenance: ResourceVersionProvenance::Derived {
                transform: PrimitiveId::parse("image.outline").unwrap(),
                parameters_sha256: digest('e'),
            },
        };
        assert!(asset.add_version(derived.clone()).unwrap());
        assert!(!asset.add_version(derived).unwrap());
        asset.select(&digest('d')).unwrap();
        assert_eq!(asset.selected_version(), &digest('d'));
        let before = asset.clone();
        assert_eq!(
            asset.select(&digest('f')),
            Err(WorkspaceError::VersionNotFound)
        );
        assert_eq!(asset, before);
    }

    #[test]
    fn deserialization_rejects_tampered_selection_and_blob_identity() {
        let asset = ResourceAsset::new(
            ResourceId::parse("resource.fixture").unwrap(),
            "relic.normal".into(),
            ResourceOrigin::UserUpload,
            original(),
        )
        .unwrap();
        let mut value = serde_json::to_value(&asset).unwrap();
        value["selectedVersion"] = serde_json::json!(digest('f'));
        assert!(serde_json::from_value::<ResourceAsset>(value).is_err());

        let mut value = serde_json::to_value(asset).unwrap();
        value["versions"][0]["blob"]["sha256"] = serde_json::json!(digest('b'));
        assert!(serde_json::from_value::<ResourceAsset>(value).is_err());
    }
}
