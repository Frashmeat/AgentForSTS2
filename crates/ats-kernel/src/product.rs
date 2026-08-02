use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{FailureCode, GamePackId, Sha256Digest};

#[derive(Debug, Clone, Error, Eq, PartialEq)]
pub enum ProductContractError {
    #[error("actionable failure contract is invalid")]
    InvalidActionableFailure,
    #[error("project template contract is invalid")]
    InvalidProjectTemplate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BuildVariant {
    Development,
    Baseline,
    Ml,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildInfo {
    pub commit: String,
    pub variant: BuildVariant,
    pub features: Vec<String>,
    pub build_id: String,
}

impl BuildInfo {
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        let candidate = self.commit.len() == 40
            && self.commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            && self.build_id != "dev";
        let variant_ok = match self.variant {
            BuildVariant::Development => true,
            BuildVariant::Baseline => self.features.is_empty() && candidate,
            BuildVariant::Ml => self.features == ["ml-rembg"] && candidate,
        };
        variant_ok
            && valid_identifier(&self.commit, 64)
            && valid_identifier(&self.build_id, 96)
            && self.features.len() <= 16
            && self
                .features
                .iter()
                .all(|feature| valid_identifier(feature, 64))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryAction {
    Retry,
    CheckSettings,
    OpenProject,
    RefreshTruth,
    ReplaceResource,
    InspectRun,
    None,
}

#[derive(Debug, Clone, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionableFailure {
    pub schema_version: u32,
    pub code: FailureCode,
    pub category: String,
    pub stage: String,
    pub message: String,
    pub action: RecoveryAction,
    pub retryable: bool,
}

impl<'de> Deserialize<'de> for ActionableFailure {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            code: FailureCode,
            category: String,
            stage: String,
            message: String,
            action: RecoveryAction,
            retryable: bool,
        }

        let wire = Wire::deserialize(deserializer)?;
        if wire.schema_version != 1 {
            return Err(serde::de::Error::custom(
                ProductContractError::InvalidActionableFailure,
            ));
        }
        Self::new(
            wire.code,
            wire.category,
            wire.stage,
            wire.message,
            wire.action,
            wire.retryable,
        )
        .map_err(serde::de::Error::custom)
    }
}

impl ActionableFailure {
    pub fn new(
        code: FailureCode,
        category: impl Into<String>,
        stage: impl Into<String>,
        message: impl Into<String>,
        action: RecoveryAction,
        retryable: bool,
    ) -> Result<Self, ProductContractError> {
        let failure = Self {
            schema_version: 1,
            code,
            category: category.into(),
            stage: stage.into(),
            message: message.into(),
            action,
            retryable,
        };
        if failure.valid() {
            Ok(failure)
        } else {
            Err(ProductContractError::InvalidActionableFailure)
        }
    }

    #[must_use]
    pub fn unclassified(stage: &str) -> Self {
        Self::new(
            FailureCode::parse("core.unclassified").expect("built-in failure code is valid"),
            "internal",
            stage,
            "The operation failed without a recognized safe error.",
            RecoveryAction::InspectRun,
            false,
        )
        .expect("built-in failure is valid")
    }

    fn valid(&self) -> bool {
        self.schema_version == 1
            && valid_identifier(&self.category, 64)
            && valid_identifier(&self.stage, 128)
            && !self.message.trim().is_empty()
            && self.message.chars().count() <= 512
            && !self.message.contains('\0')
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectTemplateFile {
    pub relative_path: String,
    pub sha256: Sha256Digest,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectTemplateBundle {
    pub game_pack_id: GamePackId,
    pub template_id: String,
    pub placeholder: String,
    pub files: Vec<ProjectTemplateFile>,
}

impl ProjectTemplateBundle {
    pub fn validate(&self) -> Result<(), ProductContractError> {
        if !valid_identifier(&self.template_id, 128)
            || self.placeholder.is_empty()
            || self.placeholder.len() > 128
            || self.files.is_empty()
            || self.files.len() > 256
        {
            return Err(ProductContractError::InvalidProjectTemplate);
        }
        let mut paths = BTreeSet::new();
        for file in &self.files {
            let path = Path::new(&file.relative_path);
            if path.is_absolute()
                || file.relative_path.contains('\\')
                || file.relative_path.starts_with(".ats/")
                || path.components().any(|component| {
                    matches!(
                        component,
                        std::path::Component::ParentDir
                            | std::path::Component::RootDir
                            | std::path::Component::Prefix(_)
                    )
                })
                || !paths.insert(file.relative_path.as_str())
                || Sha256Digest::parse(format!("{:x}", Sha256::digest(&file.bytes)))
                    .expect("SHA-256 formatter is valid")
                    != file.sha256
            {
                return Err(ProductContractError::InvalidProjectTemplate);
            }
        }
        Ok(())
    }
}

fn valid_identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actionable_failure_round_trips_through_validated_deserialization() {
        let failure = ActionableFailure::new(
            FailureCode::parse("fixture.failed").unwrap(),
            "validation",
            "fixture.execute",
            "The fixture failed validation.",
            RecoveryAction::InspectRun,
            false,
        )
        .unwrap();
        let json = serde_json::to_string(&failure).unwrap();
        assert_eq!(
            serde_json::from_str::<ActionableFailure>(&json).unwrap(),
            failure
        );
    }

    #[test]
    fn actionable_failure_rejects_invalid_wire_contracts() {
        let valid = serde_json::json!({
            "schemaVersion": 1,
            "code": "fixture.failed",
            "category": "validation",
            "stage": "fixture.execute",
            "message": "The fixture failed validation.",
            "action": "inspect_run",
            "retryable": false
        });
        for (field, value) in [
            ("schemaVersion", serde_json::json!(2)),
            ("category", serde_json::json!("not safe")),
            ("stage", serde_json::json!("C:\\private")),
            ("message", serde_json::json!("")),
        ] {
            let mut malformed = valid.clone();
            malformed[field] = value;
            assert!(serde_json::from_value::<ActionableFailure>(malformed).is_err());
        }
    }
}
