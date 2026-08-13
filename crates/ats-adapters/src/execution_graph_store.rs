use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ats_kernel::ExecutionGraphId;
use ats_runtime::{
    ExecutionGraphRecord, ExecutionGraphRecovery, ExecutionGraphRepository,
    ExecutionGraphRepositoryError, RunId, RunRepository, RunRepositoryError,
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct FileExecutionGraphRepository {
    project_root: PathBuf,
    gate: Mutex<()>,
}

impl FileExecutionGraphRepository {
    #[must_use]
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            gate: Mutex::new(()),
        }
    }

    fn root(&self) -> PathBuf {
        self.project_root.join(".ats").join("execution-graphs-v4")
    }

    fn path(&self, id: &ExecutionGraphId) -> PathBuf {
        self.root().join(format!("{id}.json"))
    }

    fn prepare_root(&self) -> Result<PathBuf, ExecutionGraphRepositoryError> {
        validate_directory(&self.project_root)?;
        let ats_root = self.project_root.join(".ats");
        ensure_directory(&ats_root)?;
        let root = self.root();
        ensure_directory(&root)?;
        cleanup_owned_temporary_files(&root)?;
        Ok(root)
    }

    fn load_unlocked(
        &self,
        id: &ExecutionGraphId,
    ) -> Result<ExecutionGraphRecord, ExecutionGraphRepositoryError> {
        let path = self.path(id);
        let metadata = fs::symlink_metadata(&path).map_err(map_not_found)?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(ExecutionGraphRepositoryError::InvalidRecord);
        }
        let graph: ExecutionGraphRecord =
            serde_json::from_slice(&fs::read(path).map_err(ExecutionGraphRepositoryError::Io)?)
                .map_err(ExecutionGraphRepositoryError::Json)?;
        if graph.id() != id {
            return Err(ExecutionGraphRepositoryError::InvalidRecord);
        }
        Ok(graph)
    }

    fn list_unlocked(&self) -> Result<Vec<ExecutionGraphRecord>, ExecutionGraphRepositoryError> {
        let root = self.prepare_root()?;
        let mut graphs = Vec::new();
        for entry in fs::read_dir(root).map_err(ExecutionGraphRepositoryError::Io)? {
            let entry = entry.map_err(ExecutionGraphRepositoryError::Io)?;
            let file_type = entry
                .file_type()
                .map_err(ExecutionGraphRepositoryError::Io)?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(ExecutionGraphRepositoryError::InvalidRecord);
            }
            let name = entry
                .file_name()
                .to_str()
                .ok_or(ExecutionGraphRepositoryError::InvalidRecord)?
                .to_owned();
            let id = ExecutionGraphId::parse(
                name.strip_suffix(".json")
                    .ok_or(ExecutionGraphRepositoryError::InvalidRecord)?,
            )
            .map_err(|_| ExecutionGraphRepositoryError::InvalidRecord)?;
            graphs.push(self.load_unlocked(&id)?);
        }
        graphs.sort_by(|left, right| left.id().cmp(right.id()));
        Ok(graphs)
    }
}

