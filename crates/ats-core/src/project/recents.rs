//! 最近打开工程列表（LRU，最多 10 条）。

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::error::ProjectResult;
use super::folder::ProjectMeta;

const MAX_RECENTS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct RecentEntry {
    pub path: String,
    pub name: String,
    pub last_opened_at: chrono::DateTime<chrono::Utc>,
}

impl Default for RecentEntry {
    fn default() -> Self {
        Self {
            path: String::new(),
            name: String::new(),
            last_opened_at: chrono::Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct RecentProjects {
    pub items: Vec<RecentEntry>,
}

impl RecentProjects {
    /// 读取 recent_projects.json；文件缺失/格式错误时返回空列表（永不报错）。
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str(&text) {
            Ok(parsed) => parsed,
            Err(_) => {
                // 损坏文件不静默丢弃：改名 .corrupt 备查（best-effort），再返回空列表。
                // 下次 save 才写出新文件，用户可从 .corrupt 找回旧数据。
                let _ = fs::rename(path, path.with_extension("json.corrupt"));
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> ProjectResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_vec_pretty(self)?;
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, &serialized)?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// 把工程加到最近列表（按 path dedup，移到最前，截断到 MAX_RECENTS）。
    pub fn record(&mut self, project_path: &Path, meta: &ProjectMeta) {
        let path_str = project_path.display().to_string();
        self.items.retain(|e| e.path != path_str);
        self.items.insert(
            0,
            RecentEntry {
                path: path_str,
                name: meta.name.clone(),
                last_opened_at: chrono::Utc::now(),
            },
        );
        if self.items.len() > MAX_RECENTS {
            self.items.truncate(MAX_RECENTS);
        }
    }

    /// 从列表中移除一条（用于"忘记"操作）。
    pub fn forget(&mut self, project_path: &Path) {
        let path_str = project_path.display().to_string();
        self.items.retain(|e| e.path != path_str);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn meta(name: &str) -> ProjectMeta {
        ProjectMeta {
            name: name.into(),
            ..ProjectMeta::default()
        }
    }

    #[test]
    fn record_dedups_and_moves_to_front() {
        let mut r = RecentProjects::default();
        r.record(&PathBuf::from("/p/a"), &meta("a"));
        r.record(&PathBuf::from("/p/b"), &meta("b"));
        r.record(&PathBuf::from("/p/a"), &meta("a")); // dup
        assert_eq!(r.items.len(), 2);
        assert_eq!(r.items[0].path, "/p/a");
        assert_eq!(r.items[1].path, "/p/b");
    }

    #[test]
    fn record_caps_at_max() {
        let mut r = RecentProjects::default();
        for i in 0..15 {
            r.record(&PathBuf::from(format!("/p/{i}")), &meta(&format!("p{i}")));
        }
        assert_eq!(r.items.len(), MAX_RECENTS);
        assert_eq!(r.items[0].path, "/p/14"); // 最新在最前
    }

    #[test]
    fn forget_removes_entry() {
        let mut r = RecentProjects::default();
        r.record(&PathBuf::from("/p/a"), &meta("a"));
        r.record(&PathBuf::from("/p/b"), &meta("b"));
        r.forget(&PathBuf::from("/p/a"));
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].path, "/p/b");
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let p = std::env::temp_dir().join("ats-test-nonexistent-recents-xyz.json");
        let _ = fs::remove_file(&p);
        let r = RecentProjects::load(&p);
        assert!(r.items.is_empty());
    }

    #[test]
    fn load_corrupt_file_renames_to_corrupt_not_silent_wipe() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("recent_projects.json");
        fs::write(&path, b"NOT JSON {{{").unwrap();
        let r = RecentProjects::load(&path);
        assert!(r.items.is_empty());
        // 原文件被改名备查，而非静默清空。
        assert!(!path.exists());
        assert!(path.with_extension("json.corrupt").exists());
    }

    #[test]
    fn save_then_load_round_trip() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("r.json");
        let mut r = RecentProjects::default();
        r.record(&PathBuf::from("/p/x"), &meta("x"));
        r.save(&path).unwrap();
        let loaded = RecentProjects::load(&path);
        assert_eq!(loaded.items.len(), 1);
        assert_eq!(loaded.items[0].name, "x");
    }
}
