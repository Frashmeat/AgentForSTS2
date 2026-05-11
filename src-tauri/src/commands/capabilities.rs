//! 本机能力检测命令。
//!
//! sync = 仅 OS/arch/CPU/ilspycmd 发现，0ms 返回
//! full = 同步 + dotnet --version 子进程（最多 5s）

use ats_core::capabilities::{LocalCapabilities, detect_full, detect_sync};

#[tauri::command]
pub fn get_local_capabilities_sync() -> LocalCapabilities {
    detect_sync()
}

#[tauri::command]
pub async fn get_local_capabilities_full() -> LocalCapabilities {
    detect_full().await
}
