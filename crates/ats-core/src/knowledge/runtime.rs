//! Knowledge 运行时——目录确保 + 状态计算。
//!
//! 业务规则镜像 Python `knowledge_runtime.get_knowledge_status`，但精简到本阶段
//! 不依赖 ilspycmd / GitHub release：只判断本地反编译产物是否到位 + 提供 warning。

use std::fs;
use std::io;
use std::path::Path;

use super::models::{BaselibStatus, GameStatus, KnowledgeStatus, OverallState, SourceMode};
use super::paths::KnowledgePaths;
use super::templates::TEMPLATE_SLOTS;

/// 创建知识库各子目录（已存在则跳过）。
pub fn ensure_dirs(paths: &KnowledgePaths) -> io::Result<()> {
    for dir in [
        &paths.root,
        &paths.game_dir,
        &paths.baselib_dir,
        &paths.resources_dir,
        &paths.cache_dir,
        &paths.packs_dir,
    ] {
        fs::create_dir_all(dir)?;
    }
    Ok(())
}

/// 返回当前知识库状态。`ensure_dirs` 由调用方负责（status 读取本身不创建目录）。
#[must_use]
pub fn get_status(paths: &KnowledgePaths) -> KnowledgeStatus {
    let game_has_cs = has_cs_files(&paths.game_dir, 4);
    let baselib_file = paths.baselib_decompiled_file();
    let baselib_exists = baselib_file.exists();

    let mut warnings: Vec<String> = Vec::new();
    if !game_has_cs {
        warnings.push(
            "游戏反编译源码目录为空——需要先执行知识库更新（stage 4 接入 ilspycmd 调度）".into(),
        );
    }
    if !baselib_exists {
        warnings.push("BaseLib 反编译结果缺失——需要先执行知识库更新".into());
    }

    let game = GameStatus {
        source_mode: if game_has_cs {
            SourceMode::RuntimeDecompiled
        } else {
            SourceMode::Missing
        },
        knowledge_path: paths.game_dir.display().to_string(),
        has_decompiled_sources: game_has_cs,
    };
    let baselib = BaselibStatus {
        source_mode: if baselib_exists {
            SourceMode::RuntimeDecompiled
        } else {
            SourceMode::Missing
        },
        knowledge_path: paths.baselib_dir.display().to_string(),
        has_decompiled_sources: baselib_exists,
    };

    let overall = if game_has_cs && baselib_exists && warnings.is_empty() {
        OverallState::Fresh
    } else {
        OverallState::Missing
    };

    KnowledgeStatus {
        overall,
        knowledge_root: paths.root.display().to_string(),
        warnings,
        game,
        baselib,
        embedded_templates: TEMPLATE_SLOTS.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// 探测知识源模式：检查 game_dir 下是否有 .cs 反编译产物。
/// 有 → RuntimeDecompiled，无 → Missing。不依赖 manifest，只看文件存在性。
#[must_use]
pub fn detect_source_mode(paths: &KnowledgePaths) -> SourceMode {
    if has_cs_files(&paths.game_dir, 4) {
        SourceMode::RuntimeDecompiled
    } else {
        SourceMode::Missing
    }
}

/// 递归探测目录下是否存在 `.cs` 文件，限制最大深度防止失控。
fn has_cs_files(root: &Path, max_depth: usize) -> bool {
    if max_depth == 0 || !root.exists() {
        return false;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_file()
            && path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("cs"))
        {
            return true;
        }
        if file_type.is_dir() && has_cs_files(&path, max_depth - 1) {
            return true;
        }
    }
    false
}
