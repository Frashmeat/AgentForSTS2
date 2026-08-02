use async_trait::async_trait;
use ats_kernel::{PrimitiveId, Sha256Digest};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::CancellationToken;

pub const MEDIA_REQUEST_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaRequest {
    pub prompt: String,
    pub logical_role: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl MediaRequest {
    fn validate(&self) -> Result<(), MediaError> {
        if self.prompt.trim().is_empty()
            || self.prompt.chars().count() > 8_000
            || !valid_role(&self.logical_role)
            || !valid_media_type(&self.media_type)
            || self.width.is_some_and(|value| value == 0 || value > 16_384)
            || self
                .height
                .is_some_and(|value| value == 0 || value > 16_384)
            || self.width.is_some() != self.height.is_some()
            || self
                .model
                .as_ref()
                .is_some_and(|value| value.trim().is_empty() || value.len() > 256)
        {
            return Err(MediaError::InvalidRequest);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MediaRequestSnapshot {
    schema_version: u32,
    request: MediaRequest,
    request_sha256: Sha256Digest,
}

impl MediaRequestSnapshot {
    pub fn new(request: MediaRequest) -> Result<Self, MediaError> {
        request.validate()?;
        let mut snapshot = Self {
            schema_version: MEDIA_REQUEST_SNAPSHOT_SCHEMA_VERSION,
            request,
            request_sha256: zero_digest(),
        };
        snapshot.request_sha256 = snapshot.compute_identity()?;
        Ok(snapshot)
    }

    pub fn verify(&self) -> Result<(), MediaError> {
        self.request.validate()?;
        if self.schema_version != MEDIA_REQUEST_SNAPSHOT_SCHEMA_VERSION
            || self.compute_identity()? != self.request_sha256
        {
            return Err(MediaError::IdentityMismatch);
        }
        Ok(())
    }

    fn compute_identity(&self) -> Result<Sha256Digest, MediaError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Identity<'a> {
            schema_version: u32,
            request: &'a MediaRequest,
        }
        let bytes = serde_json::to_vec(&Identity {
            schema_version: self.schema_version,
            request: &self.request,
        })
        .map_err(|_| MediaError::InvalidRequest)?;
        Ok(sha256_bytes(&bytes))
    }

    #[must_use]
    pub fn request(&self) -> &MediaRequest {
        &self.request
    }

    #[must_use]
    pub fn request_sha256(&self) -> &Sha256Digest {
        &self.request_sha256
    }
}

impl<'de> Deserialize<'de> for MediaRequestSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            request: MediaRequest,
            request_sha256: Sha256Digest,
        }
        let wire = Wire::deserialize(deserializer)?;
        let snapshot = Self {
            schema_version: wire.schema_version,
            request: wire.request,
            request_sha256: wire.request_sha256,
        };
        snapshot.verify().map_err(serde::de::Error::custom)?;
        Ok(snapshot)
    }
}

#[derive(Debug, Clone)]
pub struct MediaResponse {
    pub provider: PrimitiveId,
    pub model: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

impl MediaResponse {
    pub fn validate(&self) -> Result<(), MediaError> {
        if self.model.trim().is_empty()
            || self.model.len() > 256
            || !valid_media_type(&self.media_type)
            || self.bytes.is_empty()
            || self.bytes.len() > 64 * 1024 * 1024
        {
            return Err(MediaError::InvalidResponse);
        }
        Ok(())
    }
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum MediaError {
    #[error("media request is invalid")]
    InvalidRequest,
    #[error("media request identity does not match its content")]
    IdentityMismatch,
    #[error("media authentication failed")]
    Authentication,
    #[error("media request was rate limited")]
    RateLimited { retry_after_ms: Option<u64> },
    #[error("media transport failed")]
    Transport,
    #[error("media provider rejected the request")]
    Rejected,
    #[error("media response is invalid")]
    InvalidResponse,
    #[error("media request was cancelled")]
    Cancelled,
    #[error("media client configuration is invalid")]
    Configuration,
}

#[async_trait]
pub trait MediaClient: Send + Sync {
    async fn generate(
        &self,
        request: MediaRequestSnapshot,
        cancellation: &CancellationToken,
    ) -> Result<MediaResponse, MediaError>;
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

fn zero_digest() -> Sha256Digest {
    Sha256Digest::parse("0".repeat(64)).expect("fixed placeholder digest is valid")
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter always returns a valid digest")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_snapshot_round_trips_and_detects_tampering() {
        let snapshot = MediaRequestSnapshot::new(MediaRequest {
            prompt: "fixture image".into(),
            logical_role: "relic.normal".into(),
            media_type: "image/png".into(),
            width: Some(256),
            height: Some(256),
            model: None,
        })
        .unwrap();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_str::<MediaRequestSnapshot>(&json)
                .unwrap()
                .request_sha256(),
            snapshot.request_sha256()
        );
        let mut value = serde_json::to_value(snapshot).unwrap();
        value["request"]["prompt"] = serde_json::json!("tampered");
        assert!(serde_json::from_value::<MediaRequestSnapshot>(value).is_err());
    }
}
