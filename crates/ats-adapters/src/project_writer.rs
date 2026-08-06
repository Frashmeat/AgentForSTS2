use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ats_runtime::{
    PendingProjectWrites, ProjectFileWrite, ProjectFileWriter, ProjectWriteError, RunId,
    normalize_relative_path, validate_project_writes,
};
use serde::{Deserialize, Serialize};

const TRANSACTION_RECORD_SCHEMA_VERSION: u32 = 1;

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
        recover_transactions(project_root, &transactions_root)?;
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

impl FileProjectWriter {
    pub fn recover(project_root: &Path) -> Result<(), ProjectWriteError> {
        validate_directory(project_root, "inspect_project_root")?;
        let transactions_root = project_root.join(".ats/transactions");
        match fs::symlink_metadata(&transactions_root) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                recover_transactions(project_root, &transactions_root)
            }
            Ok(_) => Err(ProjectWriteError::InvalidWrite),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error("inspect_transactions", error)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransactionWriteRecord {
    schema_version: u32,
    relative_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backup_file: Option<String>,
    temporary_relative_path: String,
    created_directories: Vec<String>,
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
        let relative_path = write.relative_path().to_owned();
        let target = project_root.join(&relative_path);
        let parent = target.parent().ok_or(ProjectWriteError::InvalidWrite)?;
        let metadata = match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                Some(metadata)
            }
            Ok(_) => return Err(ProjectWriteError::InvalidWrite),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error("inspect_target", error)),
        };

        let temporary_name = format!(".ats-write-{}-{index}.tmp", std::process::id());
        let temp = parent.join(&temporary_name);
        let temporary_relative_path = parent
            .strip_prefix(project_root)
            .map_err(|_| ProjectWriteError::InvalidWrite)?
            .join(&temporary_name);
        let planned_directories = planned_nested_directories(project_root, parent)?;
        let record = TransactionWriteRecord {
            schema_version: TRANSACTION_RECORD_SCHEMA_VERSION,
            relative_path,
            backup_file: metadata.is_some().then(|| format!("backup-{index}")),
            temporary_relative_path: normalized(&temporary_relative_path)?,
            created_directories: planned_directories
                .iter()
                .map(|path| {
                    path.strip_prefix(project_root)
                        .map_err(|_| ProjectWriteError::InvalidWrite)
                        .and_then(normalized)
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        persist_transaction_record(&self.transaction_root, index, &record)?;
        create_planned_directories(&planned_directories, &mut self.created_directories)?;
        match (write.bytes(), write.source_path()) {
            (Some(bytes), None) => write_new_file(&temp, bytes, "write_temporary")?,
            (None, Some(source)) => copy_new_file(source, &temp)?,
            _ => return Err(ProjectWriteError::InvalidWrite),
        }
        let backup = if metadata.is_some() {
            let backup = self
                .transaction_root
                .join(record.backup_file.as_deref().unwrap_or_default());
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

fn recover_transactions(
    project_root: &Path,
    transactions_root: &Path,
) -> Result<(), ProjectWriteError> {
    for entry in
        fs::read_dir(transactions_root).map_err(|error| io_error("list_transactions", error))?
    {
        let entry = entry.map_err(|error| io_error("read_transaction", error))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| io_error("inspect_transaction", error))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ProjectWriteError::InvalidWrite);
        }
        if name.starts_with(".committed-") {
            fs::remove_dir_all(entry.path())
                .map_err(|error| io_error("cleanup_committed_transaction", error))?;
        } else if name.starts_with('.') || name.is_empty() {
            return Err(ProjectWriteError::InvalidWrite);
        } else {
            recover_prepared_transaction(project_root, &entry.path())?;
        }
    }
    Ok(())
}

fn persist_transaction_record(
    transaction_root: &Path,
    index: usize,
    record: &TransactionWriteRecord,
) -> Result<(), ProjectWriteError> {
    validate_transaction_record(record)?;
    let bytes = serde_json::to_vec_pretty(record).map_err(|_| ProjectWriteError::InvalidWrite)?;
    write_new_file(
        &transaction_root.join(format!("record-{index}.json")),
        &bytes,
        "write_transaction_record",
    )
}

fn recover_prepared_transaction(
    project_root: &Path,
    transaction_root: &Path,
) -> Result<(), ProjectWriteError> {
    let mut records = Vec::new();
    let mut known_backups = std::collections::BTreeSet::new();
    let mut relative_paths = std::collections::BTreeSet::new();
    let mut temporary_paths = std::collections::BTreeSet::new();
    let mut declared_backup_names = std::collections::BTreeSet::new();
    for entry in fs::read_dir(transaction_root)
        .map_err(|error| io_error("list_transaction_records", error))?
    {
        let entry = entry.map_err(|error| io_error("read_transaction_record", error))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| io_error("inspect_transaction_record", error))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(ProjectWriteError::InvalidWrite);
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(ProjectWriteError::InvalidWrite);
        };
        if name.starts_with("record-") && name.ends_with(".json") {
            let bytes = fs::read(entry.path())
                .map_err(|error| io_error("read_transaction_record", error))?;
            let record = serde_json::from_slice::<TransactionWriteRecord>(&bytes)
                .map_err(|_| ProjectWriteError::InvalidWrite)?;
            validate_transaction_record(&record)?;
            if !relative_paths.insert(record.relative_path.clone())
                || !temporary_paths.insert(record.temporary_relative_path.clone())
                || record
                    .backup_file
                    .as_ref()
                    .is_some_and(|backup| !declared_backup_names.insert(backup.clone()))
            {
                return Err(ProjectWriteError::InvalidWrite);
            }
            records.push(record);
        } else if name.starts_with("backup-") {
            known_backups.insert(name.to_owned());
        } else {
            return Err(ProjectWriteError::InvalidWrite);
        }
    }
    records.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if known_backups
        .iter()
        .any(|backup| !declared_backup_names.contains(backup))
    {
        return Err(ProjectWriteError::InvalidWrite);
    }

    for record in records.iter().rev() {
        remove_optional_regular_file(
            &project_root.join(&record.temporary_relative_path),
            "remove_recovery_temporary",
        )?;
        let target = project_root.join(&record.relative_path);
        if let Some(backup_name) = &record.backup_file {
            let backup = transaction_root.join(backup_name);
            match fs::symlink_metadata(&backup) {
                Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                    remove_optional_regular_file(&target, "remove_recovery_target")?;
                    fs::rename(&backup, &target)
                        .map_err(|error| io_error("restore_recovery_backup", error))?;
                }
                Ok(_) => return Err(ProjectWriteError::InvalidWrite),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error("inspect_recovery_backup", error)),
            }
        } else {
            remove_optional_regular_file(&target, "remove_recovery_target")?;
        }
    }

    let mut created_directories = records
        .iter()
        .flat_map(|record| record.created_directories.iter())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    created_directories.sort_by_key(|path| std::cmp::Reverse(path.matches('/').count()));
    for relative in created_directories {
        let directory = project_root.join(relative);
        if let Err(error) = fs::remove_dir(&directory)
            && !matches!(
                error.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::DirectoryNotEmpty
            )
        {
            return Err(io_error("remove_recovery_directory", error));
        }
    }
    fs::remove_dir_all(transaction_root)
        .map_err(|error| io_error("remove_recovered_transaction", error))?;
    Ok(())
}

