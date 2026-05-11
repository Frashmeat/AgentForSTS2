//! Knowledge commands —— 对标 `routes::knowledge`。

use std::path::PathBuf;

use ats_core::knowledge::{
    KnowledgePaths, KnowledgeStatus, get_status,
    pack::{self, ExportStats, ImportStats},
};
use tauri::State;

use crate::AppConfig;

#[tauri::command]
pub fn get_knowledge_status(config: State<'_, AppConfig>) -> KnowledgeStatus {
    let paths = KnowledgePaths::from_runtime_dir(&config.status.runtime_dir());
    get_status(&paths)
}

#[tauri::command]
pub fn check_knowledge_status(config: State<'_, AppConfig>) -> KnowledgeStatus {
    let paths = KnowledgePaths::from_runtime_dir(&config.status.runtime_dir());
    get_status(&paths)
}

/// 导出当前知识库到 zip 文件。`machine_hint` 写到 pack-info 里供使用者参考来源。
#[tauri::command]
pub async fn export_knowledge_pack(
    config: State<'_, AppConfig>,
    output_path: String,
    machine_hint: Option<String>,
) -> Result<ExportStats, String> {
    let paths = KnowledgePaths::from_runtime_dir(&config.status.runtime_dir());
    let out = PathBuf::from(output_path);
    tokio::task::spawn_blocking(move || pack::export(&paths, &out, machine_hint))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| e.to_string())
}

/// 从 zip 导入知识库。`overwrite=true` 覆盖现有内容。
#[tauri::command]
pub async fn import_knowledge_pack(
    config: State<'_, AppConfig>,
    input_path: String,
    overwrite: bool,
) -> Result<ImportStats, String> {
    let paths = KnowledgePaths::from_runtime_dir(&config.status.runtime_dir());
    let input = PathBuf::from(input_path);
    tokio::task::spawn_blocking(move || pack::import(&paths, &input, overwrite))
        .await
        .map_err(|e| format!("join: {e}"))?
        .map_err(|e| e.to_string())
}
