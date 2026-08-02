use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use ats_runtime::{
    CancellationToken, PackageError, PackagePrepareRequest, PackageReport, PackageWriter,
    PendingPackageOutput, validate_package_request,
};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

#[derive(Debug, Default)]
pub struct ZipPackageWriter;

impl PackageWriter for ZipPackageWriter {
    fn prepare(
        &self,
        request: PackagePrepareRequest,
        cancellation: &CancellationToken,
    ) -> Result<Box<dyn PendingPackageOutput>, PackageError> {
        validate_package_request(&request)?;
        validate_directory(&request.project_root)?;
        if cancellation.is_cancelled() {
            return Err(PackageError::Cancelled);
        }
        let source_root = request.project_root.join(&request.source_relative_root);
        validate_directory(&source_root)?;
        let output = request.project_root.join(&request.output_relative_path);
        let parent = output.parent().ok_or(PackageError::InvalidOutput)?;
        let created_directories = ensure_nested_directories(&request.project_root, parent)?;
        let transaction_root = request
            .project_root
            .join(".ats")
            .join("package-transactions")
            .join(request.run_id.as_str());
        ensure_transaction_parent(&request.project_root, transaction_root.parent().unwrap())?;
        if let Err(error) = fs::create_dir(&transaction_root) {
            cleanup_transaction(&transaction_root, &created_directories);
            return Err(io_error("create_transaction", error));
        }
        let temporary = parent.join(format!(
            ".ats-package-{}-{}.tmp",
            request.run_id,
            std::process::id()
        ));
        let backup = if output.exists() {
            let metadata =
                fs::symlink_metadata(&output).map_err(|error| io_error("inspect_output", error))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                cleanup_transaction(&transaction_root, &created_directories);
                return Err(PackageError::InvalidOutput);
            }
            let backup = transaction_root.join("previous.package");
            if let Err(error) = fs::rename(&output, &backup) {
                cleanup_transaction(&transaction_root, &created_directories);
                return Err(io_error("backup_output", error));
            }
            Some(backup)
        } else {
            None
        };
        let result =
            write_archive(&temporary, &source_root, &request, cancellation).and_then(|report| {
                fs::rename(&temporary, &output)
                    .map_err(|error| io_error("publish_package", error))?;
                Ok(report)
            });
        match result {
            Ok(report) => Ok(Box::new(ZipPackageTransaction {
                output,
                output_relative_path: request.output_relative_path,
                backup,
                transaction_root,
                created_directories,
                report,
                active: true,
            })),
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                if let Some(backup) = &backup {
                    let _ = fs::rename(backup, &output);
                }
                cleanup_transaction(&transaction_root, &created_directories);
                Err(error)
            }
        }
    }
}

struct ZipPackageTransaction {
    output: PathBuf,
    output_relative_path: String,
    backup: Option<PathBuf>,
    transaction_root: PathBuf,
    created_directories: Vec<PathBuf>,
    report: PackageReport,
    active: bool,
}

impl PendingPackageOutput for ZipPackageTransaction {
    fn report(&self) -> &PackageReport {
        &self.report
    }

    fn output_path(&self) -> &Path {
        &self.output
    }

    fn output_relative_path(&self) -> &str {
        &self.output_relative_path
    }

    fn commit(mut self: Box<Self>) -> Result<(), PackageError> {
        let result = fs::remove_dir_all(&self.transaction_root)
            .map_err(|error| io_error("commit_transaction", error));
        self.active = false;
        result
    }

    fn rollback(mut self: Box<Self>) -> Result<(), PackageError> {
        let result = self.rollback_in_place();
        self.active = false;
        result
    }
}

impl ZipPackageTransaction {
    fn rollback_in_place(&mut self) -> Result<(), PackageError> {
        match fs::remove_file(&self.output) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("remove_package", error)),
        }
        if let Some(backup) = &self.backup {
            fs::rename(backup, &self.output).map_err(|error| io_error("restore_package", error))?;
        }
        cleanup_transaction(&self.transaction_root, &self.created_directories);
        Ok(())
    }
}

impl Drop for ZipPackageTransaction {
    fn drop(&mut self) {
        if self.active {
            let _ = self.rollback_in_place();
        }
    }
}

