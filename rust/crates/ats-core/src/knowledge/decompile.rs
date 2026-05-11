//! ilspycmd 子进程发现 + 反编译调度。
//!
//! Stage 2.2.1：先把"找到 ilspycmd"和"跑 ilspycmd"两件事拆成纯函数 + 子进程封装。
//! Handler 层（platform/handlers/knowledge_refresh.rs）负责拼装这两步并接 Job 框架。
//!
//! 设计取向：
//! - 发现走 PATH + 候选目录数组。`discover_ilspycmd_in` 是纯函数，单测注入假目录即可
//! - 反编译用 std::process::Command + spawn_blocking，与 build_project handler 同套路
//! - 不做缓存命中判定（manifest 比较留给上层 handler 决策），本模块只负责"动作"

use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecompileError {
    #[error("ilspycmd not found: searched PATH and {searched_count} candidate dirs")]
    NotFound { searched_count: usize },
    #[error("source dll not found: {0}")]
    DllMissing(PathBuf),
    #[error("output directory create failed: {0}")]
    OutputCreate(String),
    #[error("spawn ilspycmd failed: {0}")]
    Spawn(String),
    #[error("ilspycmd exited with code {code}: {tail}")]
    ProcessFailed { code: i32, tail: String },
    #[error("decompile produced no .cs files")]
    EmptyOutput,
    #[error("walk output dir: {0}")]
    Walk(String),
}

/// 反编译完成后的统计结果。
#[derive(Debug, Clone)]
pub struct DecompileStats {
    pub cs_file_count: u32,
    pub total_bytes: u64,
    pub stdout_tail: String,
    pub stderr_tail: String,
    pub exit_code: i32,
}

/// 在给定的候选目录中查找 ilspycmd 可执行文件。
///
/// 顺序：
/// 1. 遍历 candidate_dirs，每个目录下查 `ilspycmd` + `ilspycmd.exe`（Windows）
/// 2. 找到第一个存在且可执行（Unix）/ 文件存在（Windows）的返回
///
/// 调用方应当把 PATH 拆分后追加到 candidate_dirs 末尾，或单独再扫一遍 PATH。
/// 本函数不主动读取环境变量，便于单测。
#[must_use]
pub fn discover_ilspycmd_in(candidate_dirs: &[&Path]) -> Option<PathBuf> {
    for dir in candidate_dirs {
        for name in ilspycmd_filenames() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// 整合 PATH + 自定义候选目录的发现入口。
///
/// `extra_dirs` 用于 .NET 全局工具的标准安装位置（如 `~/.dotnet/tools`、
/// `%USERPROFILE%\.dotnet\tools`）。
#[must_use]
pub fn discover_ilspycmd(extra_dirs: &[PathBuf]) -> Option<PathBuf> {
    let mut all: Vec<PathBuf> = Vec::new();
    if let Some(path_var) = std::env::var_os("PATH") {
        all.extend(std::env::split_paths(&path_var));
    }
    all.extend(extra_dirs.iter().cloned());
    let refs: Vec<&Path> = all.iter().map(PathBuf::as_path).collect();
    discover_ilspycmd_in(&refs)
}

/// .NET 全局工具默认安装目录候选。
///
/// 不保证存在；调用方自行 filter。
#[must_use]
pub fn default_dotnet_tools_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".dotnet").join("tools"));
    }
    dirs
}

