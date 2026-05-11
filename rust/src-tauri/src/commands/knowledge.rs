//! Knowledge commands —— 对标 `routes::knowledge`。

use ats_core::knowledge::{KnowledgePaths, KnowledgeStatus, get_status};
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
