//! build_project handler：在 project_root 下跑 `dotnet publish`，捕获输出。
//!
//! 用 std::process::Command + spawn_blocking 而非 tokio::process（Windows 下
//! tokio::process::Command::output 偶尔挂起）。成功判定：exit 0 优先，否则
//! 启发性看 stdout 含 "0 Error(s)"（部分 godot/msbuild 组合会返回 -1）。

use std::sync::Arc;

use super::common::{ProgressEvent, ProgressSink, finalize_with_error, transition_to_running};
use crate::platform::contracts::SubmitBuildProjectRequest;
use crate::platform::domain::{JobId, JobRepository, JobStatus};
use crate::project_utils::to_extended_length_path;

pub async fn run_build_project(
    repo: Arc<dyn JobRepository>,
    sink: Arc<dyn ProgressSink>,
    job_id: JobId,
    request: SubmitBuildProjectRequest,
) {
    if transition_to_running(&repo, &job_id, &sink).await.is_err() {
        return;
    }

    sink.emit(ProgressEvent {
        job_id: job_id.clone(),
        stage: "spawning".into(),
        percent: None,
        message: Some(format!(
            "dotnet publish in {}",
            request.project_root.display()
        )),
        delta: None,
    })
    .await;

    // Windows 长路径保护
    let cwd = to_extended_length_path(&request.project_root);
    let output_result = tokio::task::spawn_blocking(move || {
        std::process::Command::new("dotnet")
            .arg("publish")
            .current_dir(&cwd)
            .output()
    })
    .await;

    let output = match output_result {
        Ok(Ok(out)) => out,
        Ok(Err(err)) => {
            finalize_with_error(&repo, &job_id, &format!("spawn dotnet: {err}")).await;
            return;
        }
        Err(err) => {
            finalize_with_error(&repo, &job_id, &format!("join blocking: {err}")).await;
            return;
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);
    let success = exit_code == 0 || stdout.contains("0 Error(s)");

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
