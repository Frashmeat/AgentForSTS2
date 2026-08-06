use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ats_runtime::{
    PendingProjectWrites, ProjectFileWrite, ProjectFileWriter, ProjectWriteError, RunId,
    validate_project_writes,
};

#[derive(Debug, Default)]
pub struct FileProjectWriter;

impl ProjectFileWriter for FileProjectWriter {
    fn apply(
        &self,
        project_root: &Path,
        run_id: &RunId,
        writes: Vec<ProjectFileWrite>,
    ) -> Result<Box<dyn PendingProjectWrites>, ProjectWriteError> {
        validate_project_writes(&writes)?;
        validate_directory(project_root, "inspect_project_root")?;
        let transactions_root = project_root.join(".ats").join("transactions");
        ensure_directory(project_root, &project_root.join(".ats"), "create_ats")?;
        ensure_directory(
            &project_root.join(".ats"),
            &transactions_root,
            "create_transactions",
        )?;
        cleanup_committed_transactions(&transactions_root)?;
        let transaction_root = transactions_root.join(run_id.as_str());
        fs::create_dir(&transaction_root).map_err(|error| io_error("create_transaction", error))?;
        let committed_root = transactions_root.join(format!(".committed-{}", run_id.as_str()));

        let mut transaction = FileProjectTransaction {
            transaction_root,
            committed_root,
            records: Vec::new(),
            created_directories: Vec::new(),
        };
        for (index, write) in writes.into_iter().enumerate() {
            if let Err(error) = transaction.apply_one(project_root, index, write) {
                let _ = transaction.rollback_in_place();
                return Err(error);
            }
        }
        Ok(Box::new(transaction))
    }
}

#[derive(Debug)]
struct WriteRecord {
    target: PathBuf,
    backup: Option<PathBuf>,
}

#[derive(Debug)]
struct FileProjectTransaction {
    transaction_root: PathBuf,
    committed_root: PathBuf,
    records: Vec<WriteRecord>,
    created_directories: Vec<PathBuf>,
}

impl FileProjectTransaction {
    fn apply_one(
        &mut self,
        project_root: &Path,
        index: usize,
        write: ProjectFileWrite,
    ) -> Result<(), ProjectWriteError> {
        let target = project_root.join(write.relative_path());
        let parent = target.parent().ok_or(ProjectWriteError::InvalidWrite)?;
        ensure_nested_directories(project_root, parent, &mut self.created_directories)?;
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                Some(metadata)
            }
            Ok(_) => return Err(ProjectWriteError::InvalidWrite),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error("inspect_target", error)),
        };

        let temp = parent.join(format!(".ats-write-{}-{index}.tmp", std::process::id()));
        match (write.bytes(), write.source_path()) {
            (Some(bytes), None) => write_new_file(&temp, bytes, "write_temporary")?,
            (None, Some(source)) => copy_new_file(source, &temp)?,
            _ => return Err(ProjectWriteError::InvalidWrite),
        }
        let backup = if metadata.is_some() {
            let backup = self.transaction_root.join(format!("backup-{index}"));
            if let Err(error) = fs::rename(&target, &backup) {
                let _ = fs::remove_file(&temp);
                return Err(io_error("backup_existing", error));
            }
            Some(backup)
        } else {
            None
        };

        if let Err(error) = fs::rename(&temp, &target) {
            if let Some(backup) = &backup {
                let _ = fs::rename(backup, &target);
            }
            let _ = fs::remove_file(&temp);
            return Err(io_error("publish_write", error));
        }
        self.records.push(WriteRecord { target, backup });
        Ok(())
    }

    fn rollback_in_place(&mut self) -> Result<(), ProjectWriteError> {
        let mut first_error = None;
        for record in self.records.drain(..).rev() {
            if let Err(error) = fs::remove_file(&record.target)
                && error.kind() != io::ErrorKind::NotFound
                && first_error.is_none()
            {
                first_error = Some(io_error("remove_generated", error));
            }
            if let Some(backup) = record.backup
                && let Err(error) = fs::rename(&backup, &record.target)
                && first_error.is_none()
            {
                first_error = Some(io_error("restore_backup", error));
            }
        }
        for directory in self.created_directories.drain(..).rev() {
            let _ = fs::remove_dir(directory);
        }
        if let Err(error) = fs::remove_dir_all(&self.transaction_root)
            && error.kind() != io::ErrorKind::NotFound
            && first_error.is_none()
        {
            first_error = Some(io_error("remove_transaction", error));
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl PendingProjectWrites for FileProjectTransaction {
    fn commit(mut self: Box<Self>) -> Result<(), ProjectWriteError> {
        if let Err(error) = fs::rename(&self.transaction_root, &self.committed_root) {
            let commit_error = io_error("commit_transaction", error);
            self.rollback_in_place()?;
            return Err(commit_error);
        }

        // The same-directory rename is the durable commit decision. Cleanup may be retried by the
        // next writer without changing the already-published project state.
        self.transaction_root.clone_from(&self.committed_root);
        self.records.clear();
        self.created_directories.clear();
        let _ = fs::remove_dir_all(&self.committed_root);
        Ok(())
    }

    fn rollback(mut self: Box<Self>) -> Result<(), ProjectWriteError> {
        self.rollback_in_place()
    }
}

fn cleanup_committed_transactions(transactions_root: &Path) -> Result<(), ProjectWriteError> {
    for entry in
        fs::read_dir(transactions_root).map_err(|error| io_error("list_transactions", error))?
    {
        let entry = entry.map_err(|error| io_error("read_transaction", error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with(".committed-") {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| io_error("inspect_committed_transaction", error))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ProjectWriteError::InvalidWrite);
        }
        fs::remove_dir_all(entry.path())
            .map_err(|error| io_error("cleanup_committed_transaction", error))?;
    }
    Ok(())
}

fn ensure_nested_directories(
    project_root: &Path,
    target_parent: &Path,
    created: &mut Vec<PathBuf>,
) -> Result<(), ProjectWriteError> {
    let relative = target_parent
        .strip_prefix(project_root)
        .map_err(|_| ProjectWriteError::InvalidWrite)?;
    let mut current = project_root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(ProjectWriteError::InvalidWrite),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| io_error("create_parent", error))?;
                created.push(current.clone());
            }
            Err(error) => return Err(io_error("inspect_parent", error)),
        }
    }
    Ok(())
}

