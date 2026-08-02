use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ats_kernel::FailureCode;
use ats_runtime::{
    RunFailure, RunId, RunRecord, RunRepository, RunRepositoryError, RunStatus, RunSummary,
    RunTransition,
};
use chrono::Utc;

#[derive(Debug)]
pub struct FileRunRepository {
    root: PathBuf,
    gate: Mutex<()>,
}

impl FileRunRepository {
    pub fn new(root: PathBuf) -> Result<Self, RunRepositoryError> {
        fs::create_dir_all(&root).map_err(RunRepositoryError::Io)?;
        validate_directory(&root)?;
        Ok(Self {
            root,
            gate: Mutex::new(()),
        })
    }

    fn path(&self, id: &RunId) -> PathBuf {
        self.root.join(format!("{}.json", id.as_str()))
    }
}

impl RunRepository for FileRunRepository {
    fn create(&self, run: &RunRecord) -> Result<(), RunRepositoryError> {
        run.validate()
            .map_err(|_| RunRepositoryError::InvalidRecord)?;
        let _guard = self.gate.lock().map_err(|_| RunRepositoryError::Conflict)?;
        let path = self.path(run.id());
        if path.exists() {
            return Err(RunRepositoryError::AlreadyExists);
        }
        write_new(&path, run)
    }

    fn get(&self, id: &RunId) -> Result<RunRecord, RunRepositoryError> {
        let _guard = self.gate.lock().map_err(|_| RunRepositoryError::Conflict)?;
        read(&self.path(id))
    }

    fn list(&self) -> Result<Vec<RunSummary>, RunRepositoryError> {
        let _guard = self.gate.lock().map_err(|_| RunRepositoryError::Conflict)?;
        let mut summaries = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(RunRepositoryError::Io)? {
            let entry = entry.map_err(RunRepositoryError::Io)?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                summaries.push(RunSummary::from(&read(&entry.path())?));
            }
        }
        summaries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(summaries)
    }

    fn persist(
        &self,
        run: &RunRecord,
        expected_status: RunStatus,
    ) -> Result<(), RunRepositoryError> {
        run.validate()
            .map_err(|_| RunRepositoryError::InvalidRecord)?;
        let _guard = self.gate.lock().map_err(|_| RunRepositoryError::Conflict)?;
        let path = self.path(run.id());
        let current = read(&path)?;
        if current.status() != expected_status
            || current.feature_id() != run.feature_id()
            || current.request() != run.request()
        {
            return Err(RunRepositoryError::Conflict);
        }
        replace(&path, run)
    }

    fn reconcile_interrupted(&self) -> Result<u32, RunRepositoryError> {
        let _guard = self.gate.lock().map_err(|_| RunRepositoryError::Conflict)?;
        let mut count = 0_u32;
        for entry in fs::read_dir(&self.root).map_err(RunRepositoryError::Io)? {
            let entry = entry.map_err(RunRepositoryError::Io)?;
            if entry
                .path()
                .extension()
                .is_none_or(|extension| extension != "json")
            {
                continue;
            }
            let mut run = read(&entry.path())?;
            let expected = run.status();
            if matches!(expected, RunStatus::Pending | RunStatus::Running) {
                run.apply_transition(
                    RunTransition::Interrupt {
                        failure: RunFailure::new(
                            FailureCode::parse("run.interrupted")
                                .expect("built-in failure code is valid"),
                            "run.reconcile",
                            None,
                        )
                        .expect("built-in failure is valid"),
                    },
                    Utc::now(),
                )
                .map_err(|_| RunRepositoryError::InvalidRecord)?;
                replace(&entry.path(), &run)?;
                count = count.saturating_add(1);
            }
        }
        Ok(count)
    }
}

fn read(path: &Path) -> Result<RunRecord, RunRepositoryError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => RunRepositoryError::NotFound,
        _ => RunRepositoryError::Io(error),
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(RunRepositoryError::InvalidRecord);
    }
    serde_json::from_slice(&fs::read(path).map_err(RunRepositoryError::Io)?)
        .map_err(RunRepositoryError::Json)
}

fn write_new(path: &Path, run: &RunRecord) -> Result<(), RunRepositoryError> {
    let bytes = serde_json::to_vec_pretty(run).map_err(RunRepositoryError::Json)?;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(RunRepositoryError::Io)?;
    file.write_all(&bytes).map_err(RunRepositoryError::Io)?;
    file.sync_all().map_err(RunRepositoryError::Io)
}

fn replace(path: &Path, run: &RunRecord) -> Result<(), RunRepositoryError> {
    let temporary = path.with_extension("json.ats-tmp");
    let backup = path.with_extension("json.ats-backup");
    let _ = fs::remove_file(&temporary);
    let _ = fs::remove_file(&backup);
    write_new(&temporary, run)?;
    fs::rename(path, &backup).map_err(RunRepositoryError::Io)?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::rename(&backup, path);
        let _ = fs::remove_file(&temporary);
        return Err(RunRepositoryError::Io(error));
    }
    fs::remove_file(backup).map_err(RunRepositoryError::Io)
}

fn validate_directory(path: &Path) -> Result<(), RunRepositoryError> {
    let metadata = fs::symlink_metadata(path).map_err(RunRepositoryError::Io)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(RunRepositoryError::InvalidRecord)
    }
}

#[cfg(test)]
mod tests {
    use ats_kernel::{FeatureId, SchemaId, SchemaRef, SchemaVersion};
    use ats_runtime::VersionedPayload;

    use super::*;

    fn run() -> RunRecord {
        RunRecord::new(
            FeatureId::parse("fixture.run").unwrap(),
            VersionedPayload::from_typed(
                SchemaRef {
                    id: SchemaId::parse("fixture.request").unwrap(),
                    version: SchemaVersion::new(1).unwrap(),
                },
                &serde_json::json!({"value":1}),
            )
            .unwrap(),
        )
    }

    #[test]
    fn create_persist_reload_and_reconcile_are_v3_only() {
        let temp = tempfile::TempDir::new().unwrap();
        let repository = FileRunRepository::new(temp.path().to_path_buf()).unwrap();
        let mut record = run();
        repository.create(&record).unwrap();
        record
            .apply_transition(RunTransition::Start, Utc::now())
            .unwrap();
        repository.persist(&record, RunStatus::Pending).unwrap();
        assert_eq!(repository.get(record.id()).unwrap(), record);
        assert_eq!(repository.reconcile_interrupted().unwrap(), 1);
        assert_eq!(
            repository.get(record.id()).unwrap().status(),
            RunStatus::Failed
        );
    }
}
