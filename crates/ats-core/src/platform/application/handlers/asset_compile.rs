use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Serialize;

use super::build_project::tail;
use crate::controlled_process::{ControlledProcessResult, run_controlled_process};
use crate::platform::application::CancellationToken;
use crate::platform::domain::RunId;
use crate::project_utils::to_extended_length_path;

pub(crate) const ASSET_COMPILE_DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompileValidation {
    pub exit_code: i32,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetCompileOperation {
    CreateOutput,
    Spawn,
    Cleanup,
}

impl AssetCompileOperation {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::CreateOutput => "create_output",
            Self::Spawn => "spawn",
            Self::Cleanup => "cleanup",
        }
    }
}

#[derive(Debug)]
pub(crate) enum AssetCompileError {
    Rejected(CompileValidation),
    Io {
        operation: AssetCompileOperation,
        kind: std::io::ErrorKind,
    },
    Cancelled,
}

#[async_trait]
pub(crate) trait AssetCompileValidator: Send + Sync {
    async fn validate(
        &self,
        project_root: &Path,
        run_id: &RunId,
        cancellation: &CancellationToken,
    ) -> Result<CompileValidation, AssetCompileError>;
}

#[derive(Debug, Default)]
pub(crate) struct DotnetAssetCompileValidator;

#[async_trait]
impl AssetCompileValidator for DotnetAssetCompileValidator {
    async fn validate(
        &self,
        project_root: &Path,
        run_id: &RunId,
        cancellation: &CancellationToken,
    ) -> Result<CompileValidation, AssetCompileError> {
        let mods_path = isolated_mods_path(project_root, run_id);
        tokio::fs::create_dir_all(&mods_path)
            .await
            .map_err(|error| AssetCompileError::Io {
                operation: AssetCompileOperation::CreateOutput,
                kind: error.kind(),
            })?;

        let cwd = to_extended_length_path(project_root);
        let mods_arg = format!(
            "-p:ModsPath={}",
            with_trailing_separator(&msbuild_property_path(&mods_path))
        );
        let args = vec![
            std::ffi::OsString::from("build"),
            std::ffi::OsString::from("--nologo"),
            std::ffi::OsString::from(mods_arg),
        ];
        let output_result =
            run_controlled_process(std::ffi::OsStr::new("dotnet"), &args, &cwd, cancellation).await;

        let cleanup_result = tokio::fs::remove_dir_all(&mods_path).await;
        let output = match output_result {
            Ok(ControlledProcessResult::Completed(output)) => output,
            Ok(ControlledProcessResult::Cancelled(_)) => return Err(AssetCompileError::Cancelled),
            Err(error) => {
                return Err(AssetCompileError::Io {
                    operation: AssetCompileOperation::Spawn,
                    kind: error.kind(),
                });
            }
        };
        if let Err(err) = cleanup_result
            && mods_path.exists()
        {
            return Err(AssetCompileError::Io {
                operation: AssetCompileOperation::Cleanup,
                kind: err.kind(),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let report = CompileValidation {
            exit_code: output.status.code().unwrap_or(-1),
            stdout_tail: sanitize_compile_output(project_root, &tail(&stdout, 4000)),
            stderr_tail: sanitize_compile_output(project_root, &tail(&stderr, 4000)),
        };
        if output.status.success() {
            Ok(report)
        } else {
            Err(AssetCompileError::Rejected(report))
        }
    }
}

fn isolated_mods_path(project_root: &Path, run_id: &RunId) -> PathBuf {
    let safe_run_id: String = run_id
        .0
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    project_root
        .join(".ats")
        .join("compile-gate")
        .join(safe_run_id)
}

fn with_trailing_separator(path: &Path) -> String {
    let mut value = path.display().to_string();
    if !value.ends_with(std::path::MAIN_SEPARATOR) {
        value.push(std::path::MAIN_SEPARATOR);
    }
    value
}

/// MSBuild 会在 `ModsPath` 后继续拼接项目名和目录分隔符。Windows verbatim
/// 路径 (`\\?\`) 不接受模板中可能出现的 `/`，因此属性边界使用普通绝对路径；
/// 进程 cwd 仍通过 `to_extended_length_path` 保留长路径支持。
fn msbuild_property_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let value = path.to_string_lossy();
        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = value.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

pub(crate) fn sanitize_compile_output(project_root: &Path, value: &str) -> String {
    let mut safe = value.replace(&project_root.to_string_lossy().to_string(), "<project>");
    let extended = to_extended_length_path(project_root);
    safe = safe.replace(&extended.to_string_lossy().to_string(), "<project>");

    static WINDOWS_ABSOLUTE_PATH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let matcher = WINDOWS_ABSOLUTE_PATH.get_or_init(|| {
        regex::Regex::new(
            r#"(?i)(?:\\\\\?\\)?[a-z]:[\\/](?:[^\\/:*?\"<>|\r\n()\[\]]+[\\/])*[^\\/:*?\"<>|\r\n()\[\]]*"#,
        )
        .expect("compile diagnostic path regex must compile")
    });
    matcher.replace_all(&safe, "<absolute-path>").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isolated_output_stays_below_project_root() {
        let root = Path::new("C:/mods/demo");
        let path = isolated_mods_path(root, &RunId("run/../../escape".into()));
        assert!(path.starts_with(root));
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("run_______escape")
        );
    }

    #[test]
    fn mods_path_has_platform_separator() {
        let path = Path::new("C:/mods/isolated");
        assert!(with_trailing_separator(path).ends_with(std::path::MAIN_SEPARATOR));
    }

    #[test]
    fn compile_output_redacts_project_and_other_absolute_windows_paths() {
        let root = Path::new(r"C:\Users\private\project");
        let output = concat!(
            r"C:\Users\private\project\Generated\Card.cs(7,3): error CS1002: ; expected",
            "\n",
            r"C:\Users\other\.nuget\packages\dependency.dll: warning canary"
        );
        let safe = sanitize_compile_output(root, output);

        assert!(safe.contains("<project>"));
        assert!(safe.contains("error CS1002"));
        assert!(safe.contains("<absolute-path>"));
        assert!(!safe.contains("private"));
        assert!(!safe.contains("other"));
    }

    #[cfg(windows)]
    #[test]
    fn msbuild_property_removes_windows_verbatim_prefix() {
        assert_eq!(
            msbuild_property_path(Path::new(r"\\?\C:\mods\isolated")),
            PathBuf::from(r"C:\mods\isolated")
        );
        assert_eq!(
            msbuild_property_path(Path::new(r"\\?\UNC\server\share\mods")),
            PathBuf::from(r"\\server\share\mods")
        );
    }
}
