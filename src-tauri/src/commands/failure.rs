//! Structured Tauri reject payload backed by the Core failure contract.

use std::io;

use ats_core::failure::{
    ActionableFailure, FailureCategory, FailureContext, FailureNormalizer, RecoveryAction,
};
use ats_core::image_gen::ImageGenError;
use ats_core::llm::LlmError;
use ats_core::platform::domain::RunError;
use ats_core::project::{LocalPropsError, ProjectError};
use ats_core::toolchain::GodotValidationError;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct CommandFailure(pub Box<ActionableFailure>);

pub type CommandResult<T> = Result<T, CommandFailure>;

impl CommandFailure {
    pub fn unclassified(stage: &'static str) -> Self {
        ActionableFailure::unclassified(stage).into()
    }

    pub fn invalid_input(stage: &'static str, message: &'static str) -> Self {
        ActionableFailure::invalid_input(stage, message).into()
    }

    pub fn settings_path(stage: &'static str) -> Self {
        ActionableFailure::new(
            "settings.path_missing",
            FailureCategory::Configuration,
            stage,
            "The settings file path is unavailable.",
            RecoveryAction::OpenSettings,
            false,
        )
        .into()
    }

    pub fn llm(stage: &'static str, error: &LlmError) -> Self {
        FailureNormalizer::llm(stage, error).into()
    }

    pub fn image(stage: &'static str, error: &ImageGenError) -> Self {
        FailureNormalizer::image(stage, error).into()
    }

    pub fn project(stage: &'static str, error: &ProjectError) -> Self {
        FailureNormalizer::project(stage, error).into()
    }

    pub fn project_not_open(stage: &'static str) -> Self {
        FailureNormalizer::project_not_open(stage).into()
    }

    pub fn project_closing(stage: &'static str) -> Self {
        ActionableFailure::new(
            "project.closing",
            FailureCategory::State,
            stage,
            "The active project is closing. Wait for its runs to stop.",
            RecoveryAction::None,
            false,
        )
        .into()
    }

    pub fn project_close_timeout(stage: &'static str, blocked_run: Option<&str>) -> Self {
        ActionableFailure::new(
            "project.close_timeout",
            FailureCategory::State,
            stage,
            "The project could not close because a run is still stopping.",
            RecoveryAction::Retry,
            true,
        )
        .with_context(FailureContext {
            run_id: blocked_run.map(ToOwned::to_owned),
            ..FailureContext::default()
        })
        .into()
    }

    pub fn run(stage: &'static str, error: &RunError) -> Self {
        FailureNormalizer::run(stage, error).into()
    }

    pub fn toolchain(stage: &'static str, error: &GodotValidationError) -> Self {
        FailureNormalizer::toolchain(stage, error).into()
    }

    pub fn local_props(stage: &'static str, error: &LocalPropsError) -> Self {
        FailureNormalizer::local_props(stage, error).into()
    }

    pub fn io(
        code: &'static str,
        stage: &'static str,
        message: &'static str,
        error: &io::Error,
    ) -> Self {
        FailureNormalizer::io(code, stage, message, error).into()
    }
}

impl From<ActionableFailure> for CommandFailure {
    fn from(value: ActionableFailure) -> Self {
        Self(Box::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_failure_is_a_transparent_actionable_failure() {
        let failure = CommandFailure::unclassified("tauri.test");
        let serialized = serde_json::to_value(&failure).unwrap();
        assert_eq!(serialized["schemaVersion"], 1);
        assert_eq!(serialized["code"], "core.unclassified");
        assert!(serialized.get("failure").is_none());
    }

    #[test]
    fn known_provider_error_does_not_expose_body() {
        let canary = "provider body token=secret";
        let failure = CommandFailure::llm(
            "tauri.llm",
            &LlmError::Http {
                status: 401,
                message: canary.into(),
            },
        );
        let serialized = serde_json::to_string(&failure).unwrap();
        assert_eq!(failure.0.code, "llm.authentication_failed");
        assert!(!serialized.contains(canary));
        assert!(!serialized.contains("secret"));
    }
}