impl ExecutionGraphRepository for FileExecutionGraphRepository {
    fn create_claimed(
        &self,
        graph: &ExecutionGraphRecord,
        run_id: &RunId,
    ) -> Result<(), ExecutionGraphRepositoryError> {
        graph
            .validate()
            .map_err(|_| ExecutionGraphRepositoryError::InvalidRecord)?;
        if graph.revision() != 1 || graph.active_run_id() != Some(run_id) {
            return Err(ExecutionGraphRepositoryError::InvalidRecord);
        }
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ExecutionGraphRepositoryError::Conflict)?;
        self.prepare_root()?;
        let path = self.path(graph.id());
        write_new(&path, graph).map_err(|error| match error {
            ExecutionGraphRepositoryError::Io(source)
                if source.kind() == io::ErrorKind::AlreadyExists =>
            {
                ExecutionGraphRepositoryError::AlreadyExists
            }
            other => other,
        })
    }

    fn get(
        &self,
        id: &ExecutionGraphId,
    ) -> Result<ExecutionGraphRecord, ExecutionGraphRepositoryError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ExecutionGraphRepositoryError::Conflict)?;
        self.prepare_root()?;
        self.load_unlocked(id)
    }

    fn compare_and_set(
        &self,
        expected_revision: u64,
        next: &ExecutionGraphRecord,
    ) -> Result<(), ExecutionGraphRepositoryError> {
        next.validate()
            .map_err(|_| ExecutionGraphRepositoryError::InvalidRecord)?;
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ExecutionGraphRepositoryError::Conflict)?;
        self.prepare_root()?;
        let current = self.load_unlocked(next.id())?;
        if current.revision() != expected_revision
            || next.revision() != expected_revision.saturating_add(1)
            || current.owner_feature_id() != next.owner_feature_id()
            || current.request_snapshot_hash() != next.request_snapshot_hash()
            || current.created_at() != next.created_at()
            || current.blueprint() != next.blueprint()
        {
            return Err(ExecutionGraphRepositoryError::Conflict);
        }
        write_atomic(&self.path(next.id()), next)
    }

    fn list(&self) -> Result<Vec<ExecutionGraphRecord>, ExecutionGraphRepositoryError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ExecutionGraphRepositoryError::Conflict)?;
        self.list_unlocked()
    }

    fn recover_structure(
        &self,
        runs: &dyn RunRepository,
    ) -> Result<ExecutionGraphRecovery, ExecutionGraphRepositoryError> {
        let _guard = self
            .gate
            .lock()
            .map_err(|_| ExecutionGraphRepositoryError::Conflict)?;
        let graphs = self.list_unlocked()?;
        let mut report = ExecutionGraphRecovery::default();
        for mut graph in graphs {
            report.inspected = report.inspected.saturating_add(1);
            let Some(run_id) = graph.active_run_id().cloned() else {
                continue;
            };
            match runs.get(&run_id) {
                Ok(run) => {
                    if run.feature_id() != graph.owner_feature_id() {
                        return Err(ExecutionGraphRepositoryError::InvalidRecord);
                    }
                }
                Err(RunRepositoryError::NotFound) => {}
                Err(_) => return Err(ExecutionGraphRepositoryError::InvalidRecord),
            }
            let expected_revision = graph.revision();
            graph
                .recover_stale_claim(chrono::Utc::now())
                .map_err(|_| ExecutionGraphRepositoryError::InvalidRecord)?;
            let current = self.load_unlocked(graph.id())?;
            if current.revision() != expected_revision {
                return Err(ExecutionGraphRepositoryError::Conflict);
            }
            write_atomic(&self.path(graph.id()), &graph)?;
            report.recovered = report.recovered.saturating_add(1);
        }
        Ok(report)
    }
}

fn ensure_directory(path: &Path) -> Result<(), ExecutionGraphRepositoryError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() => {
            Ok(())
        }
        Ok(_) => Err(ExecutionGraphRepositoryError::InvalidRecord),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(ExecutionGraphRepositoryError::Io)?;
            validate_directory(path)
        }
        Err(error) => Err(ExecutionGraphRepositoryError::Io(error)),
    }
}

fn validate_directory(path: &Path) -> Result<(), ExecutionGraphRepositoryError> {
    let metadata = fs::symlink_metadata(path).map_err(ExecutionGraphRepositoryError::Io)?;
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(ExecutionGraphRepositoryError::InvalidRecord)
    }
}

fn cleanup_owned_temporary_files(root: &Path) -> Result<(), ExecutionGraphRepositoryError> {
    for entry in fs::read_dir(root).map_err(ExecutionGraphRepositoryError::Io)? {
        let entry = entry.map_err(ExecutionGraphRepositoryError::Io)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') && name.contains(".json.") && name.ends_with(".tmp") {
            let file_type = entry
                .file_type()
                .map_err(ExecutionGraphRepositoryError::Io)?;
            if !file_type.is_file() || file_type.is_symlink() {
                return Err(ExecutionGraphRepositoryError::InvalidRecord);
            }
            fs::remove_file(entry.path()).map_err(ExecutionGraphRepositoryError::Io)?;
        }
    }
    Ok(())
}

fn write_new(
    path: &Path,
    graph: &ExecutionGraphRecord,
) -> Result<(), ExecutionGraphRepositoryError> {
    let bytes = serde_json::to_vec_pretty(graph).map_err(ExecutionGraphRepositoryError::Json)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(ExecutionGraphRepositoryError::Io)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(ExecutionGraphRepositoryError::Io)
}