fn validate_transaction_record(record: &TransactionWriteRecord) -> Result<(), ProjectWriteError> {
    if record.schema_version != TRANSACTION_RECORD_SCHEMA_VERSION
        || normalized(Path::new(&record.relative_path))? != record.relative_path
        || record.relative_path.starts_with(".ats/")
        || normalized(Path::new(&record.temporary_relative_path))? != record.temporary_relative_path
        || Path::new(&record.temporary_relative_path).parent()
            != Path::new(&record.relative_path).parent()
        || Path::new(&record.temporary_relative_path)
            .file_name()
            .and_then(|value| value.to_str())
            .is_none_or(|name| !name.starts_with(".ats-write-") || !name.ends_with(".tmp"))
        || record.backup_file.as_ref().is_some_and(|name| {
            !name.starts_with("backup-")
                || name.len() > 32
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(ProjectWriteError::InvalidWrite);
    }
    let target_parent = Path::new(&record.relative_path).parent();
    for directory in &record.created_directories {
        if normalized(Path::new(directory))? != *directory
            || directory.starts_with(".ats/")
            || target_parent.is_none_or(|parent| !parent.starts_with(directory))
        {
            return Err(ProjectWriteError::InvalidWrite);
        }
    }
    Ok(())
}

fn remove_optional_regular_file(
    path: &Path,
    operation: &'static str,
) -> Result<(), ProjectWriteError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::remove_file(path).map_err(|error| io_error(operation, error))
        }
        Ok(_) => Err(ProjectWriteError::InvalidWrite),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(operation, error)),
    }
}

