use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use ats_kernel::{FeatureId, Sha256Digest};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::{RunId, VersionedPayload};

pub const ARTIFACT_MANIFEST_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ArtifactContractError {
    #[error("artifact manifest schema version is unsupported")]
    UnsupportedSchema,
    #[error("artifact identifier or kind is unsafe")]
    UnsafeIdentifier,
    #[error("artifact path is not a normalized relative path")]
    UnsafeRelativePath,
    #[error("artifact manifest must contain at least one file")]
    EmptyFiles,
    #[error("artifact manifest contains duplicate snapshot paths")]
    DuplicateFile,
    #[error("artifact file role is invalid")]
    InvalidRole,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub artifact_id: String,
    pub artifact_kind: String,
    pub feature_id: FeatureId,
    pub producing_run_id: RunId,
    pub created_at: DateTime<Utc>,
    pub contexts: Vec<VersionedPayload>,
    pub provenance: Vec<VersionedPayload>,
    pub feature_extension: VersionedPayload,
    pub files: Vec<ArtifactFileRecord>,
}

impl ArtifactManifest {
    pub fn validate(&self) -> Result<(), ArtifactContractError> {
        if self.schema_version != ARTIFACT_MANIFEST_SCHEMA_VERSION {
            return Err(ArtifactContractError::UnsupportedSchema);
        }
        validate_safe_segment(&self.artifact_id)?;
        validate_safe_segment(&self.artifact_kind)?;
        if self.files.is_empty() {
            return Err(ArtifactContractError::EmptyFiles);
        }

        let mut paths = BTreeSet::new();
        for file in &self.files {
            if file.role.is_empty()
                || file.role.len() > 128
                || !file.role.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'_' | b'-')
                })
            {
                return Err(ArtifactContractError::InvalidRole);
            }
            let snapshot = normalize_relative_path(Path::new(&file.snapshot_relative_path))?;
            if snapshot != file.snapshot_relative_path || !snapshot.starts_with("files/") {
                return Err(ArtifactContractError::UnsafeRelativePath);
            }
            if !paths.insert(snapshot) {
                return Err(ArtifactContractError::DuplicateFile);
            }
            if let Some(path) = &file.published_relative_path
                && normalize_relative_path(Path::new(path))? != *path
            {
                return Err(ArtifactContractError::UnsafeRelativePath);
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for ArtifactManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            artifact_id: String,
            artifact_kind: String,
            feature_id: FeatureId,
            producing_run_id: RunId,
            created_at: DateTime<Utc>,
            contexts: Vec<VersionedPayload>,
            provenance: Vec<VersionedPayload>,
            feature_extension: VersionedPayload,
            files: Vec<ArtifactFileRecord>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let manifest = Self {
            schema_version: wire.schema_version,
            artifact_id: wire.artifact_id,
            artifact_kind: wire.artifact_kind,
            feature_id: wire.feature_id,
            producing_run_id: wire.producing_run_id,
            created_at: wire.created_at,
            contexts: wire.contexts,
            provenance: wire.provenance,
            feature_extension: wire.feature_extension,
            files: wire.files,
        };
        manifest.validate().map_err(serde::de::Error::custom)?;
        Ok(manifest)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactFileRecord {
    pub role: String,
    pub snapshot_relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_relative_path: Option<String>,
    pub byte_length: u64,
    pub sha256: Sha256Digest,
}

#[derive(Debug, Clone)]
pub struct ArtifactFileInput {
    pub role: String,
    pub source_path: PathBuf,
    pub published_relative_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArtifactPublishRequest {
    pub artifact_id: String,
    pub artifact_kind: String,
    pub feature_id: FeatureId,
    pub producing_run_id: RunId,
    pub contexts: Vec<VersionedPayload>,
    pub provenance: Vec<VersionedPayload>,
    pub feature_extension: VersionedPayload,
    pub files: Vec<ArtifactFileInput>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublishedArtifact {
    pub artifact_manifest_ref: String,
    pub manifest_sha256: Sha256Digest,
}

pub trait ArtifactPublisher: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn publish(&self, request: ArtifactPublishRequest) -> Result<PublishedArtifact, Self::Error>;
    fn remove_published_run(&self, artifact_id: &str, run_id: &RunId) -> Result<(), Self::Error>;
}

pub fn normalize_relative_path(path: &Path) -> Result<String, ArtifactContractError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(ArtifactContractError::UnsafeRelativePath);
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part
                    .to_str()
                    .ok_or(ArtifactContractError::UnsafeRelativePath)?;
                if part.is_empty() || part.contains('/') || part.contains('\\') {
                    return Err(ArtifactContractError::UnsafeRelativePath);
                }
                parts.push(part);
            }
            _ => return Err(ArtifactContractError::UnsafeRelativePath),
        }
    }
    if parts.is_empty() {
        return Err(ArtifactContractError::UnsafeRelativePath);
    }
    Ok(parts.join("/"))
}

