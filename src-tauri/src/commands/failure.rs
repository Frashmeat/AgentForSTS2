use ats_kernel::{ActionableFailure, FailureCode, RecoveryAction};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct CommandFailure(pub Box<ActionableFailure>);

pub type CommandResult<T> = Result<T, CommandFailure>;

impl CommandFailure {
    pub fn invalid_input(stage: &str) -> Self {
        fixed(
            "run.input_invalid",
            "input",
            stage,
            "The request did not match the registered Feature contract.",
            RecoveryAction::Retry,
            false,
        )
    }

    pub fn project_not_open(stage: &str) -> Self {
        fixed(
            "project.not_open",
            "project",
            stage,
            "Open a project before running this operation.",
            RecoveryAction::OpenProject,
            false,
        )
    }

    pub fn project_closing(stage: &str) -> Self {
        fixed(
            "project.closing",
            "project",
            stage,
            "The project is closing and cannot accept new work.",
            RecoveryAction::Retry,
            true,
        )
    }

    pub fn project_locked(stage: &str) -> Self {
        fixed(
            "project.locked",
            "project",
            stage,
            "The project is already open in another session.",
            RecoveryAction::Retry,
            true,
        )
    }

    pub fn project_local_environment(stage: &str) -> Self {
        fixed(
            "project.local_environment_invalid",
            "configuration",
            stage,
            "Configure valid game assembly and Godot executable paths before generating or building.",
            RecoveryAction::Retry,
            false,
        )
    }

    pub fn model_configuration(stage: &str) -> Self {
        fixed(
            "model.configuration",
            "configuration",
            stage,
            "Configure a valid model output budget before starting model work.",
            RecoveryAction::CheckSettings,
            false,
        )
    }

    pub fn truth_missing(stage: &str) -> Self {
        fixed(
            "truth.missing",
            "truth",
            stage,
            "Verified game evidence is not available.",
            RecoveryAction::RefreshTruth,
            false,
        )
    }

    pub fn truth_evidence_missing(stage: &str) -> Self {
        fixed(
            "truth.evidence_missing",
            "truth",
            stage,
            "The selected item type is not ready for the current game evidence.",
            RecoveryAction::RefreshTruth,
            false,
        )
    }

    pub fn item_invalid(stage: &str) -> Self {
        fixed(
            "item.definition_invalid",
            "input",
            stage,
            "The item definition does not match the selected Game Pack contract.",
            RecoveryAction::None,
            false,
        )
    }

    pub fn item_not_found(stage: &str) -> Self {
        fixed(
            "item.not_found",
            "state",
            stage,
            "The requested item definition was not found.",
            RecoveryAction::None,
            false,
        )
    }

    pub fn item_storage(stage: &str) -> Self {
        fixed(
            "item.storage_failed",
            "storage",
            stage,
            "The item library could not update its local state.",
            RecoveryAction::Retry,
            true,
        )
    }

    pub fn composition_invalid(stage: &str) -> Self {
        fixed(
            "composition.draft.invalid",
            "input",
            stage,
            "The composition Draft does not match the active Game Pack contract.",
            RecoveryAction::None,
            false,
        )
    }

    pub fn composition_not_found(stage: &str) -> Self {
        fixed(
            "composition.draft.not_found",
            "state",
            stage,
            "The requested composition Draft was not found.",
            RecoveryAction::None,
            false,
        )
    }

    pub fn composition_conflict(stage: &str) -> Self {
        fixed(
            "composition.draft.conflict",
            "state",
            stage,
            "The composition Draft changed. Refresh it before retrying.",
            RecoveryAction::Retry,
            true,
        )
    }

    pub fn composition_not_ready(stage: &str) -> Self {
        fixed(
            "composition.confirm.not_ready",
            "validation",
            stage,
            "Prepare and bind every required resource before confirming the composition Draft.",
            RecoveryAction::ReplaceResource,
            false,
        )
    }

    pub fn composition_storage(stage: &str) -> Self {
        fixed(
            "composition.draft.storage_failed",
            "storage",
            stage,
            "The composition Draft repository could not update its local state.",
            RecoveryAction::Retry,
            true,
        )
    }

    pub fn composition_adjustment_invalid(stage: &str) -> Self {
        fixed(
            "composition.adjustment.invalid",
            "input",
            stage,
            "The selected result cannot accept this item adjustment.",
            RecoveryAction::None,
            false,
        )
    }

