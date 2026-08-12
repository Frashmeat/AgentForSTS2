use std::fs;
use std::io;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use async_trait::async_trait;
use ats_runtime::{
    CancellationToken, ValidationError, ValidationIssue, ValidationIssueRepairability,
    ValidationIssueSeverity, ValidationReport, ValidationRequest, ValidationRunner,
};
use regex::Regex;
use sha2::{Digest, Sha256};
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
            issues: parse_validation_issues(&stdout, &stderr, &request.project_root),
        };
        report.validate()?;
        if status.success() {
            Ok(report)
        } else {
            Err(ValidationError::Rejected(report))
        }
    }
}

fn parse_validation_issues(
    stdout: &str,
    stderr: &str,
    project_root: &std::path::Path,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for line in stdout.lines().chain(stderr.lines()) {
        if issues.len() == 256 {
            break;
        }
        let issue =
            parse_environment_issue(line).or_else(|| parse_msbuild_issue(line, project_root));
        if let Some(issue) = issue
            && !issues
                .iter()
                .any(|existing: &ValidationIssue| existing.fingerprint == issue.fingerprint)
        {
            issues.push(issue);
        }
    }
    issues
}

fn parse_environment_issue(line: &str) -> Option<ValidationIssue> {
    let (code, message) = if line.contains("Slay the Spire 2 data not found at path") {
        (
            "local.sts2_data_missing",
            "The configured Slay the Spire 2 assembly path is unavailable.",
        )
    } else if line.contains("Godot not found at path") {
        (
            "local.godot_missing",
            "The configured Godot executable path is unavailable.",
        )
    } else {
        return None;
    };
    Some(issue(
        code,
        ValidationIssueSeverity::Error,
        None,
        None,
        None,
        message,
        None,
        ValidationIssueRepairability::LocalEnvironment,
    ))
}

fn parse_msbuild_issue(line: &str, project_root: &std::path::Path) -> Option<ValidationIssue> {
    static DIAGNOSTIC: OnceLock<Regex> = OnceLock::new();
    let captures = DIAGNOSTIC
        .get_or_init(|| {
            Regex::new(
                r"^(?P<path>.+?)\((?P<line>\d+),(?P<column>\d+)\): (?P<severity>error|warning) (?P<code>[A-Za-z]+\d+): (?P<message>.*?)(?: \[[^\]]+\])?$",
            )
            .expect("MSBuild diagnostic regex is valid")
        })
        .captures(line.trim())?;
    let relative_path = normalized_diagnostic_path(&captures["path"], project_root)?;
    let repairability = if relative_path.starts_with("Generated/") {
        ValidationIssueRepairability::GeneratedContent
    } else {
        ValidationIssueRepairability::NonRepairable
    };
    let severity = match &captures["severity"] {
        "error" => ValidationIssueSeverity::Error,
        _ => ValidationIssueSeverity::Warning,
    };
    let message = sanitize_issue_message(&captures["message"], project_root);
    Some(issue(
        &captures["code"],
        severity,
        Some(relative_path),
        captures["line"].parse().ok(),
        captures["column"].parse().ok(),
        &message,
        extract_symbol(&message),
        repairability,
    ))
}

fn normalized_diagnostic_path(value: &str, project_root: &std::path::Path) -> Option<String> {
    let path = std::path::Path::new(value.trim());
    let relative = if path.is_absolute() {
        path.strip_prefix(project_root).ok()?
    } else {
        path
    };
    ats_runtime::normalize_relative_path(relative).ok()
}