/// 项目模式：跑 `ilspycmd <dll> -o <output_dir> -p`，按命名空间拆多个 .cs 文件。
/// 适合大型 DLL（如 sts2.dll）。
///
/// # Errors
/// - `DllMissing` / `OutputCreate` / `Spawn` / `ProcessFailed` / `EmptyOutput` / `Walk`
pub fn run_decompile_project(
    ilspycmd: &Path,
    dll: &Path,
    output_dir: &Path,
) -> Result<DecompileStats, DecompileError> {
    if !dll.is_file() {
        return Err(DecompileError::DllMissing(dll.to_path_buf()));
    }
    std::fs::create_dir_all(output_dir)
        .map_err(|e| DecompileError::OutputCreate(e.to_string()))?;

    let output = Command::new(ilspycmd)
        .arg(dll)
        .arg("-o")
        .arg(output_dir)
        .arg("-p")
        .output()
        .map_err(|e| DecompileError::Spawn(e.to_string()))?;

    let stdout_tail = tail_lossy(&output.stdout, 4000);
    let stderr_tail = tail_lossy(&output.stderr, 4000);
    let exit_code = output.status.code().unwrap_or(-1);

    if !output.status.success() {
        return Err(DecompileError::ProcessFailed {
            code: exit_code,
            tail: format!("stdout: {stdout_tail}\nstderr: {stderr_tail}"),
        });
    }

    let (cs_file_count, total_bytes) =
        count_cs_files(output_dir).map_err(|e| DecompileError::Walk(e.to_string()))?;
    if cs_file_count == 0 {
        return Err(DecompileError::EmptyOutput);
    }

    Ok(DecompileStats {
        cs_file_count,
        total_bytes,
        stdout_tail,
        stderr_tail,
        exit_code,
    })
}

/// 单文件模式：跑 `ilspycmd <dll> -o <output_file>`，输出合并到单个 .cs 文件。
/// 适合小型 DLL（如 BaseLib.dll）以及 runtime 既定单文件布局。
///
/// # Errors
/// - `DllMissing`：dll 不存在
/// - `OutputCreate`：output_file 父目录创建失败
/// - `Spawn`：进程启动失败
/// - `ProcessFailed`：进程退出码非 0
/// - `EmptyOutput`：输出文件不存在或 0 字节
pub fn run_decompile_file(
    ilspycmd: &Path,
    dll: &Path,
    output_file: &Path,
) -> Result<DecompileStats, DecompileError> {
    if !dll.is_file() {
        return Err(DecompileError::DllMissing(dll.to_path_buf()));
    }
    if let Some(parent) = output_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DecompileError::OutputCreate(e.to_string()))?;
    }

    let output = Command::new(ilspycmd)
        .arg(dll)
        .arg("-o")
        .arg(output_file)
        .output()
        .map_err(|e| DecompileError::Spawn(e.to_string()))?;

    let stdout_tail = tail_lossy(&output.stdout, 4000);
    let stderr_tail = tail_lossy(&output.stderr, 4000);
    let exit_code = output.status.code().unwrap_or(-1);

    if !output.status.success() {
        return Err(DecompileError::ProcessFailed {
            code: exit_code,
            tail: format!("stdout: {stdout_tail}\nstderr: {stderr_tail}"),
        });
    }

    let meta = std::fs::metadata(output_file).map_err(|e| DecompileError::Walk(e.to_string()))?;
    if meta.len() == 0 {
        return Err(DecompileError::EmptyOutput);
    }

    Ok(DecompileStats {
        cs_file_count: 1,
        total_bytes: meta.len(),
        stdout_tail,
        stderr_tail,
        exit_code,
    })
}

fn ilspycmd_filenames() -> &'static [&'static str] {
    if cfg!(windows) {
        &["ilspycmd.exe", "ilspycmd"]
    } else {
        &["ilspycmd"]
    }
}

fn tail_lossy(bytes: &[u8], max_chars: usize) -> String {
    let s = String::from_utf8_lossy(bytes);
    if s.chars().count() <= max_chars {
        return s.into_owned();
    }
    let skip = s.chars().count() - max_chars;
    let tail: String = s.chars().skip(skip).collect();
    format!("...[truncated {skip} chars]\n{tail}")
}

