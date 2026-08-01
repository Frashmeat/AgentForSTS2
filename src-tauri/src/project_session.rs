use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::Duration;

use ats_core::failure::ActionableFailure;
use ats_core::platform::domain::{
    CancellationReason, RunError, RunId, RunRepository, RunRepositoryResult, RunStatus,
    RunTransition,
};
use ats_core::platform::{FileRunRepository, SpawnedRun};
use ats_core::project::{ProjectFolder, ProjectMeta};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use tokio::time::Instant;

const SESSION_OPEN: u8 = 0;
const SESSION_CLOSING: u8 = 1;
pub(crate) const PROJECT_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

pub struct ActiveProject {
    current: RwLock<Option<Arc<ProjectSession>>>,
    pub lifecycle: Mutex<()>,
}

impl ActiveProject {
    #[must_use]
    pub fn new() -> Self {
        Self {
            current: RwLock::new(None),
            lifecycle: Mutex::new(()),
        }
    }

    pub fn current(&self) -> Result<Option<Arc<ProjectSession>>, ()> {
        self.current
            .read()
            .map(|current| current.clone())
            .map_err(|_| ())
    }

    pub fn require(&self) -> Result<Arc<ProjectSession>, ActiveProjectError> {
        self.current()
            .map_err(|_| ActiveProjectError::Poisoned)?
            .ok_or(ActiveProjectError::NotOpen)
    }

    pub fn replace(&self, session: Option<Arc<ProjectSession>>) -> Result<(), ()> {
        *self.current.write().map_err(|_| ())? = session;
        Ok(())
    }
}

impl Default for ActiveProject {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ActiveProjectError {
    NotOpen,
    Poisoned,
}

pub struct ProjectSession {
    project_lock: StdMutex<Option<ProjectFolder>>,
    project_root: PathBuf,
    project_meta: ProjectMeta,
    repository: Arc<FileRunRepository>,
    state: AtomicU8,
    tasks: Mutex<RunTaskScope>,
    task_finished: Arc<Notify>,
}

#[derive(Default)]
struct RunTaskScope {
    accepting: bool,
    tasks: HashMap<RunId, TrackedRun>,
}

struct TrackedRun {
    cancellation: ats_core::platform::CancellationToken,
    finished: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

#[derive(Debug)]
pub enum SubmitError {
    Closing,
    Run(RunError),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DrainTimeout {
    pub blocked_runs: Vec<RunId>,
}

impl ProjectSession {
    pub async fn open(project: ProjectFolder) -> RunRepositoryResult<Arc<Self>> {
        let repository = Arc::new(FileRunRepository::new(project.history_dir()));
        repository.reconcile_interrupted().await?;
        let project_root = project.path().to_path_buf();
        let project_meta = project.meta().clone();
        Ok(Arc::new(Self {
            project_lock: StdMutex::new(Some(project)),
            project_root,
            project_meta,
            repository,
            state: AtomicU8::new(SESSION_OPEN),
            tasks: Mutex::new(RunTaskScope {
                accepting: true,
                tasks: HashMap::new(),
            }),
            task_finished: Arc::new(Notify::new()),
        }))
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.project_root
    }

    #[must_use]
    pub fn meta(&self) -> &ProjectMeta {
        &self.project_meta
    }

    #[must_use]
    pub fn items_dir(&self) -> PathBuf {
        self.project_root.join("items")
    }

    #[must_use]
    pub fn artifacts_dir(&self) -> PathBuf {
        self.project_root.join("artifacts")
    }

    pub fn release_project_lock(&self) -> Result<(), ()> {
        self.project_lock.lock().map_err(|_| ())?.take();
        Ok(())
    }

    #[must_use]
    pub fn repository(&self) -> Arc<dyn RunRepository> {
        Arc::clone(&self.repository) as Arc<dyn RunRepository>
    }

    #[cfg(test)]
    #[must_use]
    pub fn file_repository(&self) -> Arc<FileRunRepository> {
        Arc::clone(&self.repository)
    }

    #[must_use]
    pub fn is_closing(&self) -> bool {
        self.state.load(Ordering::Acquire) == SESSION_CLOSING
    }

