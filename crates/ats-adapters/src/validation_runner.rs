use std::fs;
use std::io;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use ats_runtime::{
    CancellationToken, ValidationError, ValidationReport, ValidationRequest, ValidationRunner,
};
use tokio::process::Command;

#[derive(Debug, Default)]
pub struct RegisteredValidationRunner;

#[async_trait]
impl ValidationRunner for RegisteredValidationRunner {
    async fn validate(
        &self,
        request: ValidationRequest,
        cancellation: &CancellationToken,
    ) -> Result<ValidationReport, ValidationError> {
        if request.primitive.as_str() != "code.dotnet-validate" {
            return Err(ValidationError::UnknownPrimitive);
        }
        if cancellation.is_cancelled() {
            return Err(ValidationError::Cancelled);
        }
        let metadata = fs::symlink_metadata(&request.project_root)
            .map_err(|error| unavailable("inspect_project", error))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(ValidationError::Unavailable {
                operation: "inspect_project",
                kind: io::ErrorKind::InvalidInput,
            });
        }

        let output_root = request
            .project_root
            .join(".ats")
            .join("validation")
            .join(request.run_id.as_str());
        fs::create_dir_all(&output_root).map_err(|error| unavailable("create_output", error))?;
        let stdout_path = output_root.join("stdout.log");
        let stderr_path = output_root.join("stderr.log");
        let stdout_file =
            fs::File::create(&stdout_path).map_err(|error| unavailable("create_stdout", error))?;
        let stderr_file =
            fs::File::create(&stderr_path).map_err(|error| unavailable("create_stderr", error))?;

        let mut child = Command::new("dotnet")
            .args(["build", "--nologo"])
            .current_dir(&request.project_root)
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| unavailable("spawn", error))?;

        let status = loop {
            if cancellation.is_cancelled() {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = fs::remove_dir_all(&output_root);
                return Err(ValidationError::Cancelled);
            }
            match child
                .try_wait()
                .map_err(|error| unavailable("wait", error))?
            {
                Some(status) => break status,
                None => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        };
        let stdout =
            fs::read_to_string(&stdout_path).map_err(|error| unavailable("read_stdout", error))?;
        let stderr =
            fs::read_to_string(&stderr_path).map_err(|error| unavailable("read_stderr", error))?;
        let _ = fs::remove_dir_all(&output_root);
        let report = ValidationReport {
            exit_code: status.code().unwrap_or(-1),
            stdout_tail: sanitize_tail(&stdout, &request.project_root.to_string_lossy()),
            stderr_tail: sanitize_tail(&stderr, &request.project_root.to_string_lossy()),
        };
        report.validate()?;
        if status.success() {
            Ok(report)
        } else {
            Err(ValidationError::Rejected(report))
        }
    }
}

fn sanitize_tail(value: &str, project_root: &str) -> String {
    let redacted = value.replace(project_root, "<project>");
    let count = redacted.chars().count();
    if count <= 8_000 {
        redacted
    } else {
        redacted.chars().skip(count - 8_000).collect()
    }
}

fn unavailable(operation: &'static str, error: io::Error) -> ValidationError {
    ValidationError::Unavailable {
        operation,
        kind: error.kind(),
    }
}

#[cfg(test)]
mod tests {
    use ats_kernel::PrimitiveId;

    use super::*;

    #[tokio::test]
    async fn rejects_unknown_primitive_before_process_work() {
        let request = ValidationRequest {
            primitive: PrimitiveId::parse("code.unknown").unwrap(),
            project_root: std::path::PathBuf::from("missing"),
            run_id: ats_runtime::RunId::new(),
        };
        assert!(matches!(
            RegisteredValidationRunner
                .validate(request, &CancellationToken::new())
                .await,
            Err(ValidationError::UnknownPrimitive)
        ));
    }

    #[tokio::test]
    async fn real_registered_compile_accepts_and_rejects_isolated_projects() {
        for (source, succeeds) in [
            ("public class Valid {}", true),
            ("public class Invalid {", false),
        ] {
            let temp = tempfile::TempDir::new().unwrap();
            let project = temp.path().join("project");
            fs::create_dir(&project).unwrap();
            fs::write(
                project.join("Fixture.csproj"),
                br#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>"#,
            )
            .unwrap();
            fs::write(project.join("Generated.cs"), source).unwrap();
            let result = RegisteredValidationRunner
                .validate(
                    ValidationRequest {
                        primitive: PrimitiveId::parse("code.dotnet-validate").unwrap(),
                        project_root: project.clone(),
                        run_id: ats_runtime::RunId::new(),
                    },
                    &CancellationToken::new(),
                )
                .await;
            assert_eq!(result.is_ok(), succeeds);
            if let Err(ValidationError::Rejected(report)) = result {
                assert!(!report.stderr_tail.contains(&project.display().to_string()));
                assert!(!report.stdout_tail.contains(&project.display().to_string()));
            }
            assert!(
                !project
                    .join(".ats/validation")
                    .read_dir()
                    .is_ok_and(|mut entries| entries.next().is_some())
            );
        }
    }
}