fn write_atomic(
    path: &Path,
    graph: &ExecutionGraphRecord,
) -> Result<(), ExecutionGraphRepositoryError> {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(ExecutionGraphRepositoryError::InvalidRecord)?;
    let temporary = path.with_file_name(format!(".{name}.{}.{counter}.tmp", std::process::id()));
    let result = (|| {
        write_new(&temporary, graph)?;
        fs::rename(&temporary, path).map_err(ExecutionGraphRepositoryError::Io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn map_not_found(error: io::Error) -> ExecutionGraphRepositoryError {
    if error.kind() == io::ErrorKind::NotFound {
        ExecutionGraphRepositoryError::NotFound
    } else {
        ExecutionGraphRepositoryError::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use ats_kernel::{
        ExecutionNodeId, FeatureId, SchemaId, SchemaRef, SchemaVersion, Sha256Digest,
    };
    use ats_runtime::{
        ExecutionGraphStatus, ExecutionNodeSpec, RunRecord, RunStatus, RunTransition,
        VersionedPayload,
    };

    use super::*;

    fn payload() -> VersionedPayload {
        VersionedPayload::from_typed(
            SchemaRef {
                id: SchemaId::parse("fixture.blueprint").unwrap(),
                version: SchemaVersion::new(1).unwrap(),
            },
            &serde_json::json!({"value": 1}),
        )
        .unwrap()
    }

    fn graph(run_id: RunId) -> ExecutionGraphRecord {
        ExecutionGraphRecord::new_claimed(
            ExecutionGraphId::parse("graph-fixture").unwrap(),
            FeatureId::parse("composition.plan").unwrap(),
            Sha256Digest::parse("a".repeat(64)).unwrap(),
            payload(),
            vec![ExecutionNodeSpec {
                node_id: ExecutionNodeId::parse("node.fixture").unwrap(),
                role_id: "item.generate".into(),
                depends_on: Vec::new(),
                request_snapshot_hash: Sha256Digest::parse("b".repeat(64)).unwrap(),
            }],
            run_id,
            chrono::Utc::now(),
        )
        .unwrap()
    }

    #[test]
    fn create_cas_reload_and_recover_stale_claim() {
        let temp = tempfile::tempdir().unwrap();
        let run_root = temp.path().join("runs");
        let runs = crate::FileRunRepository::new(run_root).unwrap();
        let mut run = RunRecord::new(FeatureId::parse("composition.plan").unwrap(), payload());
        runs.create(&run).unwrap();
        run.apply_transition(RunTransition::Start, chrono::Utc::now())
            .unwrap();
        runs.persist(&run, RunStatus::Pending).unwrap();

        let repository = FileExecutionGraphRepository::new(temp.path().to_path_buf());
        let graph = graph(run.id().clone());
        repository.create_claimed(&graph, run.id()).unwrap();
        assert_eq!(repository.get(graph.id()).unwrap(), graph);
        assert!(temp.path().join(".ats/execution-graphs-v4").is_dir());
        assert!(!temp.path().join(".ats/execution-graphs-v3").exists());

        let report = repository.recover_structure(&runs).unwrap();
        assert_eq!(report.recovered, 1);
        let recovered = repository.get(graph.id()).unwrap();
        assert_eq!(recovered.status(), ExecutionGraphStatus::Paused);
        assert!(recovered.active_run_id().is_none());
    }

    #[test]
    fn cas_rejects_stale_revision_and_cleans_owned_temporary_files() {
        let temp = tempfile::tempdir().unwrap();
        let repository = FileExecutionGraphRepository::new(temp.path().to_path_buf());
        let run_id = RunId::parse("run-fixture").unwrap();
        let graph = graph(run_id.clone());
        repository.create_claimed(&graph, &run_id).unwrap();

        let temporary = repository.root().join(".graph-fixture.json.1.1.tmp");
        fs::write(&temporary, b"partial").unwrap();
        assert_eq!(repository.list().unwrap().len(), 1);
        assert!(!temporary.exists());

        assert!(matches!(
            repository.compare_and_set(graph.revision() + 1, &graph),
            Err(ExecutionGraphRepositoryError::Conflict)
        ));
    }
}
