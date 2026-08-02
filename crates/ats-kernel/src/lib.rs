//! Stable value objects shared by the Stage 2 responsibility domains.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

mod product;

pub use product::{
    ActionableFailure, BuildInfo, BuildVariant, ProductContractError, ProjectTemplateBundle,
    ProjectTemplateFile, RecoveryAction,
};

#[derive(Debug, Clone, Error, Eq, PartialEq)]
pub enum ContractValueError {
    #[error("invalid {kind}: expected at least two dot-separated lowercase ASCII segments")]
    InvalidQualifiedId { kind: &'static str },
    #[error("invalid {kind}: expected a lowercase ASCII slug")]
    InvalidSlugId { kind: &'static str },
    #[error("schema version must be greater than zero")]
    InvalidSchemaVersion,
    #[error("SHA-256 digest must contain exactly 64 hexadecimal characters")]
    InvalidSha256,
}

fn validate_qualified_id(value: &str) -> bool {
    let mut segments = value.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    let Some(second) = segments.next() else {
        return false;
    };

    valid_segment(first) && valid_segment(second) && segments.all(valid_segment)
}

fn valid_segment(segment: &str) -> bool {
    let mut bytes = segment.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        && bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

macro_rules! qualified_id {
    ($name:ident, $kind:literal) => {
        #[derive(Debug, Clone, Serialize, Eq, PartialEq, Ord, PartialOrd, Hash)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, ContractValueError> {
                let value = value.into();
                if validate_qualified_id(&value) {
                    Ok(Self(value))
                } else {
                    Err(ContractValueError::InvalidQualifiedId { kind: $kind })
                }
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = ContractValueError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = ContractValueError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::parse(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

qualified_id!(FeatureId, "feature ID");
qualified_id!(ContributionId, "contribution ID");
qualified_id!(PrimitiveId, "primitive ID");
qualified_id!(SchemaId, "schema ID");
qualified_id!(FailureCode, "failure code");
qualified_id!(ResourceId, "resource ID");
qualified_id!(RecipeId, "recipe ID");

#[derive(Debug, Clone, Serialize, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[serde(transparent)]
pub struct GamePackId(String);

impl GamePackId {
    pub fn parse(value: impl Into<String>) -> Result<Self, ContractValueError> {
        let value = value.into();
        if value.len() <= 64
            && value
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase())
            && value.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
        {
            Ok(Self(value))
        } else {
            Err(ContractValueError::InvalidSlugId {
                kind: "game pack ID",
            })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for GamePackId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for GamePackId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[serde(transparent)]
pub struct SchemaVersion(u32);

impl SchemaVersion {
    pub fn new(value: u32) -> Result<Self, ContractValueError> {
        Self::try_from(value)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for SchemaVersion {
    type Error = ContractValueError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 0 {
            Err(ContractValueError::InvalidSchemaVersion)
        } else {
            Ok(Self(value))
        }
    }
}

impl From<SchemaVersion> for u32 {
    fn from(value: SchemaVersion) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        Self::try_from(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[serde(transparent)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(value: impl Into<String>) -> Result<Self, ContractValueError> {
        let value = value.into();
        if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            Ok(Self(value.to_ascii_lowercase()))
        } else {
            Err(ContractValueError::InvalidSha256)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Sha256Digest {
    type Error = ContractValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<&str> for Sha256Digest {
    type Error = ContractValueError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<Sha256Digest> for String {
    fn from(value: Sha256Digest) -> Self {
        value.0
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Sha256Digest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaRef {
    pub id: SchemaId,
    pub version: SchemaVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaEnvelope<T> {
    pub schema: SchemaRef,
    pub payload: T,
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct FixturePayload {
        value: String,
    }

    #[test]
    fn typed_envelope_round_trip_preserves_validated_identity() {
        let envelope = SchemaEnvelope {
            schema: SchemaRef {
                id: SchemaId::parse("feature.mod-plan-request").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
            payload: FixturePayload { value: "ok".into() },
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: SchemaEnvelope<FixturePayload> = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, envelope);
    }

    #[test]
    fn qualified_ids_reject_ambiguous_or_unsafe_values() {
        for value in [
            "single",
            "Mod.generate",
            "mod..generate",
            "mod/generate.single",
            "mod.generate.",
            "mod.1generate",
            " mod.generate",
        ] {
            assert!(FeatureId::parse(value).is_err(), "accepted {value:?}");
        }
        assert!(FeatureId::parse("mod.generate.single").is_ok());
        assert!(FailureCode::parse("artifact.publish_failed").is_ok());
        assert!(GamePackId::parse("sts2").is_ok());
        assert!(GamePackId::parse("fixture-game").is_ok());
        assert!(GamePackId::parse("Bad/Game").is_err());
    }

    #[test]
    fn deserialization_cannot_bypass_id_or_version_validation() {
        assert!(serde_json::from_str::<FeatureId>(r#""../feature""#).is_err());
        assert!(serde_json::from_str::<SchemaVersion>("0").is_err());
        assert!(serde_json::from_str::<SchemaRef>(r#"{"id":"run","version":1}"#).is_err());
    }

    #[test]
    fn sha256_is_validated_and_normalized() {
        let uppercase = "ABCDEF0123456789".repeat(4);
        let digest = Sha256Digest::parse(uppercase).unwrap();
        assert_eq!(digest.as_str(), "abcdef0123456789".repeat(4));

        assert!(Sha256Digest::parse("a".repeat(63)).is_err());
        assert!(Sha256Digest::parse(format!("{}g", "a".repeat(63))).is_err());
        assert!(serde_json::from_str::<Sha256Digest>(r#""not-a-digest""#).is_err());
    }
}