    pub fn composition_adjustment_stale(stage: &str) -> Self {
        fixed(
            "composition.adjustment.stale",
            "state",
            stage,
            "The selected item changed. Refresh the result before adjusting it.",
            RecoveryAction::Retry,
            false,
        )
    }

    pub fn composition_adjustment_requires_replan(stage: &str) -> Self {
        fixed(
            "composition.adjustment.requires_replan",
            "state",
            stage,
            "This change requires planning the composition again.",
            RecoveryAction::Retry,
            false,
        )
    }

    pub fn composition_feedback_input_unavailable(stage: &str) -> Self {
        fixed(
            "composition.feedback.input_unavailable",
            "input",
            stage,
            "Re-submit feedback from the succeeded source result; the original feedback text is not persisted.",
            RecoveryAction::Retry,
            false,
        )
    }

    pub fn composition_execution_invalid(stage: &str) -> Self {
        fixed(
            "composition.execution.invalid",
            "state",
            stage,
            "The saved generation state does not match the current composition contract.",
            RecoveryAction::InspectRun,
            false,
        )
    }

    pub fn pack_invalid(stage: &str) -> Self {
        fixed(
            "pack.contribution_invalid",
            "pack",
            stage,
            "The active Game Pack resource contract is invalid.",
            RecoveryAction::None,
            false,
        )
    }

    pub fn resource_invalid(stage: &str) -> Self {
        fixed(
            "resource.selection_invalid",
            "input",
            stage,
            "The selected resource does not match the active Game Pack contract.",
            RecoveryAction::ReplaceResource,
            false,
        )
    }

    pub fn resource_media_invalid(stage: &str) -> Self {
        fixed(
            "resource.media_invalid",
            "input",
            stage,
            "The resource media does not match the required role shape.",
            RecoveryAction::ReplaceResource,
            false,
        )
    }

    pub fn resource_storage(stage: &str) -> Self {
        fixed(
            "resource.storage_failed",
            "storage",
            stage,
            "The resource workspace could not update its local state.",
            RecoveryAction::Retry,
            true,
        )
    }

    pub fn storage(stage: &str) -> Self {
        fixed(
            "run.storage_failed",
            "storage",
            stage,
            "The operation could not update its local state.",
            RecoveryAction::InspectRun,
            true,
        )
    }

    pub fn unclassified(stage: &str) -> Self {
        ActionableFailure::unclassified(stage).into()
    }
}

impl From<ActionableFailure> for CommandFailure {
    fn from(value: ActionableFailure) -> Self {
        Self(Box::new(value))
    }
}

fn fixed(
    code: &str,
    category: &str,
    stage: &str,
    message: &str,
    action: RecoveryAction,
    retryable: bool,
) -> CommandFailure {
    ActionableFailure::new(
        FailureCode::parse(code).expect("built-in failure code is valid"),
        category,
        stage,
        message,
        action,
        retryable,
    )
    .expect("built-in actionable failure is valid")
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_failure_is_the_kernel_shape_without_unknown_text() {
        let value = serde_json::to_value(CommandFailure::storage("run.list")).unwrap();
        assert_eq!(value["code"], "run.storage_failed");
        assert_eq!(value["stage"], "run.list");
        assert!(value.get("diagnostic").is_none());
        assert!(!value.to_string().contains("C:\\Users"));
    }

    #[test]
    fn invalid_execution_state_has_a_stable_safe_failure() {
        let value = serde_json::to_value(CommandFailure::composition_execution_invalid(
            "execution.get",
        ))
        .unwrap();
        assert_eq!(value["code"], "composition.execution.invalid");
        assert_eq!(value["category"], "state");
        assert_eq!(value["stage"], "execution.get");
        assert_eq!(value["retryable"], false);
        assert!(value.get("diagnostic").is_none());
    }

    #[test]
    fn invalid_project_local_environment_has_one_stable_shell_contract() {
        let failure = CommandFailure::project_local_environment("project.local_props");
        assert_eq!(failure.0.code.as_str(), "project.local_environment_invalid");
        assert_eq!(failure.0.category, "configuration");
        assert_eq!(failure.0.stage, "project.local_props");
        assert_eq!(failure.0.action, RecoveryAction::Retry);
        assert!(!failure.0.retryable);
    }
}
