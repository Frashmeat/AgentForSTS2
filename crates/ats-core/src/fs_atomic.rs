//! 原子文件写：写到同目录的唯一临时文件，flush 后 rename 到目标路径。
//!
//! 为什么：直接 `fs::write(path, ..)` 是「打开-截断-逐块写」，一旦中途崩溃/取消/
//! 重试，读者（IDE、dotnet、下一次构建）可能读到半截或被清零的文件，甚至把上一份
//! 好产物覆盖坏。`tmp + rename`（同卷 rename 原子）保证目标要么是旧内容、要么是完整
//! 新内容，绝不出现中间态。临时名带进程内单调计数器 + pid，避免并发写同一目标时撞名
//! （否则两个写者用同一个 `.tmp` 会把彼此写坏的中间文件 rename 上去）。
//!
//! 本仓库约定见 `CLAUDE.md §2`：重要产物一律原子写。FileRunRepository / plan_artifact /
//! recents 早已各自实现，本模块把它收成一份共享实现，供 handler 复用。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;
use std::{fs as std_fs, io, io::Write};

use tokio::fs;
use tokio::io::AsyncWriteExt;

/// 进程内单调计数器，保证并发写同一目标时临时文件名不撞。
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) const RENAME_RETRY_DELAYS: [Duration; 5] = [
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(400),
    Duration::from_millis(800),
];

/// Rename within one filesystem, retrying only bounded transient Windows conflicts.
///
/// Callers retain ownership of rollback and cleanup when the final attempt fails.
pub(crate) fn rename_with_retry(
    source: &Path,
    destination: &Path,
    operation: &'static str,
) -> io::Result<()> {
    rename_with_retry_with(
        source,
        destination,
        operation,
        |from, to| std_fs::rename(from, to),
        thread::sleep,
        is_transient_rename_error,
    )
}

