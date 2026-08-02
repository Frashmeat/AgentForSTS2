use std::fs;
use std::io;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use ats_runtime::{BuildError, BuildRunner, BuildStepReport, BuildStepRequest, CancellationToken};
use tokio::process::Command;

#[derive(Debug, Default)]
pub struct RegisteredBuildRunner;

#[async_trait]
impl BuildRunner for RegisteredBuildRunner {
    async fn run_step(
        &self,
        request: BuildStepRequest,
        cancellation: &CancellationToken,
    ) -> Result<BuildStepReport, BuildError> {
        if request.primitive.as_str() != "process.dotnet-publish" {
            return Err(BuildError::UnknownPrimitive);
        }
        if cancellation.is_cancelled() {
            return Err(BuildError::Cancelled);
        }
        let metadata = fs::symlink_metadata(&request.project_root)
            .map_err(|error| unavailable("inspect_project", error))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(BuildError::Unavailable {
                operation: "inspect_project",
                kind: io::ErrorKind::InvalidInput,
            });
        }

        let output_root = request
            .project_root
            .join(".ats")
            .join("build-reports")
            .join(request.run_id.as_str());
        fs::create_dir_all(&output_root).map_err(|error| unavailable("create_output", error))?;
        let stdout_path = output_root.join("stdout.log");
        let stderr_path = output_root.join("stderr.log");
        let result = async {
            let stdout_file = fs::File::create(&stdout_path)
                .map_err(|error| unavailable("create_stdout", error))?;
            let stderr_file = fs::File::create(&stderr_path)
                .map_err(|error| unavailable("create_stderr", error))?;
            let mut child = Command::new("dotnet")
                .args(["publish", "--nologo"])
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
                    return Err(BuildError::Cancelled);
                }
                match child
                    .try_wait()
                    .map_err(|error| unavailable("wait", error))?
                {
                    Some(status) => break status,
                    None => tokio::time::sleep(Duration::from_millis(25)).await,
                }
            };
            let stdout = fs::read_to_string(&stdout_path)
                .map_err(|error| unavailable("read_stdout", error))?;
            let stderr = fs::read_to_string(&stderr_path)
                .map_err(|error| unavailable("read_stderr", error))?;
            let report = BuildStepReport {
                primitive: request.primitive,
                exit_code: status.code().unwrap_or(-1),
                stdout_tail: sanitize_tail(&stdout, &request.project_root.to_string_lossy()),
                stderr_tail: sanitize_tail(&stderr, &request.project_root.to_string_lossy()),
            };
            report.validate()?;
            if status.success() {
                Ok(report)
            } else {
                Err(BuildError::Rejected(report))
            }
        }
        .await;
        let _ = fs::remove_dir_all(&output_root);
        result
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

fn unavailable(operation: &'static str, error: io::Error) -> BuildError {
    BuildError::Unavailable {
        operation,
        kind: error.kind(),
    }
}

#[cfg(test)]
mod tests {
    use ats_kernel::PrimitiveId;

    use super::*;

    #[tokio::test]
    async fn registered_publish_accepts_fixture_and_rejects_unknown_primitive() {
        let unknown = RegisteredBuildRunner
            .run_step(
                BuildStepRequest {
                    primitive: PrimitiveId::parse("process.unknown").unwrap(),
                    project_root: "missing".into(),
                    run_id: ats_runtime::RunId::new(),
                },
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(unknown, Err(BuildError::UnknownPrimitive)));

        let temp = tempfile::TempDir::new().unwrap();
        fs::write(
            temp.path().join("Fixture.csproj"),
            br#"<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>"#,
        )
        .unwrap();
        fs::write(temp.path().join("Fixture.cs"), "public class Fixture {}").unwrap();
        let run_id = ats_runtime::RunId::new();
        let report = RegisteredBuildRunner
            .run_step(
                BuildStepRequest {
                    primitive: PrimitiveId::parse("process.dotnet-publish").unwrap(),
                    project_root: temp.path().to_path_buf(),
                    run_id: run_id.clone(),
                },
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(report.exit_code, 0);
        assert!(
            !report
                .stdout_tail
                .contains(&temp.path().display().to_string())
        );
        assert!(
            !temp
                .path()
                .join(".ats/build-reports")
                .join(run_id.as_str())
                .exists()
        );
    }

    #[tokio::test]
    async fn rejected_publish_removes_the_bounded_report_directory() {
        let temp = tempfile::TempDir::new().unwrap();
        fs::write(temp.path().join("invalid.csproj"), "not an MSBuild project").unwrap();
        let run_id = ats_runtime::RunId::new();
        let result = RegisteredBuildRunner
            .run_step(
                BuildStepRequest {
                    primitive: PrimitiveId::parse("process.dotnet-publish").unwrap(),
                    project_root: temp.path().to_path_buf(),
                    run_id: run_id.clone(),
                },
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(result, Err(BuildError::Rejected(_))));
        assert!(
            !temp
                .path()
                .join(".ats/build-reports")
                .join(run_id.as_str())
                .exists()
        );
    }
}
