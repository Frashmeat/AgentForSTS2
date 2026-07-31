//! Platform domain 层：实体 + repository trait + 错误。

mod errors;
mod models;
mod repository;

pub use crate::failure::ActionableFailure;
pub use errors::{PackageError, RunError, RunRepositoryResult};
pub use models::{
    BatchArtifactItemResult, BuildStepResult, CancellationReason, RUN_SCHEMA_VERSION, RunId,
    RunKind, RunProgress, RunRecord, RunResult, RunStatus, RunSummary, RunTimelineEvent,
    RunTimelineEventKind, RunTransition, TokenUsage,
};
pub use repository::RunRepository;
