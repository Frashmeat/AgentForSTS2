use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use std::time::Duration;

use ats_adapters::{
    FileCompositionDraftRepository, FileExecutionGraphRepository, FileItemRepository,
    FileProjectWriter, FileResourceRepository, FileRunRepository,
};
use ats_runtime::{
    CancellationReason, CancellationToken, ExecutionGraphRecord, ExecutionGraphRepository,
    ExecutionGraphRepositoryError, RunFailure, RunId, RunRecord, RunRepository, RunRepositoryError,
    RunStatus, RunTransition,
};
use ats_workspace::{ProjectFolder, ProjectMeta};
use chrono::Utc;
use futures_util::FutureExt;
use thiserror::Error;
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
    execution_graph_repository: Arc<FileExecutionGraphRepository>,
    item_repository: Arc<FileItemRepository>,
    composition_draft_repository: Arc<FileCompositionDraftRepository>,
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

#[derive(Debug, Error)]
pub enum ProjectSessionOpenError {
    #[error("project write recovery failed")]
    ProjectRecovery(#[from] ats_runtime::ProjectWriteError),
    #[error(transparent)]
    RunRepository(#[from] RunRepositoryError),
    #[error(transparent)]
    ExecutionGraphRepository(#[from] ExecutionGraphRepositoryError),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DrainTimeout {
    pub blocked_runs: Vec<RunId>,
}

impl ProjectSession {
    pub fn open(project: ProjectFolder) -> Result<Arc<Self>, ProjectSessionOpenError> {
        FileProjectWriter::recover(project.path())?;
        let repository = Arc::new(FileRunRepository::new(project.run_history_dir())?);
        let execution_graph_repository = Arc::new(FileExecutionGraphRepository::new(
            project.path().to_path_buf(),
        ));
        execution_graph_repository.recover_structure(repository.as_ref())?;
        repository.reconcile_interrupted()?;
        let item_repository = Arc::new(FileItemRepository::new(project.path().to_path_buf()));
        let composition_draft_repository = Arc::new(FileCompositionDraftRepository::new(
            project.path().to_path_buf(),
        ));
        let resource_repository =
            Arc::new(FileResourceRepository::new(project.path().to_path_buf()));
        Ok(Arc::new(Self {
            root: project.path().to_path_buf(),
            meta: project.meta().clone(),
            project_lock: StdMutex::new(Some(project)),
            repository,
            execution_graph_repository,
            item_repository,
            composition_draft_repository,
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
    pub fn execution_graph_repository(&self) -> Arc<FileExecutionGraphRepository> {
        Arc::clone(&self.execution_graph_repository)
    }

    #[must_use]
    pub fn item_repository(&self) -> Arc<FileItemRepository> {
        Arc::clone(&self.item_repository)
    }

    #[must_use]
    pub fn composition_draft_repository(&self) -> Arc<FileCompositionDraftRepository> {
        Arc::clone(&self.composition_draft_repository)
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
        self.submit_internal(&mut run, None, worker).await
    }

    pub async fn submit_claimed<F, Fut>(
        &self,
        mut run: RunRecord,
        graph: ExecutionGraphRecord,
        worker: F,
    ) -> Result<RunId, SubmitError>
    where
        F: FnOnce(RunRecord, CancellationToken, Arc<dyn RunRepository>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<RunRecord, RunFailure>> + Send + 'static,
    {
        self.submit_internal(&mut run, Some((None, graph)), worker)
            .await
    }

    pub async fn submit_resumed<F, Fut>(
        &self,
        mut run: RunRecord,
        expected_revision: u64,
        graph: ExecutionGraphRecord,
        worker: F,
    ) -> Result<RunId, SubmitError>
    where
        F: FnOnce(RunRecord, CancellationToken, Arc<dyn RunRepository>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<RunRecord, RunFailure>> + Send + 'static,
    {
        self.submit_internal(&mut run, Some((Some(expected_revision), graph)), worker)
            .await
    }

    async fn submit_internal<F, Fut>(
        &self,
        run: &mut RunRecord,
        graph: Option<(Option<u64>, ExecutionGraphRecord)>,
        worker: F,
    ) -> Result<RunId, SubmitError>
    where
        F: FnOnce(RunRecord, CancellationToken, Arc<dyn RunRepository>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<RunRecord, RunFailure>> + Send + 'static,
    {
        let mut tasks = self.tasks.lock().await;
        if self.is_closing() {
            return Err(SubmitError::Closing);
        }
        if let Some((expected_revision, graph)) = &graph {
            match expected_revision {
                Some(expected_revision) => self
                    .execution_graph_repository
                    .compare_and_set(*expected_revision, graph),
                None => self
                    .execution_graph_repository
                    .create_claimed(graph, run.id()),
            }
            .map_err(|_| SubmitError::Repository)?;
        }
        persist_started_run(
            self.repository.as_ref(),
            self.execution_graph_repository.as_ref(),
            run,
            graph.as_ref().map(|(_, graph)| graph),
        )?;

        let id = run.id().clone();
        let run = run.clone();
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

fn persist_started_run<R, G>(
    runs: &R,
    graphs: &G,
    run: &mut RunRecord,
    claimed_graph: Option<&ExecutionGraphRecord>,
) -> Result<(), SubmitError>
where
    R: RunRepository + ?Sized,
    G: ExecutionGraphRepository + ?Sized,
{
    let result = runs
        .create(run)
        .map_err(|_| SubmitError::Repository)
        .and_then(|()| {
            run.apply_transition(RunTransition::Start, Utc::now())
                .map_err(|_| SubmitError::Repository)
        })
        .and_then(|()| {
            runs.persist(run, RunStatus::Pending)
                .map_err(|_| SubmitError::Repository)
        });
    if result.is_err()
        && let Some(graph) = claimed_graph
    {
        let _ = compensate_graph_claim(graphs, graph.clone());
    }
    result
}

fn compensate_graph_claim<G>(
    repository: &G,
    mut graph: ExecutionGraphRecord,
) -> Result<(), ExecutionGraphRepositoryError>
where
    G: ExecutionGraphRepository + ?Sized,
{
    let expected_revision = graph.revision();
    graph
        .recover_stale_claim(Utc::now())
        .map_err(|_| ExecutionGraphRepositoryError::InvalidRecord)?;
    repository.compare_and_set(expected_revision, &graph)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ats_kernel::{
        ExecutionGraphId, ExecutionNodeId, FeatureId, SchemaId, SchemaRef, SchemaVersion,
        Sha256Digest,
    };
    use ats_runtime::{
        ExecutionGraphRecord, ExecutionGraphRepository, ExecutionGraphStatus, ExecutionNodeSpec,
        ProjectFileWrite, ProjectFileWriter, RunSummary, VersionedPayload,
    };

    use super::*;

    #[derive(Default)]
    struct PersistFailingRunRepository {
        created: StdMutex<Option<RunRecord>>,
    }

    impl RunRepository for PersistFailingRunRepository {
        fn create(&self, run: &RunRecord) -> Result<(), RunRepositoryError> {
            *self.created.lock().unwrap() = Some(run.clone());
            Ok(())
        }

        fn get(&self, id: &RunId) -> Result<RunRecord, RunRepositoryError> {
            self.created
                .lock()
                .unwrap()
                .clone()
                .filter(|run| run.id() == id)
                .ok_or(RunRepositoryError::NotFound)
        }

        fn list(&self) -> Result<Vec<RunSummary>, RunRepositoryError> {
            Ok(self
                .created
                .lock()
                .unwrap()
                .as_ref()
                .map(RunSummary::from)
                .into_iter()
                .collect())
        }

        fn persist(&self, _: &RunRecord, _: RunStatus) -> Result<(), RunRepositoryError> {
            Err(RunRepositoryError::Conflict)
        }

        fn reconcile_interrupted(&self) -> Result<u32, RunRepositoryError> {
            Ok(0)
        }
    }

    #[test]
    fn run_start_persist_failure_releases_the_execution_graph_claim() {
        let temp = tempfile::tempdir().unwrap();
        let payload = VersionedPayload::from_typed(
            SchemaRef {
                id: SchemaId::parse("fixture.request").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
            &serde_json::json!({"value": 1}),
        )
        .unwrap();
        let run_id = RunId::parse("run-persist-failure").unwrap();
        let mut run = RunRecord::new_with_id(
            run_id.clone(),
            FeatureId::parse("composition.plan").unwrap(),
            payload.clone(),
        );
        let graph = ExecutionGraphRecord::new_claimed(
            ExecutionGraphId::parse("graph-persist-failure").unwrap(),
            FeatureId::parse("composition.plan").unwrap(),
            Sha256Digest::parse("a".repeat(64)).unwrap(),
            payload,
            vec![ExecutionNodeSpec {
                node_id: ExecutionNodeId::parse("node.pending").unwrap(),
                role_id: "item.generate".into(),
                depends_on: Vec::new(),
                request_snapshot_hash: Sha256Digest::parse("b".repeat(64)).unwrap(),
            }],
            run_id.clone(),
            Utc::now(),
        )
        .unwrap();
        let graphs = FileExecutionGraphRepository::new(temp.path().to_path_buf());
        graphs.create_claimed(&graph, &run_id).unwrap();
        let runs = PersistFailingRunRepository::default();

        assert!(matches!(
            persist_started_run(&runs, &graphs, &mut run, Some(&graph)),
            Err(SubmitError::Repository)
        ));
        let recovered = graphs.get(graph.id()).unwrap();
        assert_eq!(recovered.status(), ExecutionGraphStatus::Paused);
        assert_eq!(recovered.active_run_id(), None);
        assert_eq!(recovered.previous_run_id(), Some(&run_id));
        assert_eq!(runs.get(&run_id).unwrap().status(), RunStatus::Pending);
    }

    #[test]
    fn opening_a_project_recovers_prepared_publication_before_exposing_the_session() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join(".ats/runs-v3")).unwrap();
        fs::create_dir(project.join("Generated")).unwrap();
        fs::write(project.join(".ats/version"), b"2").unwrap();
        fs::write(
            project.join("project.json"),
            serde_json::to_vec(&ProjectMeta {
                name: "RecoveryProject".into(),
                csharp_name: "RecoveryProject".into(),
                game_id: "sts2".into(),
                scaffolded: true,
                generated_files: Vec::new(),
                build_output_dir: None,
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(project.join("Generated/One.cs"), b"old").unwrap();
        let pending = FileProjectWriter
            .apply(
                &project,
                &RunId::new(),
                vec![ProjectFileWrite::new("Generated/One.cs", b"new".to_vec()).unwrap()],
            )
            .unwrap();
        drop(pending);
        assert_eq!(fs::read(project.join("Generated/One.cs")).unwrap(), b"new");

        let folder = ProjectFolder::open(&project).unwrap();
        let session = ProjectSession::open(folder).unwrap();
        assert_eq!(fs::read(project.join("Generated/One.cs")).unwrap(), b"old");
        assert!(
            !project
                .join(".ats/transactions")
                .read_dir()
                .unwrap()
                .any(|_| true)
        );
        session.release_project_lock().unwrap();
    }

    #[test]
    fn opening_a_project_pauses_graphs_before_interrupting_their_runs() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir_all(project.join(".ats/runs-v3")).unwrap();
        fs::write(project.join(".ats/version"), b"2").unwrap();
        fs::write(
            project.join("project.json"),
            serde_json::to_vec(&ProjectMeta {
                name: "GraphRecoveryProject".into(),
                csharp_name: "GraphRecoveryProject".into(),
                game_id: "sts2".into(),
                scaffolded: true,
                generated_files: Vec::new(),
                build_output_dir: None,
            })
            .unwrap(),
        )
        .unwrap();
        let payload = VersionedPayload::from_typed(
            SchemaRef {
                id: SchemaId::parse("fixture.request").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
            &serde_json::json!({"value": 1}),
        )
        .unwrap();
        let runs = FileRunRepository::new(project.join(".ats/runs-v3")).unwrap();
        let mut run = RunRecord::new(
            FeatureId::parse("composition.plan").unwrap(),
            payload.clone(),
        );
        runs.create(&run).unwrap();
        run.apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        runs.persist(&run, RunStatus::Pending).unwrap();
        let graph = ExecutionGraphRecord::new_claimed(
            ExecutionGraphId::parse("graph-recovery").unwrap(),
            FeatureId::parse("composition.plan").unwrap(),
            Sha256Digest::parse("a".repeat(64)).unwrap(),
            payload,
            vec![ExecutionNodeSpec {
                node_id: ExecutionNodeId::parse("node.pending").unwrap(),
                role_id: "item.generate".into(),
                depends_on: Vec::new(),
                request_snapshot_hash: Sha256Digest::parse("b".repeat(64)).unwrap(),
            }],
            run.id().clone(),
            Utc::now(),
        )
        .unwrap();
        let graphs = FileExecutionGraphRepository::new(project.clone());
        graphs.create_claimed(&graph, run.id()).unwrap();

        let session = ProjectSession::open(ProjectFolder::open(&project).unwrap()).unwrap();
        assert_eq!(
            session
                .execution_graph_repository()
                .get(graph.id())
                .unwrap()
                .status(),
            ExecutionGraphStatus::Paused
        );
        assert_eq!(
            session.repository.get(run.id()).unwrap().status(),
            RunStatus::Failed
        );
        session.release_project_lock().unwrap();
    }
}
