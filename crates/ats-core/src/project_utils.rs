//! 路径处理辅助函数，专门处理 Windows 中文路径 / 长路径 / 子进程参数传递。
//!
//! Rust 的 `PathBuf`（在 Windows 上底层是 OsString，存 UTF-16 LE）对中文名字
//! 路径处理普遍 OK——`std::process::Command::arg` / `tokio::fs::read_to_string`
//! 等都通过 OsStr 走 UTF-16 调用 Win32 W-API，不会丢字符。
//!
//! 真正容易踩坑的是：
//! 1. 超过 MAX_PATH（260 字符）的长路径——Windows 默认要 `\\?\` 前缀
//! 2. UI 显示时 `path.display()` 对非 UTF-8 字节序列会 replacement char
//! 3. PathBuf → String 序列化时同样 lossy
//!
//! 本模块只提供小而精的辅助函数，handler 在需要时显式调用。

use std::path::{Path, PathBuf};

/// Windows 上若路径长度接近 MAX_PATH 阈值，加上扩展长度前缀 `\\?\`。
/// 其它平台返回原路径不变。
///
/// 前缀让 Win32 API 接受 32k 字符的长路径，对深层嵌套 / 中文长路径有用。
#[must_use]
pub fn to_extended_length_path(p: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        // 已有前缀直接返回
        let s = p.to_string_lossy();
        if s.starts_with(r"\\?\") || s.starts_with(r"\\.\") {
            return p.to_path_buf();
        }
        // 短路径无需前缀（避免破坏 UNC / 相对路径行为）
        if !p.is_absolute() || s.chars().count() < 240 {
            return p.to_path_buf();
        }
        // UNC 路径 \\server\share → \\?\UNC\server\share
        if s.starts_with(r"\\") {
            let rest = s.trim_start_matches(r"\\");
            return PathBuf::from(format!(r"\\?\UNC\{rest}"));
        }
        PathBuf::from(format!(r"\\?\{s}"))
    }
    #[cfg(not(windows))]
    {
        p.to_path_buf()
    }
}

/// 安全显示路径（UTF-8 lossy）。等价于 `path.display().to_string()` 但意图明确。
#[must_use]
pub fn display_path_safe(p: &Path) -> String {
    p.display().to_string()
}

/// 检测路径是否含非 ASCII 字符（中文 / 日文 / Emoji 等）。
/// 用于诊断 / 日志，**不**作判定标准——大多数非 ASCII 路径在 Rust 下能正常工作。
#[must_use]
pub fn path_has_non_ascii(p: &Path) -> bool {
    !p.to_string_lossy().is_ascii()
}

/// 检测路径在 Windows 上是否可能踩 MAX_PATH 限制（260 字符阈值，留 20 缓冲）。
/// 非 Windows 平台始终返回 false。
#[must_use]
pub fn windows_long_path_risk(p: &Path) -> bool {
    #[cfg(windows)]
    {
        let len = p.to_string_lossy().chars().count();
        len >= 240
    }
    #[cfg(not(windows))]
    {
        let _ = p;
        false
    }
}

/// 返回路径诊断信息，用于在 health / status 接口里告诉用户"你的工程路径有无潜在问题"。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathDiagnostics {
    pub has_non_ascii: bool,
    pub windows_long_path_risk: bool,
    pub absolute: bool,
    pub char_count: u32,
}

#[must_use]
pub fn diagnose(p: &Path) -> PathDiagnostics {
    PathDiagnostics {
        has_non_ascii: path_has_non_ascii(p),
        windows_long_path_risk: windows_long_path_risk(p),
        absolute: p.is_absolute(),
        char_count: u32::try_from(p.to_string_lossy().chars().count()).unwrap_or(u32::MAX),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_ascii_path_passes_through_unchanged() {
        let p = Path::new("C:/foo/bar");
        let r = to_extended_length_path(p);
        assert_eq!(r, p);
    }

    #[test]
    fn non_ascii_detection_true_for_chinese() {
        assert!(path_has_non_ascii(Path::new(
            "E:/zuolan_lib/中文目录/foo.dll"
        )));
    }

    #[test]
    fn non_ascii_detection_false_for_ascii() {
        assert!(!path_has_non_ascii(Path::new("E:/zuolan_lib/foo.dll")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_long_path_risk_triggers_at_threshold() {
        let long = format!("C:/{}", "a".repeat(250));
        assert!(windows_long_path_risk(Path::new(&long)));
        let short = "C:/short";
        assert!(!windows_long_path_risk(Path::new(short)));
    }

    #[cfg(windows)]
    #[test]
    fn windows_long_absolute_path_gets_prefix() {
        let long = format!("C:\\{}", "a".repeat(260));
        let prefixed = to_extended_length_path(Path::new(&long));
        let s = prefixed.to_string_lossy();
        assert!(s.starts_with(r"\\?\"), "expected \\\\?\\ prefix, got: {s}");
    }

    #[cfg(windows)]
    #[test]
    fn windows_already_prefixed_path_unchanged() {
        let p = Path::new(r"\\?\C:\some\path");
        assert_eq!(to_extended_length_path(p), p);
    }

    #[cfg(windows)]
    #[test]
    fn windows_relative_path_not_prefixed_even_if_long() {
        let long = format!("foo/{}", "a".repeat(260));
        let r = to_extended_length_path(Path::new(&long));
        // relative path: 不加前缀（避免破坏行为）
        assert!(!r.to_string_lossy().starts_with(r"\\?\"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_unc_path_gets_unc_prefix() {
        // 240+ chars to trigger the long path heuristic; UNC + many segments
        let unc = format!(r"\\server\share\{}", "a".repeat(230));
        let r = to_extended_length_path(Path::new(&unc));
        let s = r.to_string_lossy();
        assert!(
            s.starts_with(r"\\?\UNC\server\share"),
            "expected UNC extended prefix, got: {s}"
        );
    }

    #[test]
    fn diagnose_collects_fields() {
        let d = diagnose(Path::new("E:/中文/foo.dll"));
        assert!(d.has_non_ascii);
        assert!(d.char_count > 0);
        // 短路径不会触发 long_path_risk
        assert!(!d.windows_long_path_risk);
    }
}