fn normalized(path: &Path) -> Result<String, ProjectWriteError> {
    Ok(normalize_relative_path(path)?)
}

fn planned_nested_directories(
    project_root: &Path,
    target_parent: &Path,
) -> Result<Vec<PathBuf>, ProjectWriteError> {
    let relative = target_parent
        .strip_prefix(project_root)
        .map_err(|_| ProjectWriteError::InvalidWrite)?;
    let mut current = project_root.to_path_buf();
    let mut planned = Vec::new();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(ProjectWriteError::InvalidWrite),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                planned.push(current.clone());
            }
            Err(error) => return Err(io_error("inspect_parent", error)),
        }
    }
    Ok(planned)
}

fn create_planned_directories(
    planned: &[PathBuf],
    created: &mut Vec<PathBuf>,
) -> Result<(), ProjectWriteError> {
    for directory in planned {
        fs::create_dir(directory).map_err(|error| io_error("create_parent", error))?;
        created.push(directory.clone());
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

    #[test]
    fn prepared_process_stop_is_rolled_back_before_the_project_is_reused() {
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
        drop(pending);

        assert_eq!(
            fs::read(project.join("Generated/Existing.cs")).unwrap(),
            b"new"
        );
        assert!(project.join("Nested/New.cs").is_file());
        FileProjectWriter::recover(&project).unwrap();
        assert_eq!(
            fs::read(project.join("Generated/Existing.cs")).unwrap(),
            b"old"
        );
        assert!(!project.join("Nested/New.cs").exists());
        assert!(!project.join("Nested").exists());
        assert!(
            !project
                .join(".ats/transactions")
                .read_dir()
                .unwrap()
                .any(|_| true)
        );
    }

    #[test]
    fn committed_process_stop_keeps_published_bytes_and_cleans_only_journal_state() {
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
        let transactions = project.join(".ats/transactions");
        fs::rename(
            transactions.join(run_id.as_str()),
            transactions.join(format!(".committed-{}", run_id.as_str())),
        )
        .unwrap();
        drop(pending);

        FileProjectWriter::recover(&project).unwrap();
        assert_eq!(fs::read(project.join("Generated/One.cs")).unwrap(), b"new");
        assert!(!transactions.read_dir().unwrap().any(|_| true));
    }

    #[test]
    fn recovery_rejects_duplicate_write_records_without_mutating_the_project() {
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
        let transaction = project.join(".ats/transactions").join(run_id.as_str());
        fs::copy(
            transaction.join("record-0.json"),
            transaction.join("record-1.json"),
        )
        .unwrap();
        drop(pending);

        assert!(matches!(
            FileProjectWriter::recover(&project),
            Err(ProjectWriteError::InvalidWrite)
        ));
        assert_eq!(fs::read(project.join("Generated/One.cs")).unwrap(), b"new");
        assert!(transaction.exists());
    }
}
