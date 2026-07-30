//! build_project handler：在 project_root 下跑 `dotnet publish`，捕获输出。
//!
//! 用 std::process::Command + spawn_blocking 而非 tokio::process（Windows 下
//! tokio::process::Command::output 偶尔挂起）。成功判定：exit 0 优先，否则
//! 启发性看 stdout 含 "0 Error(s)"（部分 godot/msbuild 组合会返回 -1）。

use std::sync::Arc;

use super::common::{ProgressEvent, ProgressSink, finalize_with_error, transition_to_running};
use crate::game_pack::{BuildRecipe, BuildRunner};
use crate::platform::contracts::SubmitBuildProjectRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};
use crate::project_utils::to_extended_length_path;

pub async fn run_build_project(
    repo: Arc<dyn JobRepository>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitBuildProjectRequest,
    recipe: BuildRecipe,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    let mut step_results = Vec::with_capacity(recipe.steps.len());
    let mut success = true;
    let mut exit_code = 0;
    let mut stdout = String::new();
    let mut stderr = String::new();
    for (index, step) in recipe.steps.iter().enumerate() {
        sink.emit(ProgressEvent {
            job_id: job_id.clone(),
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
        let output = match execute_build_step(&request.project_root, step.runner).await {
            Ok(output) => output,
            Err(error) => {
                finalize_with_error(
                    &repo,
                    &job_id,
                    &sink,
                    &format!("build step `{}`: {error}", step.id),
                )
                .await;
                return;
            }
        };
        exit_code = output.exit_code;
        stdout = output.stdout;
        stderr = output.stderr;
        let step_success = exit_code == 0 || build_reports_zero_errors(&stdout);
        step_results.push(serde_json::json!({
            "id": step.id,
            "runner": step.runner.as_str(),
            "success": step_success,
            "exitCode": exit_code,
            "stdoutTail": tail(&stdout, 5000),
            "stderrTail": tail(&stderr, 5000),
        }));
        if !step_success {
            success = false;
            break;
        }
    }

    let job = match repo.get(&job_id).await {
        Ok(j) => j,
        Err(_) => return,
    };
    if matches!(job.status, JobStatus::Cancelled) {
        return;
    }
    let mut job = job;
    job.status = if success {
        JobStatus::Completed
    } else {
        JobStatus::Failed
    };
    job.completed_at = Some(chrono::Utc::now());
    if !success {
        job.error = Some(format!("dotnet publish exited with code {exit_code}"));
    }
    job.result = Some(serde_json::json!({
        "success": success,
        "exitCode": exit_code,
        "stdoutTail": tail(&stdout, 5000),
        "stderrTail": tail(&stderr, 5000),
        "projectRoot": request.project_root.display().to_string(),
        "steps": step_results,
    }));
    let _ = repo.update(&job).await;

    sink.emit(ProgressEvent {
        job_id,
        stage: if success {
            "completed".into()
        } else {
            "failed".into()
        },
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

async fn execute_build_step(
    project_root: &std::path::Path,
    runner: BuildRunner,
) -> Result<BuildStepOutput, String> {
    let cwd = to_extended_length_path(project_root);
    let output = tokio::task::spawn_blocking(move || match runner {
        BuildRunner::DotnetPublish => std::process::Command::new("dotnet")
            .arg("publish")
            .current_dir(&cwd)
            .output(),
    })
    .await
    .map_err(|error| format!("join blocking runner: {error}"))?
    .map_err(|error| format!("spawn {}: {error}", runner.as_str()))?;
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
