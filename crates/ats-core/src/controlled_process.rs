use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::{ExitStatus, Stdio};

use tokio::io::AsyncReadExt;

use crate::cancellation::CancellationToken;
use crate::platform::domain::CancellationReason;

#[derive(Debug)]
pub struct ControlledProcessOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub enum ControlledProcessResult {
    Completed(ControlledProcessOutput),
    Cancelled(CancellationReason),
}

#[cfg(windows)]
pub async fn run_controlled_process(
    program: &OsStr,
    args: &[OsString],
    cwd: &Path,
    cancellation: &CancellationToken,
) -> std::io::Result<ControlledProcessResult> {
    use process_wrap::tokio::{CommandWrap, JobObject, KillOnDrop};

    let mut command = CommandWrap::with_new(program, |command| {
        command
            .args(args)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    });
    command.wrap(KillOnDrop).wrap(JobObject);
    let mut child = command.spawn()?;
    let stdout = child.stdout().take();
    let stderr = child.stderr().take();
    let stdout_task = tokio::spawn(read_pipe(stdout));
    let stderr_task = tokio::spawn(read_pipe(stderr));

    let outcome = tokio::select! {
        reason = cancellation.cancelled() => {
            let kill_result = Box::into_pin(child.kill()).await;
            let stdout = join_pipe(stdout_task).await;
            let stderr = join_pipe(stderr_task).await;
            kill_result?;
            stdout?;
            stderr?;
            return Ok(ControlledProcessResult::Cancelled(reason));
        }
        status = child.wait() => (status?, stdout_task, stderr_task),
    };
    let stdout = join_pipe(outcome.1).await?;
    let stderr = join_pipe(outcome.2).await?;
    Ok(ControlledProcessResult::Completed(
        ControlledProcessOutput {
            status: outcome.0,
            stdout,
            stderr,
        },
    ))
}

#[cfg(not(windows))]
pub async fn run_controlled_process(
    program: &OsStr,
    args: &[OsString],
    cwd: &Path,
    cancellation: &CancellationToken,
) -> std::io::Result<ControlledProcessResult> {
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let stdout_task = tokio::spawn(read_pipe(child.stdout.take()));
    let stderr_task = tokio::spawn(read_pipe(child.stderr.take()));
    let status = tokio::select! {
        reason = cancellation.cancelled() => {
            child.kill().await?;
            join_pipe(stdout_task).await?;
            join_pipe(stderr_task).await?;
            return Ok(ControlledProcessResult::Cancelled(reason));
        }
        status = child.wait() => status?,
    };
    Ok(ControlledProcessResult::Completed(
        ControlledProcessOutput {
            status,
            stdout: join_pipe(stdout_task).await?,
            stderr: join_pipe(stderr_task).await?,
        },
    ))
}

async fn read_pipe<R>(pipe: Option<R>) -> std::io::Result<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    if let Some(mut pipe) = pipe {
        pipe.read_to_end(&mut bytes).await?;
    }
    Ok(bytes)
}

async fn join_pipe(
    task: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
) -> std::io::Result<Vec<u8>> {
    task.await
        .map_err(|error| std::io::Error::other(format!("process output task failed: {error}")))?
}

#[cfg(all(test, windows))]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::platform::domain::CancellationReason;

    #[tokio::test]
    async fn cancellation_kills_the_windows_job_process_tree() {
        let temp = tempfile::TempDir::new().unwrap();
        let child = temp.path().join("child.cmd");
        let parent = temp.path().join("parent.cmd");
        let sentinel = temp.path().join("child-survived.txt");
        std::fs::write(
            &child,
            "@echo off\r\nping -n 4 127.0.0.1 >nul\r\necho survived>child-survived.txt\r\n",
        )
        .unwrap();
        std::fs::write(
            &parent,
            "@echo off\r\nstart \"\" /b cmd.exe /D /C child.cmd\r\nping -n 30 127.0.0.1 >nul\r\n",
        )
        .unwrap();
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let cwd = temp.path().to_path_buf();
        let task = tokio::spawn(async move {
            run_controlled_process(
                OsStr::new("cmd.exe"),
                &[
                    OsString::from("/D"),
                    OsString::from("/C"),
                    parent.into_os_string(),
                ],
                &cwd,
                &worker_cancellation,
            )
            .await
        });

        tokio::time::sleep(Duration::from_millis(200)).await;
        cancellation.cancel(CancellationReason::ProjectClose);
        let result = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("controlled process did not finish after cancellation")
            .unwrap()
            .unwrap();
        assert!(matches!(result, ControlledProcessResult::Cancelled(_)));

        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(
            !sentinel.exists(),
            "a child process survived the Windows Job Object cancellation"
        );
    }
}