fn validate_safe_segment(value: &str) -> Result<(), ArtifactContractError> {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(ArtifactContractError::UnsafeIdentifier)
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use ats_kernel::{SchemaId, SchemaRef, SchemaVersion};

    use super::*;

    #[derive(Serialize)]
    struct Fixture {
        value: String,
    }

    fn payload(id: &str) -> VersionedPayload {
        VersionedPayload::from_typed(
            SchemaRef {
                id: SchemaId::parse(id).unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
            &Fixture { value: "ok".into() },
        )
        .unwrap()
    }

    fn manifest() -> ArtifactManifest {
        ArtifactManifest {
            schema_version: ARTIFACT_MANIFEST_SCHEMA_VERSION,
            artifact_id: "Fixture".into(),
            artifact_kind: "code".into(),
            feature_id: FeatureId::parse("fixture.generate").unwrap(),
            producing_run_id: RunId::new(),
            created_at: Utc::now(),
            contexts: vec![payload("fixture.context")],
            provenance: vec![payload("fixture.provenance")],
            feature_extension: payload("fixture.artifact-extension"),
            files: vec![ArtifactFileRecord {
                role: "source".into(),
                snapshot_relative_path: "files/000-Fixture.cs".into(),
                published_relative_path: Some("Generated/Fixture.cs".into()),
                byte_length: 10,
                sha256: Sha256Digest::parse("a".repeat(64)).unwrap(),
            }],
        }
    }

    #[test]
    fn base_manifest_has_no_upper_layer_fields() {
        let manifest = manifest();
        manifest.validate().unwrap();
        let value = serde_json::to_value(&manifest).unwrap();

        assert!(value.get("contexts").is_some());
        assert!(value.get("provenance").is_some());
        assert!(value.get("featureExtension").is_some());
        for forbidden in ["gameContext", "evidence", "generation", "imageProcessing"] {
            assert!(value.get(forbidden).is_none(), "found {forbidden}");
        }
    }

    #[test]
    fn rejects_empty_duplicate_and_escaping_file_records() {
        let mut empty = manifest();
        empty.files.clear();
        assert_eq!(empty.validate(), Err(ArtifactContractError::EmptyFiles));

        let mut duplicate = manifest();
        duplicate.files.push(duplicate.files[0].clone());
        assert_eq!(
            duplicate.validate(),
            Err(ArtifactContractError::DuplicateFile)
        );

        let mut escaping = manifest();
        escaping.files[0].snapshot_relative_path = "../outside".into();
        assert_eq!(
            escaping.validate(),
            Err(ArtifactContractError::UnsafeRelativePath)
        );

        let mut non_normalized = manifest();
        non_normalized.files[0].snapshot_relative_path = "files\\Fixture.cs".into();
        assert_eq!(
            non_normalized.validate(),
            Err(ArtifactContractError::UnsafeRelativePath)
        );
    }

    #[test]
    fn deserialization_cannot_bypass_manifest_validation() {
        let mut value = serde_json::to_value(manifest()).unwrap();
        value["files"] = serde_json::json!([]);
        assert!(serde_json::from_value::<ArtifactManifest>(value).is_err());

        let mut value = serde_json::to_value(manifest()).unwrap();
        value["files"][0]["role"] = serde_json::json!("Bad Role");
        assert!(serde_json::from_value::<ArtifactManifest>(value).is_err());
    }
}
