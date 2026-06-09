//! 原子文件写：写到同目录的唯一临时文件，flush 后 rename 到目标路径。
//!
//! 为什么：直接 `fs::write(path, ..)` 是「打开-截断-逐块写」，一旦中途崩溃/取消/
//! 重试，读者（IDE、dotnet、下一次构建）可能读到半截或被清零的文件，甚至把上一份
//! 好产物覆盖坏。`tmp + rename`（同卷 rename 原子）保证目标要么是旧内容、要么是完整
//! 新内容，绝不出现中间态。临时名带进程内单调计数器 + pid，避免并发写同一目标时撞名
//! （否则两个写者用同一个 `.tmp` 会把彼此写坏的中间文件 rename 上去）。
//!
//! 本仓库约定见 `CLAUDE.md §2`：重要产物一律原子写。FileJobRepository / plan_artifact /
//! recents 早已各自实现，本模块把它收成一份共享实现，供 handler 复用。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::fs;
use tokio::io::AsyncWriteExt;

/// 进程内单调计数器，保证并发写同一目标时临时文件名不撞。
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
}
