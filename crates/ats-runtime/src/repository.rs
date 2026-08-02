use thiserror::Error;

use crate::{RunId, RunRecord, RunStatus, RunSummary};

#[derive(Debug, Error)]
pub enum RunRepositoryError {
    #[error("Run already exists")]
    AlreadyExists,
    #[error("Run was not found")]
    NotFound,
    #[error("Run compare-and-swap conflict")]
    Conflict,
    #[error("Run record is invalid")]
    InvalidRecord,
    #[error("Run repository I/O failed")]
    Io(#[source] std::io::Error),
    #[error("Run repository JSON is invalid")]
    Json(#[source] serde_json::Error),
}

pub trait RunRepository: Send + Sync {
    fn create(&self, run: &RunRecord) -> Result<(), RunRepositoryError>;
    fn get(&self, id: &RunId) -> Result<RunRecord, RunRepositoryError>;
    fn list(&self) -> Result<Vec<RunSummary>, RunRepositoryError>;
    fn persist(
        &self,
        run: &RunRecord,
        expected_status: RunStatus,
    ) -> Result<(), RunRepositoryError>;
    fn reconcile_interrupted(&self) -> Result<u32, RunRepositoryError>;
}
