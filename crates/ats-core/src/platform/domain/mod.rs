//! Platform domain 层：实体 + repository trait + 错误。

mod errors;
mod models;
mod repository;

pub use errors::{JobError, JobResult};
pub use models::{Job, JobId, JobKind, JobProgress, JobStatus, JobSummary};
pub use repository::JobRepository;