    /// Holds the scope lock while the lazy submission future creates and spawns
    /// the run, so close cannot pass the accepting gate between those actions.
    pub async fn submit<F>(&self, submission: F) -> Result<RunId, SubmitError>
    where
        F: Future<Output = RunRepositoryResult<SpawnedRun>>,
    {
        let mut scope = self.tasks.lock().await;
        if !scope.accepting || self.is_closing() {
            return Err(SubmitError::Closing);
        }
        let spawned = submission.await.map_err(SubmitError::Run)?;
        let run_id = spawned.run_id.clone();
        let cancellation = spawned.cancellation.clone();
        let repository = Arc::clone(&self.repository);
        let finished = Arc::clone(&self.task_finished);
        let task_complete = Arc::new(AtomicBool::new(false));
        let supervisor_complete = Arc::clone(&task_complete);
        let supervised_id = run_id.clone();
        let supervised_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            let join_result = spawned.task.await;
            if let Ok(run) = repository.get(&supervised_id).await
                && matches!(run.status, RunStatus::Pending | RunStatus::Running)
            {
                let transition = if let Some(reason) = supervised_cancellation.reason() {
                    RunTransition::Cancel { reason }
                } else {
                    RunTransition::Interrupt {
                        failure: ActionableFailure::interrupted(if join_result.is_err() {
                            "run.task_panic"
                        } else {
                            "run.task_incomplete"
                        }),
                    }
                };
                let _ = repository.transition(&supervised_id, transition).await;
            }
            supervisor_complete.store(true, Ordering::Release);
            // `notify_one` retains a permit when the drain future has not been
            // polled yet. Together with the completion flag this closes both
            // sides of the completion/wait registration race.
            finished.notify_one();
        });
        scope.tasks.insert(
            run_id.clone(),
            TrackedRun {
                cancellation,
                finished: task_complete,
                task,
            },
        );
        Ok(run_id)
    }

    pub async fn cancel_run(&self, id: &RunId, reason: CancellationReason) -> bool {
        let scope = self.tasks.lock().await;
        scope.tasks.get(id).is_some_and(|tracked| {
            tracked.cancellation.cancel(reason) || tracked.cancellation.is_cancelled()
        })
    }

