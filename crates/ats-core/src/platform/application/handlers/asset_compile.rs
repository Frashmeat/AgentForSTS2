use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::build_project::tail;
use crate::platform::domain::RunId;
use crate::project_utils::to_extended_length_path;

#[derive(Debug, Clone)]
pub(crate) struct CompileValidation {
    pub exit_code: i32,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

#[async_trait]
pub(crate) trait AssetCompileValidator: Send + Sync {
    async fn validate(
        &self,
        project_root: &Path,
        run_id: &RunId,
    ) -> Result<CompileValidation, String>;
}

#[derive(Debug, Default)]
pub(crate) struct DotnetAssetCompileValidator;

#[async_trait]
impl AssetCompileValidator for DotnetAssetCompileValidator {
    async fn validate(
        &self,
        project_root: &Path,
        run_id: &RunId,
    ) -> Result<CompileValidation, String> {
        let mods_path = isolated_mods_path(project_root, run_id);
        tokio::fs::create_dir_all(&mods_path)
            .await
            .map_err(|err| format!("create isolated compile output: {err}"))?;

        let cwd = to_extended_length_path(project_root);
        let mods_arg = format!(
            "-p:ModsPath={}",
            with_trailing_separator(&msbuild_property_path(&mods_path))
        );
        let output_result = tokio::task::spawn_blocking(move || {
            std::process::Command::new("dotnet")
                .arg("build")
                .arg("--nologo")
                .arg(mods_arg)
                .current_dir(cwd)
                .output()
        })
        .await;

        let cleanup_result = tokio::fs::remove_dir_all(&mods_path).await;
        let output = match output_result {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => return Err(format!("spawn dotnet compile gate: {err}")),
            Err(err) => return Err(format!("join dotnet compile gate: {err}")),
        };
        if let Err(err) = cleanup_result
            && mods_path.exists()
        {
            return Err(format!("clean isolated compile output: {err}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let report = CompileValidation {
            exit_code: output.status.code().unwrap_or(-1),
            stdout_tail: tail(&stdout, 4000),
            stderr_tail: tail(&stderr, 4000),
        };
        if output.status.success() {
            Ok(report)
        } else {
            Err(format_compile_failure(&report))
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

fn format_compile_failure(report: &CompileValidation) -> String {
    format!(
        "generated asset failed compile gate (exit code {})\nstdout tail:\n{}\nstderr tail:\n{}",
        report.exit_code, report.stdout_tail, report.stderr_tail
    )
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
