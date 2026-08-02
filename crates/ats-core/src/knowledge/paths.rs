//! Knowledge 目录布局。镜像 Python 现 `runtime/knowledge/...`。

use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct KnowledgePaths {
    pub root: PathBuf,
    pub game_dir: PathBuf,
    pub baselib_dir: PathBuf,
    pub resources_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub packs_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub active_pack_path: PathBuf,
}

const BASELIB_DECOMPILED_FILE: &str = "BaseLib.decompiled.cs";

impl KnowledgePaths {
    /// `runtime_dir` points at the role-owned mutable runtime root. Desktop
    /// composition uses OS app-data; Web composition may explicitly anchor it
    /// beside the server config.
    #[must_use]
    pub fn from_runtime_dir(runtime_dir: &Path) -> Self {
        let root = runtime_dir.join("knowledge");
        Self {
            game_dir: root.join("game"),
            baselib_dir: root.join("baselib"),
            resources_dir: root.join("resources").join("sts2"),
            cache_dir: root.join("cache"),
            packs_dir: root.join("packs"),
            manifest_path: root.join("knowledge-manifest.json"),
            active_pack_path: root.join("active-knowledge-pack.json"),
            root,
        }
    }

    pub fn baselib_decompiled_file(&self) -> PathBuf {
        self.baselib_dir.join(BASELIB_DECOMPILED_FILE)
    }
}
