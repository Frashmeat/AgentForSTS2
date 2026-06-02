//! 本地能力检测（原 Python `workstation_capabilities`，决议 Q3 改名后只做"本机"
//! 体检，无远端协议）。
//!
//! 给前端体检卡片 + Stage 5 health 报告填详细字段。设计上同步快路径（OS/arch/
//! CPU）+ async 慢路径（subprocess 探测 dotnet/ilspycmd 版本）。

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::knowledge::{default_dotnet_tools_dirs, discover_ilspycmd};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalCapabilities {
    /// OS family，例 "windows" / "macos" / "linux"
    pub os: String,
    /// CPU arch，例 "x86_64" / "aarch64"
    pub arch: String,
    /// available_parallelism()；0 表示检测失败
    pub cpu_count: u32,
    /// 是否找到 ilspycmd（PATH + ~/.dotnet/tools 自动发现）
    pub ilspycmd_found: bool,
    pub ilspycmd_path: Option<PathBuf>,
    /// `dotnet --version` 输出（已 trim）。None 表示找不到或调用失败。
    pub dotnet_version: Option<String>,
    /// 简短诊断列表，UI 可红/黄/绿色显示
    pub warnings: Vec<String>,
}

/// 同步快路径：填 OS / arch / CPU / ilspycmd 发现。不跑任何子进程。
#[must_use]
pub fn detect_sync() -> LocalCapabilities {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(0);

    let extras = default_dotnet_tools_dirs();
    let ilspycmd_path = discover_ilspycmd(&extras);
    let ilspycmd_found = ilspycmd_path.is_some();

    let mut warnings: Vec<String> = Vec::new();
    if !ilspycmd_found {
        warnings.push(
            "ilspycmd 未找到（PATH + ~/.dotnet/tools 都没有）。运行 `dotnet tool install -g ilspycmd` 安装。"
                .into(),
        );
    }
    if cpu_count == 0 {
        warnings.push("无法检测 CPU 核心数。".into());
    }

    LocalCapabilities {
        os,
        arch,
        cpu_count,
        ilspycmd_found,
        ilspycmd_path,
        dotnet_version: None,
        warnings,
    }
}

/// 完整检测（含 dotnet --version 子进程）。
///
/// 不会在 detect_sync 的基础上重新构造，只是 await 子进程探测后补 `dotnet_version`。
pub async fn detect_full() -> LocalCapabilities {
    let mut caps = detect_sync();
    caps.dotnet_version = detect_dotnet_version().await;
    if caps.dotnet_version.is_none() {
        caps.warnings
            .push("dotnet SDK 未检测到。运行 `dotnet --version` 看是否能输出版本号。".into());
    }
    caps
}

async fn detect_dotnet_version() -> Option<String> {
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        tokio::task::spawn_blocking(|| {
            std::process::Command::new("dotnet")
                .arg("--version")
                .output()
        }),
    )
    .await;
    match result {
        Ok(Ok(Ok(out))) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_sync_fills_os_and_arch() {
        let caps = detect_sync();
        assert!(!caps.os.is_empty());
        assert!(!caps.arch.is_empty());
        // CPU 核心数应至少 1（除非异常环境）
        // 不强断言因为 CI sandbox 也许有怪异行为
    }

    #[test]
    fn detect_sync_warns_when_ilspycmd_absent() {
        // 在 CI / 大多数测试环境下不预装 ilspycmd → 应有 warning
        let caps = detect_sync();
        if !caps.ilspycmd_found {
            assert!(
                caps.warnings.iter().any(|w| w.contains("ilspycmd")),
                "expected ilspycmd warning when not found"
            );
        }
    }

    #[tokio::test]
    async fn detect_full_does_not_hang() {
        // 此测试不强断言 dotnet_version 存在（CI 可能没装 dotnet），只验证 5s 超时
        // 起作用、函数能 return。
        let caps = detect_full().await;
        // 至少 os/arch 填了
        assert!(!caps.os.is_empty());
    }
}