fn count_cs_files(dir: &Path) -> std::io::Result<(u32, u64)> {
    let mut count: u32 = 0;
    let mut bytes: u64 = 0;
    let walker = walkdir::WalkDir::new(dir);
    for entry in walker.into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file()
            && p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("cs"))
        {
            count += 1;
            if let Ok(meta) = entry.metadata() {
                bytes += meta.len();
            }
        }
    }
    Ok((count, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discover_finds_binary_in_first_candidate_dir() {
        let td = tempfile::TempDir::new().unwrap();
        let d1 = td.path().join("d1");
        let d2 = td.path().join("d2");
        fs::create_dir_all(&d1).unwrap();
        fs::create_dir_all(&d2).unwrap();

        let bin_name = if cfg!(windows) { "ilspycmd.exe" } else { "ilspycmd" };
        fs::write(d2.join(bin_name), b"fake").unwrap();

        let dirs: Vec<&Path> = vec![d1.as_path(), d2.as_path()];
        let found = discover_ilspycmd_in(&dirs);
        assert_eq!(found, Some(d2.join(bin_name)));
    }

    #[test]
    fn discover_returns_none_when_absent() {
        let td = tempfile::TempDir::new().unwrap();
        let dirs: Vec<&Path> = vec![td.path()];
        assert!(discover_ilspycmd_in(&dirs).is_none());
    }

    #[test]
    fn run_decompile_project_errors_when_dll_missing() {
        let td = tempfile::TempDir::new().unwrap();
        let fake_dll = td.path().join("nope.dll");
        let out = td.path().join("out");
        let result = run_decompile_project(Path::new("/usr/bin/true"), &fake_dll, &out);
        match result {
            Err(DecompileError::DllMissing(p)) => assert_eq!(p, fake_dll),
            other => panic!("expected DllMissing, got {other:?}"),
        }
    }

    #[test]
    fn run_decompile_project_errors_when_ilspycmd_path_invalid() {
        let td = tempfile::TempDir::new().unwrap();
        let dll = td.path().join("fake.dll");
        fs::write(&dll, b"mz...").unwrap();
        let out = td.path().join("out");

        let bogus = td.path().join("does-not-exist-binary");
        let result = run_decompile_project(&bogus, &dll, &out);
        match result {
            Err(DecompileError::Spawn(msg)) => assert!(!msg.is_empty()),
            other => panic!("expected Spawn error, got {other:?}"),
        }
    }

    #[test]
    fn run_decompile_file_errors_when_dll_missing() {
        let td = tempfile::TempDir::new().unwrap();
        let fake_dll = td.path().join("nope.dll");
        let out = td.path().join("out.cs");
        let result = run_decompile_file(Path::new("/usr/bin/true"), &fake_dll, &out);
        assert!(matches!(result, Err(DecompileError::DllMissing(_))));
    }

    #[test]
    fn run_decompile_file_errors_when_ilspycmd_invalid() {
        let td = tempfile::TempDir::new().unwrap();
        let dll = td.path().join("fake.dll");
        fs::write(&dll, b"mz").unwrap();
        let out = td.path().join("out.cs");
        let bogus = td.path().join("nonexistent-cmd");
        let result = run_decompile_file(&bogus, &dll, &out);
        assert!(matches!(result, Err(DecompileError::Spawn(_))));
    }

    #[test]
    fn tail_lossy_truncates_long_input() {
        let bytes = "x".repeat(200).into_bytes();
        let out = tail_lossy(&bytes, 50);
        assert!(out.starts_with("...[truncated"));
    }

    #[test]
    fn count_cs_files_walks_nested() {
        let td = tempfile::TempDir::new().unwrap();
        fs::create_dir_all(td.path().join("a/b")).unwrap();
        fs::write(td.path().join("a.cs"), b"// 1").unwrap();
        fs::write(td.path().join("a/b/c.cs"), b"// 22").unwrap();
        fs::write(td.path().join("not_cs.txt"), b"skip me").unwrap();

        let (n, bytes) = count_cs_files(td.path()).unwrap();
        assert_eq!(n, 2);
        assert!(bytes >= 7); // "// 1" + "// 22" = 4 + 5
    }
}
