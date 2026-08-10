use thiserror::Error;

use ats_kernel::ExecutionGraphId;

use crate::{ExecutionGraphRecord, RunId, RunRecord, RunStatus, RunSummary};

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

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct ExecutionGraphRecovery {
    pub inspected: u32,
    pub recovered: u32,
}

#[derive(Debug, Error)]
pub enum ExecutionGraphRepositoryError {
    #[error("execution graph already exists")]
    AlreadyExists,
    #[error("execution graph was not found")]
    NotFound,
    #[error("execution graph compare-and-swap conflict")]
    Conflict,
    #[error("execution graph record is invalid")]
    InvalidRecord,
    #[error("execution graph repository I/O failed")]
    Io(#[source] std::io::Error),
    #[error("execution graph repository JSON is invalid")]
    Json(#[source] serde_json::Error),
}

pub trait ExecutionGraphRepository: Send + Sync {
    fn create_claimed(
        &self,
        graph: &ExecutionGraphRecord,
        run_id: &RunId,
    ) -> Result<(), ExecutionGraphRepositoryError>;
    fn get(
        &self,
        id: &ExecutionGraphId,
    ) -> Result<ExecutionGraphRecord, ExecutionGraphRepositoryError>;
    fn compare_and_set(
        &self,
        expected_revision: u64,
        next: &ExecutionGraphRecord,
    ) -> Result<(), ExecutionGraphRepositoryError>;
    fn list(&self) -> Result<Vec<ExecutionGraphRecord>, ExecutionGraphRepositoryError>;
    fn recover_structure(
        &self,
        runs: &dyn RunRepository,
    ) -> Result<ExecutionGraphRecovery, ExecutionGraphRepositoryError>;
}
