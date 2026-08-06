use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ats_runtime::{PendingProjectStage, ProjectStageError, ProjectStageRequest, ProjectStager};
use walkdir::{DirEntry, WalkDir};

const MAX_STAGE_FILES: usize = 50_000;
const MAX_STAGE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct FileProjectStager;

impl ProjectStager for FileProjectStager {
    fn stage(
        &self,
        request: ProjectStageRequest,
    ) -> Result<Box<dyn PendingProjectStage>, ProjectStageError> {
        validate_directory(&request.project_root)?;
        let ats_root = request.project_root.join(".ats");
        ensure_directory(&request.project_root, &ats_root)?;
        let stages_root = ats_root.join("composition-staging");
        ensure_directory(&ats_root, &stages_root)?;
        let stage_root = stages_root.join(request.run_id.as_str());
        fs::create_dir(&stage_root).map_err(|error| io_error("create_stage", error))?;

        let result = copy_project(&request.project_root, &stage_root);
        if let Err(error) = result {
            let _ = fs::remove_dir_all(&stage_root);
            return Err(error);
        }
        Ok(Box::new(FileProjectStage {
            root: stage_root,
            stages_root,
            active: true,
        }))
    }
}

struct FileProjectStage {
    root: PathBuf,
    stages_root: PathBuf,
    active: bool,
}

impl PendingProjectStage for FileProjectStage {
    fn root(&self) -> &Path {
        &self.root
    }

    fn cleanup(mut self: Box<Self>) -> Result<(), ProjectStageError> {
        let result = remove_stage(&self.root, &self.stages_root);
        if result.is_ok() {
            self.active = false;
        }
        result
    }
}

impl Drop for FileProjectStage {
    fn drop(&mut self) {
        if self.active {
            let _ = remove_stage(&self.root, &self.stages_root);
        }
    }
}

fn copy_project(source: &Path, target: &Path) -> Result<(), ProjectStageError> {
    let mut file_count = 0_usize;
    let mut byte_count = 0_u64;
    for entry in WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| include_entry(source, entry))
    {
        let entry = entry.map_err(|error| {
            error
                .into_io_error()
                .map_or(ProjectStageError::InvalidSource, |error| {
                    io_error("walk_source", error)
                })
        })?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| ProjectStageError::InvalidSource)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| io_error("inspect_source", error))?;
        if metadata.file_type().is_symlink() {
            return Err(ProjectStageError::InvalidSource);
        }
        let destination = target.join(relative);
        if metadata.is_dir() {
            fs::create_dir(&destination)
                .map_err(|error| io_error("create_stage_directory", error))?;
            continue;
        }
        if !metadata.is_file() {
            return Err(ProjectStageError::InvalidSource);
        }
        file_count = file_count.saturating_add(1);
        byte_count = byte_count.saturating_add(metadata.len());
        if file_count > MAX_STAGE_FILES || byte_count > MAX_STAGE_BYTES {
            return Err(ProjectStageError::LimitExceeded);
        }
        fs::copy(entry.path(), &destination).map_err(|error| io_error("copy_stage_file", error))?;
    }
    Ok(())
}

fn include_entry(root: &Path, entry: &DirEntry) -> bool {
    if entry.path() == root {
        return true;
    }
    let Ok(relative) = entry.path().strip_prefix(root) else {
        return false;
    };
    if relative.components().count() != 1 || !entry.file_type().is_dir() {
        return true;
    }
    !matches!(
        entry.file_name().to_str(),
        Some(".ats" | ".git" | ".godot" | "artifacts" | "delivery" | "dist" | "target")
    )
}

fn ensure_directory(parent: &Path, path: &Path) -> Result<(), ProjectStageError> {
    validate_directory(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(ProjectStageError::InvalidSource),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(path).map_err(|error| io_error("create_stage_parent", error))
        }
        Err(error) => Err(io_error("inspect_stage_parent", error)),
    }
}

fn validate_directory(path: &Path) -> Result<(), ProjectStageError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("inspect_project", error))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        Ok(())
    } else {
        Err(ProjectStageError::InvalidSource)
    }
}

fn remove_stage(root: &Path, stages_root: &Path) -> Result<(), ProjectStageError> {
    match fs::remove_dir_all(root) {
        Ok(()) => {
            let _ = fs::remove_dir(stages_root);
            Ok(())
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove_stage", error)),
    }
}

fn io_error(operation: &'static str, source: io::Error) -> ProjectStageError {
    ProjectStageError::Io {
        operation,
        kind: source.kind(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ats_runtime::RunId;

    #[test]
    fn stage_copies_sources_and_excludes_mutable_evidence_roots() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("Fixture.csproj"), b"project").unwrap();
        fs::create_dir(project.join("Generated")).unwrap();
        fs::write(project.join("Generated/Old.cs"), b"old").unwrap();
        fs::create_dir(project.join("artifacts")).unwrap();
        fs::write(project.join("artifacts/evidence.json"), b"evidence").unwrap();
        fs::create_dir(project.join("delivery")).unwrap();
        fs::write(project.join("delivery/stale.dll"), b"stale").unwrap();

        let stage = FileProjectStager
            .stage(ProjectStageRequest {
                project_root: project.clone(),
                run_id: RunId::new(),
            })
            .unwrap();
        assert!(stage.root().join("Fixture.csproj").is_file());
        assert!(stage.root().join("Generated/Old.cs").is_file());
        assert!(!stage.root().join("artifacts").exists());
        assert!(!stage.root().join("delivery").exists());
        let root = stage.root().to_path_buf();
        stage.cleanup().unwrap();
        assert!(!root.exists());
    }
}
