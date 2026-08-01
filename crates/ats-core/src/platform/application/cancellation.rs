use std::ops::Deref;

use tokio::task::JoinHandle;

pub use crate::cancellation::CancellationToken;
use crate::platform::domain::RunId;

#[derive(Debug)]
pub struct SpawnedRun {
    pub run_id: RunId,
    pub cancellation: CancellationToken,
    pub task: JoinHandle<()>,
}

impl Deref for SpawnedRun {
    type Target = RunId;

    fn deref(&self) -> &Self::Target {
        &self.run_id
    }
}

impl PartialEq<RunId> for SpawnedRun {
    fn eq(&self, other: &RunId) -> bool {
        self.run_id == *other
    }
}

impl PartialEq<SpawnedRun> for RunId {
    fn eq(&self, other: &SpawnedRun) -> bool {
        *self == other.run_id
    }
}
