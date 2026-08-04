use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::Duration;

use ats_adapters::{FileItemRepository, FileResourceRepository, FileRunRepository};
use ats_runtime::{
    CancellationReason, CancellationToken, RunFailure, RunId, RunRecord, RunRepository,
    RunRepositoryError, RunStatus, RunTransition,
};
use ats_workspace::{ProjectFolder, ProjectMeta};
use chrono::Utc;
use futures_util::FutureExt;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub const PROJECT_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct ActiveProject {
    inner: RwLock<Option<Arc<ProjectSession>>>,
    pub(crate) lifecycle: Mutex<()>,
}

impl ActiveProject {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Result<Option<Arc<ProjectSession>>, ()> {
        self.inner.read().map(|value| value.clone()).map_err(|_| ())
    }

    pub fn replace(&self, session: Option<Arc<ProjectSession>>) -> Result<(), ()> {
        *self.inner.write().map_err(|_| ())? = session;
        Ok(())
    }
}

pub struct ProjectSession {
    project_lock: StdMutex<Option<ProjectFolder>>,
    root: PathBuf,
    meta: ProjectMeta,
    repository: Arc<FileRunRepository>,
    item_repository: Arc<FileItemRepository>,
    resource_repository: Arc<FileResourceRepository>,
    closing: AtomicBool,
    tasks: Mutex<HashMap<RunId, TrackedRun>>,
    finished: Arc<Notify>,
}

struct TrackedRun {
    cancellation: CancellationToken,
    finished: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

#[derive(Debug)]
pub enum SubmitError {
    Closing,
    Repository,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DrainTimeout {
    pub blocked_runs: Vec<RunId>,
}

impl ProjectSession {
    pub fn open(project: ProjectFolder) -> Result<Arc<Self>, RunRepositoryError> {
        let repository = Arc::new(FileRunRepository::new(project.run_history_dir())?);
        repository.reconcile_interrupted()?;
        let item_repository = Arc::new(FileItemRepository::new(project.path().to_path_buf()));
        let resource_repository =
            Arc::new(FileResourceRepository::new(project.path().to_path_buf()));
        Ok(Arc::new(Self {
            root: project.path().to_path_buf(),
            meta: project.meta().clone(),
            project_lock: StdMutex::new(Some(project)),
            repository,
            item_repository,
            resource_repository,
            closing: AtomicBool::new(false),
            tasks: Mutex::new(HashMap::new()),
            finished: Arc::new(Notify::new()),
        }))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn meta(&self) -> &ProjectMeta {
        &self.meta
    }

    #[must_use]
    pub fn repository(&self) -> Arc<dyn RunRepository> {
        Arc::clone(&self.repository) as Arc<dyn RunRepository>
    }

    #[must_use]
    pub fn item_repository(&self) -> Arc<FileItemRepository> {
        Arc::clone(&self.item_repository)
    }

    #[must_use]
    pub fn resource_repository(&self) -> Arc<FileResourceRepository> {
        Arc::clone(&self.resource_repository)
    }

    pub fn release_project_lock(&self) -> Result<(), ()> {
        self.project_lock.lock().map_err(|_| ())?.take();
        Ok(())
    }

    #[must_use]
    pub fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Acquire)
    }

    pub async fn submit<F, Fut>(&self, mut run: RunRecord, worker: F) -> Result<RunId, SubmitError>
    where
        F: FnOnce(RunRecord, CancellationToken, Arc<dyn RunRepository>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<RunRecord, RunFailure>> + Send + 'static,
    {
        let mut tasks = self.tasks.lock().await;
        if self.is_closing() {
            return Err(SubmitError::Closing);
        }
        self.repository
            .create(&run)
            .map_err(|_| SubmitError::Repository)?;
        run.apply_transition(RunTransition::Start, Utc::now())
            .map_err(|_| SubmitError::Repository)?;
        self.repository
            .persist(&run, RunStatus::Pending)
            .map_err(|_| SubmitError::Repository)?;

        let id = run.id().clone();
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let repository = self.repository();
        let supervisor_repository = Arc::clone(&repository);
        let finished_flag = Arc::new(AtomicBool::new(false));
        let supervisor_flag = Arc::clone(&finished_flag);
        let finished = Arc::clone(&self.finished);
        let task = tokio::spawn(async move {
            let outcome = std::panic::AssertUnwindSafe(worker(
                run.clone(),
                worker_cancellation.clone(),
                Arc::clone(&repository),
            ))
            .catch_unwind()
            .await;
            let terminal = match outcome {
                Ok(Ok(record)) if record.status().is_terminal() => Some(record),
                Ok(Ok(mut record)) => {
                    let failure = RunFailure::new(
                        ats_kernel::FailureCode::parse("run.incomplete")
                            .expect("built-in failure code is valid"),
                        "run.supervisor",
                        None,
                    )
                    .expect("built-in failure is valid");
                    let _ = record.apply_transition(RunTransition::Fail { failure }, Utc::now());
                    Some(record)
                }
                Ok(Err(failure)) => {
                    let mut record = run;
                    let transition = worker_cancellation.reason().map_or_else(
                        || RunTransition::Fail { failure },
                        |reason| RunTransition::Cancel { reason },
                    );
                    let _ = record.apply_transition(transition, Utc::now());
                    Some(record)
                }
                Err(_) => {
                    let mut record = run;
                    let failure = RunFailure::new(
                        ats_kernel::FailureCode::parse("run.task_panic")
                            .expect("built-in failure code is valid"),
                        "run.supervisor",
                        None,
                    )
                    .expect("built-in failure is valid");
                    let _ = record.apply_transition(RunTransition::Fail { failure }, Utc::now());
                    Some(record)
                }
            };
            if let Some(record) = terminal {
                let _ = supervisor_repository.persist(&record, RunStatus::Running);
            }
            supervisor_flag.store(true, Ordering::Release);
            finished.notify_one();
        });
        tasks.insert(
            id.clone(),
            TrackedRun {
                cancellation,
                finished: finished_flag,
                task,
            },
        );
        Ok(id)
    }

    pub async fn cancel_run(&self, id: &RunId, reason: CancellationReason) -> bool {
        self.tasks.lock().await.get(id).is_some_and(|tracked| {
            tracked.cancellation.cancel(reason) || tracked.cancellation.is_cancelled()
        })
    }

    pub async fn cancel_and_drain(
        &self,
        reason: CancellationReason,
        timeout: Duration,
    ) -> Result<(), DrainTimeout> {
        self.closing.store(true, Ordering::Release);
        {
            let tasks = self.tasks.lock().await;
            for tracked in tasks.values() {
                tracked.cancellation.cancel(reason);
            }
        }
        let deadline = Instant::now() + timeout;
        loop {
            let notified = self.finished.notified();
            let completed = {
                let mut tasks = self.tasks.lock().await;
                let ids = tasks
                    .iter()
                    .filter_map(|(id, tracked)| {
                        tracked
                            .finished
                            .load(Ordering::Acquire)
                            .then_some(id.clone())
                    })
                    .collect::<Vec<_>>();
                ids.into_iter()
                    .filter_map(|id| tasks.remove(&id).map(|tracked| tracked.task))
                    .collect::<Vec<_>>()
            };
            for task in completed {
                let _ = task.await;
            }
            if self.tasks.lock().await.is_empty() {
                return Ok(());
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                let mut blocked_runs = self.tasks.lock().await.keys().cloned().collect::<Vec<_>>();
                blocked_runs.sort_by(|left, right| left.as_str().cmp(right.as_str()));
                return Err(DrainTimeout { blocked_runs });
            }
        }
    }
}
