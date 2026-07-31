//! mod_analyzer command —— 分析已存在的 mod 项目结构。
//!
//! 同步操作（几十毫秒），不进 Run 框架。

use std::path::PathBuf;

use crate::commands::failure::{CommandFailure, CommandResult};
use ats_core::mod_analyzer::{ModAnalysisReport, analyze};

#[tauri::command]
pub fn analyze_mod_project(project_root: String) -> CommandResult<ModAnalysisReport> {
    analyze(&PathBuf::from(project_root))
        .map_err(|_| CommandFailure::unclassified("mod_analyzer.analyze"))
}