fn write_archive(
    temporary: &Path,
    source_root: &Path,
    request: &PackagePrepareRequest,
    cancellation: &CancellationToken,
) -> Result<PackageReport, PackageError> {
    let file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temporary)
        .map_err(|error| io_error("create_temporary", error))?;
    let mut archive = zip::ZipWriter::new(file);
    let mut options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    if let Some(level) = request.compression_level {
        options = options.compression_level(Some(i64::from(level)));
    }
    let mut buffer = [0_u8; 64 * 1024];
    let mut uncompressed_bytes = 0_u64;
    for entry in &request.entries {
        if cancellation.is_cancelled() {
            return Err(PackageError::Cancelled);
        }
        let source = source_root.join(entry.source_relative_path());
        validate_descendant_file(source_root, &source)?;
        archive
            .start_file(entry.archive_path(), options)
            .map_err(|_| PackageError::Archive)?;
        let mut input = fs::File::open(&source).map_err(|error| io_error("open_source", error))?;
        loop {
            if cancellation.is_cancelled() {
                return Err(PackageError::Cancelled);
            }
            let read = input
                .read(&mut buffer)
                .map_err(|error| io_error("read_source", error))?;
            if read == 0 {
                break;
            }
            archive
                .write_all(&buffer[..read])
                .map_err(|error| io_error("write_archive", error))?;
            uncompressed_bytes = uncompressed_bytes.saturating_add(read as u64);
        }
    }
    let file = archive.finish().map_err(|_| PackageError::Archive)?;
    file.sync_all()
        .map_err(|error| io_error("sync_archive", error))?;
    let package_bytes = file
        .metadata()
        .map_err(|error| io_error("inspect_archive", error))?
        .len();
    Ok(PackageReport {
        file_count: u32::try_from(request.entries.len())
            .map_err(|_| PackageError::InvalidRequest)?,
        uncompressed_bytes,
        package_bytes,
    })
}

fn ensure_transaction_parent(project_root: &Path, path: &Path) -> Result<(), PackageError> {
    let ats = project_root.join(".ats");
    ensure_directory(project_root, &ats)?;
    ensure_directory(&ats, path)
}

fn ensure_directory(parent: &Path, path: &Path) -> Result<(), PackageError> {
    validate_directory(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(PackageError::InvalidOutput),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| io_error("create_directory", error))
        }
        Err(error) => Err(io_error("inspect_directory", error)),
    }
}

fn ensure_nested_directories(root: &Path, target: &Path) -> Result<Vec<PathBuf>, PackageError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| PackageError::InvalidOutput)?;
    let mut current = root.to_path_buf();
    let mut created = Vec::new();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err(PackageError::InvalidOutput),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|error| io_error("create_output_parent", error))?;
                created.push(current.clone());
            }
            Err(error) => return Err(io_error("inspect_output_parent", error)),
        }
    }
    Ok(created)
}

fn validate_directory(path: &Path) -> Result<(), PackageError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("inspect_directory", error))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(PackageError::InvalidSource)
    }
}

fn validate_descendant_file(root: &Path, path: &Path) -> Result<(), PackageError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| PackageError::InvalidSource)?;
    let mut current = root.to_path_buf();
    for (index, component) in relative.components().enumerate() {
        current.push(component);
        let metadata = fs::symlink_metadata(&current).map_err(|_| PackageError::InvalidSource)?;
        if metadata.file_type().is_symlink()
            || (index + 1 < relative.components().count() && !metadata.is_dir())
        {
            return Err(PackageError::InvalidSource);
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| PackageError::InvalidSource)?;
    if metadata.is_file() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(PackageError::InvalidSource)
    }
}

fn cleanup_transaction(transaction_root: &Path, created_directories: &[PathBuf]) {
    let _ = fs::remove_dir_all(transaction_root);
    for directory in created_directories.iter().rev() {
        let _ = fs::remove_dir(directory);
    }
}

