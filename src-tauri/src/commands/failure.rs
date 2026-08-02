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
}