fn sanitize_issue_message(value: &str, project_root: &std::path::Path) -> String {
    static WINDOWS_PATH: OnceLock<Regex> = OnceLock::new();
    static UNIX_PATH: OnceLock<Regex> = OnceLock::new();
    let redacted = value.replace(&project_root.to_string_lossy().to_string(), "<project>");
    let redacted = WINDOWS_PATH
        .get_or_init(|| Regex::new(r#"(?i)\b[A-Z]:[\\/][^\s'\"\]]+"#).unwrap())
        .replace_all(&redacted, "<path>");
    let redacted = UNIX_PATH
        .get_or_init(|| Regex::new(r#"(^|[\s'\"])/[^\s'\"\]]+"#).unwrap())
        .replace_all(&redacted, "$1<path>");
    redacted.chars().take(2_000).collect()
}

fn extract_symbol(message: &str) -> Option<String> {
    static SYMBOL: OnceLock<Regex> = OnceLock::new();
    let value = SYMBOL
        .get_or_init(|| Regex::new(r"'([A-Za-z_][A-Za-z0-9_.]{0,127})'").unwrap())
        .captures(message)?
        .get(1)?
        .as_str();
    Some(value.into())
}

#[allow(clippy::too_many_arguments)]
fn issue(
    code: &str,
    severity: ValidationIssueSeverity,
    relative_path: Option<String>,
    line: Option<u32>,
    column: Option<u32>,
    message: &str,
    symbol: Option<String>,
    repairability: ValidationIssueRepairability,
) -> ValidationIssue {
    let canonical = format!(
        "code.dotnet-validate\0{code}\0{severity:?}\0{}\0{}\0{}\0{message}\0{}\0{repairability:?}",
        relative_path.as_deref().unwrap_or_default(),
        line.map_or_else(String::new, |value| value.to_string()),
        column.map_or_else(String::new, |value| value.to_string()),
        symbol.as_deref().unwrap_or_default(),
    );
    ValidationIssue {
        validator_id: "code.dotnet-validate".into(),
        code: code.into(),
        severity,
        relative_path,
        line,
        column,
        message: message.into(),
        symbol,
        repairability,
        fingerprint: ats_kernel::Sha256Digest::parse(format!(
            "{:x}",
            Sha256::digest(canonical.as_bytes())
        ))
        .expect("SHA-256 formatter is valid"),
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
                assert!(!report.issues.is_empty());
            }
            assert!(
                !project
                    .join(".ats/validation")
                    .read_dir()
                    .is_ok_and(|mut entries| entries.next().is_some())
            );
        }
    }

    #[test]
    fn parses_generated_compiler_and_local_environment_issues_without_absolute_paths() {
        let project = std::path::Path::new(r"C:\fixture\project");
        let output = concat!(
            "C:\\fixture\\project\\Generated\\card.cs(12,29): error CS0246: The type or namespace name 'ImaginaryType' could not be found [C:\\fixture\\project\\Fixture.csproj]\n",
            "C:\\fixture\\project\\Fixture.csproj : error : Slay the Spire 2 data not found at path 'J:\\private'\n",
        );
        let issues = parse_validation_issues(output, "", project);
        assert_eq!(issues.len(), 2);
        assert_eq!(
            issues[0].relative_path.as_deref(),
            Some("Generated/card.cs")
        );
        assert_eq!(issues[0].line, Some(12));
        assert_eq!(issues[0].column, Some(29));
        assert_eq!(issues[0].symbol.as_deref(), Some("ImaginaryType"));
        assert_eq!(
            issues[0].repairability,
            ValidationIssueRepairability::GeneratedContent
        );
        assert_eq!(
            issues[1].repairability,
            ValidationIssueRepairability::LocalEnvironment
        );
        assert!(
            issues
                .iter()
                .all(|issue| !issue.message.contains("J:\\private"))
        );
    }

    #[test]
    fn diagnostic_fingerprints_are_stable_and_duplicate_lines_are_collapsed() {
        let project = std::path::Path::new(r"C:\fixture\project");
        let line = "C:\\fixture\\project\\Generated\\card.cs(1,2): error CS1002: ; expected";
        let issues = parse_validation_issues(&format!("{line}\n{line}"), "", project);
        assert_eq!(issues.len(), 1);
        assert_eq!(
            issues[0].fingerprint,
            parse_validation_issues(line, "", project)[0].fingerprint
        );
    }

    #[test]
    fn compiler_issue_messages_redact_absolute_paths_outside_the_project() {
        let project = std::path::Path::new(r"C:\fixture\project");
        let line = "C:\\fixture\\project\\Generated\\card.cs(1,2): error CS0001: See J:\\private\\sdk.dll and /opt/private/sdk.dll";
        let issues = parse_validation_issues(line, "", project);
        assert_eq!(issues.len(), 1);
        assert!(!issues[0].message.contains("J:\\private"));
        assert!(!issues[0].message.contains("/opt/private"));
    }
}