fn io_error(operation: &'static str, source: io::Error) -> PackageError {
    PackageError::Io {
        operation,
        kind: source.kind(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use ats_runtime::{PackageEntry, RunId};

    use super::*;

    #[test]
    fn package_commit_and_rollback_are_atomic() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join("delivery")).unwrap();
        fs::create_dir(project.join("packages")).unwrap();
        fs::write(project.join("delivery/mod.bin"), b"new-content").unwrap();
        let output = project.join("packages/mod.zip");
        fs::write(&output, b"previous").unwrap();
        let request = || PackagePrepareRequest {
            project_root: project.clone(),
            source_relative_root: "delivery".into(),
            output_relative_path: "packages/mod.zip".into(),
            run_id: RunId::new(),
            entries: vec![PackageEntry::new("mod.bin", "mod.bin").unwrap()],
            compression_level: Some(5),
        };
        let pending = ZipPackageWriter
            .prepare(request(), &CancellationToken::new())
            .unwrap();
        assert_ne!(fs::read(&output).unwrap(), b"previous");
        pending.rollback().unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"previous");

        let pending = ZipPackageWriter
            .prepare(request(), &CancellationToken::new())
            .unwrap();
        pending.commit().unwrap();
        assert_ne!(fs::read(&output).unwrap(), b"previous");
    }

    #[test]
    fn missing_source_and_cancellation_preserve_existing_output() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join("delivery")).unwrap();
        fs::create_dir(project.join("packages")).unwrap();
        let output = project.join("packages/mod.zip");
        fs::write(&output, b"previous").unwrap();
        let request = || PackagePrepareRequest {
            project_root: project.clone(),
            source_relative_root: "delivery".into(),
            output_relative_path: "packages/mod.zip".into(),
            run_id: RunId::new(),
            entries: vec![PackageEntry::new("missing.bin", "missing.bin").unwrap()],
            compression_level: None,
        };
        assert!(
            ZipPackageWriter
                .prepare(request(), &CancellationToken::new())
                .is_err()
        );
        assert_eq!(fs::read(&output).unwrap(), b"previous");

        let cancellation = CancellationToken::new();
        cancellation.cancel(ats_runtime::CancellationReason::User);
        assert!(matches!(
            ZipPackageWriter.prepare(request(), &cancellation),
            Err(PackageError::Cancelled)
        ));
        assert_eq!(fs::read(&output).unwrap(), b"previous");
    }

    #[test]
    fn dropping_pending_package_restores_output_and_removes_transaction() {
        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join("delivery")).unwrap();
        fs::write(project.join("delivery/mod.bin"), b"new-content").unwrap();
        let output = project.join("packages/mod.zip");
        fs::create_dir(output.parent().unwrap()).unwrap();
        fs::write(&output, b"previous").unwrap();
        let run_id = RunId::new();
        let pending = ZipPackageWriter
            .prepare(
                PackagePrepareRequest {
                    project_root: project.clone(),
                    source_relative_root: "delivery".into(),
                    output_relative_path: "packages/mod.zip".into(),
                    run_id: run_id.clone(),
                    entries: vec![PackageEntry::new("mod.bin", "mod.bin").unwrap()],
                    compression_level: None,
                },
                &CancellationToken::new(),
            )
            .unwrap();
        drop(pending);
        assert_eq!(fs::read(&output).unwrap(), b"previous");
        assert!(
            !project
                .join(".ats/package-transactions")
                .join(run_id.as_str())
                .exists()
        );
    }

    #[cfg(windows)]
    #[test]
    fn symlinked_package_source_is_rejected() {
        use std::os::windows::fs::symlink_file;

        let temp = tempfile::TempDir::new().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::create_dir(project.join("delivery")).unwrap();
        fs::create_dir(project.join("packages")).unwrap();
        let target = project.join("target.bin");
        fs::write(&target, b"target").unwrap();
        let link = project.join("delivery/link.bin");
        if symlink_file(&target, &link).is_err() {
            return;
        }
        let result = ZipPackageWriter.prepare(
            PackagePrepareRequest {
                project_root: project,
                source_relative_root: "delivery".into(),
                output_relative_path: "packages/mod.zip".into(),
                run_id: RunId::new(),
                entries: vec![PackageEntry::new("link.bin", "link.bin").unwrap()],
                compression_level: None,
            },
            &CancellationToken::new(),
        );
        assert!(matches!(result, Err(PackageError::InvalidSource)));
    }
}