pub(crate) fn rename_with_retry_with<R, S, C>(
    source: &Path,
    destination: &Path,
    operation: &'static str,
    mut rename: R,
    mut sleep: S,
    is_retryable: C,
) -> io::Result<()>
where
    R: FnMut(&Path, &Path) -> io::Result<()>,
    S: FnMut(Duration),
    C: Fn(&io::Error) -> bool,
{
    let mut attempt = 1_u8;
    loop {
        match rename(source, destination) {
            Ok(()) => return Ok(()),
            Err(error)
                if is_retryable(&error) && usize::from(attempt) <= RENAME_RETRY_DELAYS.len() =>
            {
                let delay = RENAME_RETRY_DELAYS[usize::from(attempt) - 1];
                tracing::warn!(
                    operation,
                    attempt,
                    next_attempt = attempt + 1,
                    delay_ms = delay.as_millis(),
                    io_kind = ?error.kind(),
                    "atomic directory rename was temporarily unavailable; retrying"
                );
                sleep(delay);
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

pub(crate) fn is_transient_rename_error(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::Interrupted {
        return true;
    }
    #[cfg(windows)]
    {
        error.kind() == io::ErrorKind::PermissionDenied
            || matches!(error.raw_os_error(), Some(5 | 32 | 33))
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 原子写 `bytes` 到 `path`：写同目录唯一临时文件 → flush → rename。
///
/// 失败时尽力删除临时文件，不留垃圾。要求 `path` 有父目录（调用方负责先建好）。
///
/// # Errors
/// 底层 IO 失败（创建 / 写入 / flush / rename）。
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = unique_tmp_path(path);

    let write_result = async {
        let mut f = fs::File::create(&tmp).await?;
        f.write_all(bytes).await?;
        f.flush().await?;
        Ok::<(), std::io::Error>(())
    }
    .await;

    if let Err(err) = write_result {
        let _ = fs::remove_file(&tmp).await;
        return Err(err);
    }
    if let Err(err) = fs::rename(&tmp, path).await {
        let _ = fs::remove_file(&tmp).await;
        return Err(err);
    }
    Ok(())
}

/// 同步版原子写，供同步 Tauri command 和工程配置模块使用。
pub fn write_atomic_sync(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = unique_tmp_path(path);
    let write_result = (|| {
        let mut file = std_fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok::<(), std::io::Error>(())
    })();
    if let Err(error) = write_result {
        let _ = std_fs::remove_file(&tmp);
        return Err(error);
    }
    if let Err(error) = std_fs::rename(&tmp, path) {
        let _ = std_fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

/// 为 `path` 生成同目录下的唯一隐藏临时文件名：`.<filename>.<pid>.<counter>.tmp`。
fn unique_tmp_path(path: &Path) -> PathBuf {
    let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let base = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "tmp".into());
    let tmp_name = format!(".{base}.{pid}.{n}.tmp");
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn dir_entries(dir: &Path) -> Vec<String> {
        let mut names = Vec::new();
        let mut rd = fs::read_dir(dir).await.unwrap();
        while let Some(e) = rd.next_entry().await.unwrap() {
            names.push(e.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        names
    }

    #[tokio::test]
    async fn writes_file_and_leaves_no_tmp() {
        let td = tempfile::TempDir::new().unwrap();
        let p = td.path().join("out.cs");
        write_atomic(&p, b"hello").await.unwrap();
        assert_eq!(fs::read(&p).await.unwrap(), b"hello");
        // 目录里只剩目标文件，没有 .tmp 残留
        assert_eq!(dir_entries(td.path()).await, vec!["out.cs".to_string()]);
    }

    #[tokio::test]
    async fn overwrites_existing_atomically() {
        let td = tempfile::TempDir::new().unwrap();
        let p = td.path().join("out.cs");
        write_atomic(&p, b"v1").await.unwrap();
        write_atomic(&p, b"v2-longer").await.unwrap();
        assert_eq!(fs::read(&p).await.unwrap(), b"v2-longer");
        assert_eq!(dir_entries(td.path()).await, vec!["out.cs".to_string()]);
    }

    #[tokio::test]
    async fn concurrent_writes_to_same_target_do_not_collide() {
        let td = tempfile::TempDir::new().unwrap();
        let p = td.path().join("out.bin");
        let mut handles = Vec::new();
        for i in 0..16u8 {
            let p = p.clone();
            handles.push(tokio::spawn(async move {
                write_atomic(&p, &[i; 32]).await.unwrap();
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        // 最终只有目标文件，无 .tmp 残留；内容是某个写者的完整 32 字节
        assert_eq!(dir_entries(td.path()).await, vec!["out.bin".to_string()]);
        let got = fs::read(&p).await.unwrap();
        assert_eq!(got.len(), 32);
        assert!(got.iter().all(|b| *b == got[0]));
    }

    #[test]
    fn sync_write_overwrites_atomically() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("local.props");
        write_atomic_sync(&path, b"v1").unwrap();
        write_atomic_sync(&path, b"v2").unwrap();
        assert_eq!(std_fs::read(&path).unwrap(), b"v2");
    }

    #[test]
    fn rename_retries_transient_conflicts_then_succeeds() {
        let temp = tempfile::TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        std_fs::create_dir(&source).unwrap();
        std_fs::write(source.join("complete.txt"), b"complete").unwrap();
        let mut attempts = 0_u8;
        let mut delays = Vec::new();

        rename_with_retry_with(
            &source,
            &destination,
            "test_rename",
            |from, to| {
                attempts += 1;
                if attempts <= 2 {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "sharing violation canary",
                    ))
                } else {
                    std_fs::rename(from, to)
                }
            },
            |delay| delays.push(delay),
            |error| error.kind() == io::ErrorKind::PermissionDenied,
        )
        .unwrap();

        assert_eq!(attempts, 3);
        assert_eq!(delays, RENAME_RETRY_DELAYS[..2]);
        assert!(!source.exists());
        assert_eq!(
            std_fs::read(destination.join("complete.txt")).unwrap(),
            b"complete"
        );
    }

    #[test]
    fn rename_exhausts_bounded_attempts() {
        let temp = tempfile::TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        std_fs::create_dir(&source).unwrap();
        let mut attempts = 0_u8;

        let error = rename_with_retry_with(
            &source,
            &destination,
            "test_rename",
            |_, _| {
                attempts += 1;
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "sharing violation canary",
                ))
            },
            |_| {},
            |_| true,
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(usize::from(attempts), RENAME_RETRY_DELAYS.len() + 1);
        assert!(source.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn rename_does_not_retry_deterministic_errors() {
        let temp = tempfile::TempDir::new().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        std_fs::create_dir(&source).unwrap();
        let mut attempts = 0_u8;
        let mut sleeps = 0_u8;

        let error = rename_with_retry_with(
            &source,
            &destination,
            "test_rename",
            |_, _| {
                attempts += 1;
                Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "deterministic canary",
                ))
            },
            |_| sleeps += 1,
            |_| false,
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(attempts, 1);
        assert_eq!(sleeps, 0);
    }

    #[test]
    fn rename_classifier_is_platform_bounded() {
        assert!(is_transient_rename_error(&io::Error::new(
            io::ErrorKind::Interrupted,
            "interrupted canary",
        )));
        assert!(!is_transient_rename_error(&io::Error::new(
            io::ErrorKind::AlreadyExists,
            "deterministic canary",
        )));

        let permission = io::Error::new(io::ErrorKind::PermissionDenied, "permission canary");
        #[cfg(windows)]
        assert!(is_transient_rename_error(&permission));
        #[cfg(not(windows))]
        assert!(!is_transient_rename_error(&permission));
    }
}
