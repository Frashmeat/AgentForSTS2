//! build_project handler：在 project_root 下跑 `dotnet publish`，捕获输出。
//!
//! 外部构建通过受控进程启动；Windows 使用 Job Object，在取消时终止并等待
//! 整棵进程树。成功判定：exit 0 优先，否则
//! 启发性看 stdout 含 "0 Error(s)"（部分 godot/msbuild 组合会返回 -1）。

use std::sync::Arc;

use super::common::{
    FinalizeOutcome, ProgressEvent, ProgressSink, finalize_with_cancellation,
    finalize_with_failure, finalize_with_success, transition_to_running,
};
use crate::controlled_process::{ControlledProcessResult, run_controlled_process};
use crate::failure::{ActionableFailure, FailureCategory, FailureNormalizer, RecoveryAction};
use crate::game_pack::{BuildRecipe, BuildRunner};
use crate::platform::application::CancellationToken;
use crate::platform::contracts::SubmitBuildProjectRequest;
use crate::platform::domain::{BuildStepResult, RunId, RunRepository, RunResult};
use crate::project_utils::to_extended_length_path;

pub async fn run_build_project(
    repo: Arc<dyn RunRepository>,
    sink: Arc<dyn ProgressSink>,
    run_id: RunId,
    request: SubmitBuildProjectRequest,
    recipe: BuildRecipe,
    cancellation: CancellationToken,
) {
    if !matches!(
        transition_to_running(&repo, &run_id, &sink, &cancellation).await,
        Ok(true)
    ) {
        return;
    }

    let mut step_results = Vec::with_capacity(recipe.steps.len());
    let mut success = true;
    let mut exit_code = 0;
    for (index, step) in recipe.steps.iter().enumerate() {
        sink.emit(ProgressEvent {
            run_id: run_id.clone(),
            stage: step.id.clone(),
            percent: Some(index as f32 / recipe.steps.len() as f32),
            message: Some(format!(
                "{} in {}",
                step.runner.as_str(),
                request.project_root.display()
            )),
            delta: None,
        })
        .await;
        let output =
            match execute_build_step(&request.project_root, step.runner, &cancellation).await {
                Ok(output) => output,
                Err(BuildStepError::Cancelled(reason)) => {
                    finalize_with_cancellation(&repo, &run_id, &sink, reason).await;
                    return;
                }
                Err(BuildStepError::Io(error)) => {
                    finalize_with_failure(
                        &repo,
                        &run_id,
                        &sink,
                        FailureNormalizer::io(
                            "toolchain.not_found",
                            "build.spawn",
                            "The configured build tool could not be started.",
                            &error,
                        ),
                    )
                    .await;
                    return;
                }
            };
        exit_code = output.exit_code;
        let step_success = exit_code == 0 || build_reports_zero_errors(&output.stdout);
        step_results.push(BuildStepResult {
            id: step.id.clone(),
            runner: step.runner.as_str().to_string(),
            success: step_success,
            exit_code,
            stdout_tail: tail(&output.stdout, 5000),
            stderr_tail: tail(&output.stderr, 5000),
        });
        if !step_success {
            success = false;
            break;
        }
    }

    if !success {
        finalize_with_failure(
            &repo,
            &run_id,
            &sink,
            ActionableFailure::new(
                "toolchain.command_failed",
                FailureCategory::Toolchain,
                "build.execute",
                "The project build failed. Review the build output and retry.",
                RecoveryAction::Retry,
                false,
            ),
        )
        .await;
        return;
    }
    let result = RunResult::Build {
        project_relative_root: ".".into(),
        steps: step_results,
        artifact_manifest_ref: None,
        manifest_sha256: None,
    };
    if let Some(reason) = cancellation.reason() {
        finalize_with_cancellation(&repo, &run_id, &sink, reason).await;
        return;
    }
    if !matches!(
        finalize_with_success(&repo, &run_id, result).await,
        FinalizeOutcome::Succeeded
    ) {
        return;
    }

    sink.emit(ProgressEvent {
        run_id,
        stage: "completed".into(),
        percent: Some(1.0),
        message: Some(format!("exit_code={exit_code}, success={success}")),
        delta: None,
    })
    .await;
}

struct BuildStepOutput {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

enum BuildStepError {
    Io(std::io::Error),
    Cancelled(crate::platform::domain::CancellationReason),
}

async fn execute_build_step(
    project_root: &std::path::Path,
    runner: BuildRunner,
    cancellation: &CancellationToken,
) -> Result<BuildStepOutput, BuildStepError> {
    let cwd = to_extended_length_path(project_root);
    let (program, args) = match runner {
        BuildRunner::DotnetPublish => (
            std::ffi::OsString::from("dotnet"),
            vec![std::ffi::OsString::from("publish")],
        ),
    };
    let output = match run_controlled_process(&program, &args, &cwd, cancellation)
        .await
        .map_err(BuildStepError::Io)?
    {
        ControlledProcessResult::Completed(output) => output,
        ControlledProcessResult::Cancelled(reason) => {
            return Err(BuildStepError::Cancelled(reason));
        }
    };
    Ok(BuildStepOutput {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

/// 判定 dotnet/MSBuild 输出是否报告「0 个错误」。
///
/// MSBuild 末尾会打印形如 `    N Error(s)` 的摘要。必须按词边界解析数字：
/// 旧实现 `stdout.contains("0 Error(s)")` 会被 `10 Error(s)` 命中，把失败构建误报成功。
/// 取最后一个 `N Error(s)` 摘要判定，N 全为 0 才算成功；无摘要时保守返回 false。
fn build_reports_zero_errors(stdout: &str) -> bool {
    let mut zero: Option<bool> = None;
    for (idx, _) in stdout.match_indices("Error(s)") {
        let digits: String = stdout[..idx]
            .chars()
            .rev()
            .skip_while(|c| c.is_whitespace())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if !digits.is_empty() {
            zero = Some(digits.bytes().all(|b| b == b'0'));
        }
    }
    zero.unwrap_or(false)
}

/// 保留文本末尾 max_chars 个字符，超长则前缀加截断标记。
pub(crate) fn tail(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let skip = text.chars().count() - max_chars;
    let tail: String = text.chars().skip(skip).collect();
    format!("...[truncated {skip} chars]\n{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_keeps_short_text_intact() {
        assert_eq!(tail("hello", 100), "hello");
    }

    #[test]
    fn build_reports_zero_errors_respects_word_boundary() {
        assert!(build_reports_zero_errors(
            "Build succeeded.\n    0 Warning(s)\n    0 Error(s)\n"
        ));
        // 核心回归：10 Error(s) 不得被当成 0 Error(s)
        assert!(!build_reports_zero_errors("    10 Error(s)"));
        assert!(!build_reports_zero_errors("Build FAILED.\n    3 Error(s)"));
        // 无摘要 → 保守判失败
        assert!(!build_reports_zero_errors("no summary present"));
        // 取最后一个摘要：先 0 后 2 → 失败
        assert!(!build_reports_zero_errors(
            "ProjA -> 0 Error(s)\nProjB -> 2 Error(s)"
        ));
    }

    #[test]
    fn tail_truncates_long_text() {
        let s: String = "x".repeat(120);
        let t = tail(&s, 50);
        assert!(t.starts_with("...[truncated"));
        let tail_chars: String = t.chars().rev().take(50).collect();
        assert_eq!(tail_chars.chars().count(), 50);
    }

    #[test]
    fn tail_at_exact_boundary() {
        let s = "x".repeat(50);
        // 等于阈值不截断
        assert_eq!(tail(&s, 50), s);
    }
}