    pub async fn cancel_and_drain(
        &self,
        reason: CancellationReason,
        timeout: Duration,
    ) -> Result<(), DrainTimeout> {
        {
            let mut scope = self.tasks.lock().await;
            self.state.store(SESSION_CLOSING, Ordering::Release);
            scope.accepting = false;
            for tracked in scope.tasks.values() {
                tracked.cancellation.cancel(reason);
            }
        }

        let deadline = Instant::now() + timeout;
        loop {
            let notified = self.task_finished.notified();
            let finished_tasks = {
                let mut scope = self.tasks.lock().await;
                let finished_ids: Vec<_> = scope
                    .tasks
                    .iter()
                    .filter_map(|(id, tracked)| {
                        tracked
                            .finished
                            .load(Ordering::Acquire)
                            .then_some(id.clone())
                    })
                    .collect();
                let mut handles = Vec::with_capacity(finished_ids.len());
                for id in finished_ids {
                    if let Some(tracked) = scope.tasks.remove(&id) {
                        handles.push(tracked.task);
                    }
                }
                if scope.tasks.is_empty() && handles.is_empty() {
                    return Ok(());
                }
                handles
            };
            for task in finished_tasks {
                let _ = task.await;
            }
            if self.tasks.lock().await.tasks.is_empty() {
                return Ok(());
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                let mut blocked_runs: Vec<_> =
                    self.tasks.lock().await.tasks.keys().cloned().collect();
                blocked_runs.sort_by(|left, right| left.0.cmp(&right.0));
                return Err(DrainTimeout { blocked_runs });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ats_core::platform::domain::{RunKind, RunRecord, RunTimelineEventKind};
    use tokio::sync::{Barrier, Notify};

    async fn session() -> (tempfile::TempDir, Arc<ProjectSession>) {
        let temp = tempfile::TempDir::new().unwrap();
        let project = ProjectFolder::create(temp.path(), "sample", "sts2").unwrap();
        let session = ProjectSession::open(project).await.unwrap();
        (temp, session)
    }

    #[tokio::test]
    async fn close_cancels_and_waits_before_releasing_scope() {
        let (_temp, session) = session().await;
        let repo = session.file_repository();
        let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        repo.create(&run).await.unwrap();
        let cancellation = ats_core::platform::CancellationToken::new();
        let worker_token = cancellation.clone();
        let spawned = SpawnedRun {
            run_id: run.id.clone(),
            cancellation,
            task: tokio::spawn(async move {
                worker_token.cancelled().await;
            }),
        };
        session.submit(async { Ok(spawned) }).await.unwrap();

        session
            .cancel_and_drain(CancellationReason::ProjectClose, Duration::from_secs(1))
            .await
            .unwrap();
        let run = repo.get(&run.id).await.unwrap();
        assert_eq!(run.status, RunStatus::Cancelled);
        assert_eq!(
            run.timeline.last().unwrap().cancellation_reason,
            Some(CancellationReason::ProjectClose)
        );
    }

    #[tokio::test]
    async fn timeout_keeps_closing_scope_for_a_retry() {
        let (_temp, session) = session().await;
        let repo = session.file_repository();
        let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        repo.create(&run).await.unwrap();
        let cancellation = ats_core::platform::CancellationToken::new();
        let release = Arc::new(Notify::new());
        let worker_release = Arc::clone(&release);
        let spawned = SpawnedRun {
            run_id: run.id.clone(),
            cancellation,
            task: tokio::spawn(async move {
                worker_release.notified().await;
            }),
        };
        session.submit(async { Ok(spawned) }).await.unwrap();

        let timeout = session
            .cancel_and_drain(CancellationReason::ProjectClose, Duration::from_millis(10))
            .await
            .unwrap_err();
        assert_eq!(timeout.blocked_runs, vec![run.id.clone()]);
        assert!(session.is_closing());
        assert!(matches!(
            session
                .submit(async { Err(RunError::Storage("must not run".into())) })
                .await,
            Err(SubmitError::Closing)
        ));

        release.notify_waiters();
        session
            .cancel_and_drain(CancellationReason::ProjectClose, Duration::from_secs(1))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn submit_and_close_barrier_registers_before_closing_or_rejects() {
        let (_temp, session) = session().await;
        let repo = session.file_repository();
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let submit_session = Arc::clone(&session);
        let submit_repo = Arc::clone(&repo);
        let submit_entered = Arc::clone(&entered);
        let submit_release = Arc::clone(&release);
        let submit = tokio::spawn(async move {
            submit_session
                .submit(async move {
                    let run = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
                    submit_repo.create(&run).await.unwrap();
                    submit_entered.wait().await;
                    submit_release.wait().await;
                    let cancellation = ats_core::platform::CancellationToken::new();
                    let worker_cancellation = cancellation.clone();
                    Ok(SpawnedRun {
                        run_id: run.id,
                        cancellation,
                        task: tokio::spawn(async move {
                            worker_cancellation.cancelled().await;
                        }),
                    })
                })
                .await
        });

        entered.wait().await;
        let close_session = Arc::clone(&session);
        let close = tokio::spawn(async move {
            close_session
                .cancel_and_drain(CancellationReason::ProjectClose, Duration::from_secs(1))
                .await
        });
        tokio::task::yield_now().await;
        assert!(!session.is_closing());
        release.wait().await;

        let run_id = submit.await.unwrap().unwrap();
        close.await.unwrap().unwrap();
        let record = repo.get(&run_id).await.unwrap();
        assert_eq!(record.status, RunStatus::Cancelled);
        assert_eq!(
            record
                .timeline
                .iter()
                .filter(|event| event.kind == RunTimelineEventKind::Cancelled)
                .count(),
            1
        );
        assert!(session.tasks.lock().await.tasks.is_empty());
    }

    #[tokio::test]
    async fn open_reconciles_pending_run_before_returning_session() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = ProjectFolder::create(temp.path(), "sample", "sts2").unwrap();
        let repo = FileRunRepository::new(project.history_dir());
        let pending = RunRecord::new(RunKind::TextGenerate, serde_json::json!({}));
        repo.create(&pending).await.unwrap();

        let session = ProjectSession::open(project).await.unwrap();
        let reconciled = session.file_repository().get(&pending.id).await.unwrap();
        assert_eq!(reconciled.status, RunStatus::Failed);
        assert!(
            reconciled
                .failure
                .is_some_and(|failure| failure.code == "run.interrupted")
        );
        assert_eq!(
            reconciled
                .timeline
                .iter()
                .filter(|event| event.kind == RunTimelineEventKind::Interrupted)
                .count(),
            1
        );
    }
}
