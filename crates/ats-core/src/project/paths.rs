//! OS App Data 目录解析。
//!
//! Windows : `%APPDATA%/AgentTheSpire/`
//! macOS   : `~/Library/Application Support/AgentTheSpire/`
//! Linux   : `~/.local/share/AgentTheSpire/` (或 `$XDG_DATA_HOME/AgentTheSpire/`)

use std::fs;
use std::io;
use std::path::PathBuf;

const APP_DIR_NAME: &str = "AgentTheSpire";

#[derive(Debug, Clone)]
pub struct AppDataPaths {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub credentials_path: PathBuf,
    pub recent_projects_path: PathBuf,
    pub knowledge_root: PathBuf,
    pub logs_root: PathBuf,
}

impl AppDataPaths {
    /// 用 `dirs::data_dir` 解析平台标准路径；找不到时退到 `~/.AgentTheSpire`。
    #[must_use]
    pub fn resolve() -> Self {
        let root = dirs::data_dir()
            .or_else(dirs::home_dir)
            .map(|d| d.join(APP_DIR_NAME))
            .unwrap_or_else(|| PathBuf::from(format!("./{APP_DIR_NAME}")));
        Self::from_root(root)
    }

    /// 测试 / 自定义场景下显式注入 root。
    #[must_use]
    pub fn from_root(root: PathBuf) -> Self {
        Self {
            config_path: root.join("config.json"),
            credentials_path: root.join("credentials.json"),
            recent_projects_path: root.join("recent_projects.json"),
            knowledge_root: root.join("knowledge"),
            logs_root: root.join("logs"),
            root,
        }
    }

    /// 创建所有子目录（已存在则跳过）。
    pub fn ensure_dirs(&self) -> io::Result<()> {
        for dir in [&self.root, &self.knowledge_root, &self.logs_root] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_root_layout_correct() {
        let p = AppDataPaths::from_root(PathBuf::from("/tmp/ats-test"));
        assert!(p.config_path.ends_with("config.json"));
        assert!(p.credentials_path.ends_with("credentials.json"));
        assert!(p.recent_projects_path.ends_with("recent_projects.json"));
        assert!(p.knowledge_root.ends_with("knowledge"));
    }

    #[test]
    fn ensure_dirs_creates_all() {
        let temp = tempdir();
        let p = AppDataPaths::from_root(temp.path().join("app"));
        p.ensure_dirs().unwrap();
        assert!(p.root.is_dir());
        assert!(p.knowledge_root.is_dir());
        assert!(p.logs_root.is_dir());
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempdir")
    }
}
