//! Run repository contract.

use async_trait::async_trait;

use super::{RunId, RunProgress, RunRecord, RunRepositoryResult, RunSummary, RunTransition};

#[async_trait]
pub trait RunRepository: Send + Sync {
    async fn create(&self, run: &RunRecord) -> RunRepositoryResult<()>;

    async fn transition(
        &self,
        id: &RunId,
        transition: RunTransition,
    ) -> RunRepositoryResult<RunRecord>;

    async fn update_progress(
        &self,
        id: &RunId,
        progress: RunProgress,
    ) -> RunRepositoryResult<RunRecord>;

    async fn get(&self, id: &RunId) -> RunRepositoryResult<RunRecord>;

    async fn list(&self) -> RunRepositoryResult<Vec<RunSummary>>;

    async fn delete(&self, id: &RunId) -> RunRepositoryResult<()>;
}
