//! Compile-time identity shared by health, capabilities, and artifact provenance.

use serde::{Deserialize, Serialize};

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
    pub fn current() -> Self {
        let variant = match env!("ATS_EMBEDDED_BUILD_VARIANT") {
            "development" => BuildVariant::Development,
            "baseline" => BuildVariant::Baseline,
            "ml" => BuildVariant::Ml,
            _ => unreachable!("ats-core build.rs validates the embedded variant"),
        };
        let mut features = Vec::new();
        if cfg!(feature = "e2e") {
            features.push("e2e".into());
        }
        if cfg!(feature = "ml-rembg") {
            features.push("ml-rembg".into());
        }
        Self {
            commit: env!("ATS_EMBEDDED_BUILD_COMMIT").into(),
            variant,
            features,
            build_id: env!("ATS_EMBEDDED_BUILD_ID").into(),
        }
    }

    #[must_use]
    pub fn is_consistent(&self) -> bool {
        let valid_variant = match self.variant {
            BuildVariant::Development => true,
            BuildVariant::Baseline => {
                self.features.is_empty() && is_full_git_sha(&self.commit) && self.build_id != "dev"
            }
            BuildVariant::Ml => {
                self.features == ["ml-rembg"]
                    && is_full_git_sha(&self.commit)
                    && self.build_id != "dev"
            }
        };
        valid_variant
            && self.features.len() <= 16
            && is_bounded_identifier(&self.commit, 64)
            && is_bounded_identifier(&self.build_id, 96)
            && self
                .features
                .iter()
                .all(|feature| is_bounded_identifier(feature, 64))
    }
}

fn is_full_git_sha(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self::current()
    }
}

fn is_bounded_identifier(value: &str, max_len: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_build_info_matches_the_compiled_feature_set() {
        let info = BuildInfo::current();
        assert!(info.is_consistent());
        assert_eq!(
            info.features.iter().any(|feature| feature == "ml-rembg"),
            cfg!(feature = "ml-rembg")
        );
    }

    #[test]
    fn inconsistent_candidate_identity_is_rejected() {
        let info = BuildInfo {
            commit: "a".repeat(40),
            variant: BuildVariant::Baseline,
            features: vec!["ml-rembg".into()],
            build_id: "candidate-1".into(),
        };
        assert!(!info.is_consistent());
    }
}
