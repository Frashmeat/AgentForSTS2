//! sts2.dll 自动探测。
//!
//! 尝试顺序：
//! 1. 常见的 Steam 默认路径
//! 2. 遍历 D:-Z: 盘符找 SteamLibrary/steamapps/...

use std::path::PathBuf;

const STS2_REL: &str = "steamapps/common/Slay the Spire 2/data_sts2_windows_x86_64/sts2.dll";

/// 返回找到的第一个 sts2.dll 路径。都找不到返回 None。
#[must_use]
pub fn discover_sts2_dll() -> Option<String> {
    // 1. 已知默认路径
    for root in [
        r"C:\Program Files (x86)\Steam",
        r"D:\SteamLibrary",
        r"E:\SteamLibrary",
        r"F:\SteamLibrary",
    ] {
        let candidate = PathBuf::from(root).join(STS2_REL);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    // 2. 扫描盘符
    for drive_letter in 'D'..='Z' {
        let root = format!("{drive_letter}:\\");
        let candidate = PathBuf::from(&root).join("SteamLibrary").join(STS2_REL);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}