fn ensure_directory(
    parent: &Path,
    path: &Path,
    operation: &'static str,
) -> Result<(), ProjectWriteError> {
    validate_directory(parent, "inspect_parent")?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(ProjectWriteError::InvalidWrite),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| io_error(operation, error))
        }
        Err(error) => Err(io_error(operation, error)),
    }
}

fn validate_directory(path: &Path, operation: &'static str) -> Result<(), ProjectWriteError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(ProjectWriteError::InvalidWrite),
        Err(error) => Err(io_error(operation, error)),
    }
}

fn write_new_file(
    path: &Path,
    bytes: &[u8],
    operation: &'static str,
) -> Result<(), ProjectWriteError> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| io_error(operation, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| io_error(operation, error))
}

fn copy_new_file(source: &Path, target: &Path) -> Result<(), ProjectWriteError> {
    use std::io::Write;

    let metadata =
        fs::symlink_metadata(source).map_err(|error| io_error("inspect_source", error))?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > 512 * 1024 * 1024
    {
        return Err(ProjectWriteError::InvalidWrite);
    }
    let mut input = fs::File::open(source).map_err(|error| io_error("open_source", error))?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)
        .map_err(|error| io_error("create_temporary", error))?;
    io::copy(&mut input, &mut output).map_err(|error| io_error("copy_source", error))?;
    output
        .flush()
        .and_then(|()| output.sync_all())
        .map_err(|error| io_error("sync_temporary", error))
}

fn io_error(operation: &'static str, source: io::Error) -> ProjectWriteError {
    ProjectWriteError::Io {
        operation,
        kind: source.kind(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_keeps_new_bytes_and_removes_transaction_state() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join("Generated")).unwrap();
        fs::write(project.join("Generated/One.cs"), b"old").unwrap();
        let run_id = RunId::new();
        let pending = FileProjectWriter
            .apply(
                &project,
                &run_id,
                vec![ProjectFileWrite::new("Generated/One.cs", b"new".to_vec()).unwrap()],
            )
            .unwrap();
        assert_eq!(fs::read(project.join("Generated/One.cs")).unwrap(), b"new");
        pending.commit().unwrap();
        assert!(
            !project
                .join(".ats/transactions")
                .join(run_id.as_str())
                .exists()
        );
    }

    #[test]
    fn rollback_restores_existing_and_removes_new_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join("Generated")).unwrap();
        fs::write(project.join("Generated/Existing.cs"), b"old").unwrap();
        let run_id = RunId::new();
        let pending = FileProjectWriter
            .apply(
                &project,
                &run_id,
                vec![
                    ProjectFileWrite::new("Generated/Existing.cs", b"new".to_vec()).unwrap(),
                    ProjectFileWrite::new("Nested/New.cs", b"created".to_vec()).unwrap(),
                ],
            )
            .unwrap();
        pending.rollback().unwrap();
        assert_eq!(
            fs::read(project.join("Generated/Existing.cs")).unwrap(),
            b"old"
        );
        assert!(!project.join("Nested/New.cs").exists());
        assert!(!project.join("Nested").exists());
    }

    #[test]
    fn commit_decision_failure_rolls_back_before_returning_error() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join("Generated")).unwrap();
        fs::write(project.join("Generated/One.cs"), b"old").unwrap();
        let run_id = RunId::new();
        let pending = FileProjectWriter
            .apply(
                &project,
                &run_id,
                vec![ProjectFileWrite::new("Generated/One.cs", b"new".to_vec()).unwrap()],
            )
            .unwrap();
        let blocker = project
            .join(".ats/transactions")
            .join(format!(".committed-{}", run_id.as_str()));
        fs::create_dir(&blocker).unwrap();
        fs::write(blocker.join("blocker"), b"occupied").unwrap();

        assert!(pending.commit().is_err());
        assert_eq!(fs::read(project.join("Generated/One.cs")).unwrap(), b"old");
        assert!(blocker.exists());
        assert!(
            !project
                .join(".ats/transactions")
                .join(run_id.as_str())
                .exists()
        );
    }

    #[test]
    fn source_file_write_is_streamed_through_the_same_transaction() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let source = temp.path().join("package.zip");
        fs::write(&source, b"package-bytes").unwrap();
        let pending = FileProjectWriter
            .apply(
                &project,
                &RunId::new(),
                vec![ProjectFileWrite::from_source("packages/mod.zip", source).unwrap()],
            )
            .unwrap();
        assert_eq!(
            fs::read(project.join("packages/mod.zip")).unwrap(),
            b"package-bytes"
        );
        pending.commit().unwrap();
    }
}
