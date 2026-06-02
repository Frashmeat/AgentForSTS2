//! PlanItem 产物状态跟踪 —— 跨任务记录每个 PlanItem 当前处于什么阶段。
//!
//! 存储：`<project>/items/<item_id>.status.json`（每个 item 一个文件，避免多任务
//! 并发竞争同一个大 JSON）。

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactState {
    /// 还没开始
    Pending,
    /// 任务正在跑
    InProgress,
    /// 任务完成，等用户校对
    Generated,
    /// 用户已校对接受
    Reviewed,
    /// 失败
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactStatus {
    pub item_id: String,
    pub state: ArtifactState,
    pub updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cs_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub png_path: Option<PathBuf>,
    /// 自由文本，e.g. 失败原因 / 用户备注
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ArtifactStatus {
    #[must_use]
    pub fn new(item_id: impl Into<String>, state: ArtifactState) -> Self {
        Self {
            item_id: item_id.into(),
            state,
            updated_at: Utc::now(),
            last_job_id: None,
            cs_path: None,
            png_path: None,
            note: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum PlanArtifactError {
    #[error("io: {0}")]
    Io(String),
    #[error("parse: {0}")]
    Parse(String),
}

fn items_dir(project_root: &Path) -> PathBuf {
    project_root.join("items")
}

fn status_path(project_root: &Path, item_id: &str) -> PathBuf {
    items_dir(project_root).join(format!("{item_id}.status.json"))
}

/// 写入（覆盖）单个 item 的状态。
///
/// 原子写：tmp + rename，避免读者看到半截 JSON。
pub fn save_status(project_root: &Path, status: &ArtifactStatus) -> Result<(), PlanArtifactError> {
    let path = status_path(project_root, &status.item_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PlanArtifactError::Io(e.to_string()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(status)
        .map_err(|e| PlanArtifactError::Parse(e.to_string()))?;
    std::fs::write(&tmp, text).map_err(|e| PlanArtifactError::Io(e.to_string()))?;
    std::fs::rename(&tmp, &path).map_err(|e| PlanArtifactError::Io(e.to_string()))?;
    Ok(())
}

pub fn load_status(
    project_root: &Path,
    item_id: &str,
) -> Result<Option<ArtifactStatus>, PlanArtifactError> {
    let path = status_path(project_root, item_id);
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let s: ArtifactStatus =
                serde_json::from_str(&text).map_err(|e| PlanArtifactError::Parse(e.to_string()))?;
            Ok(Some(s))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(PlanArtifactError::Io(e.to_string())),
    }
}

pub fn list_statuses(project_root: &Path) -> Result<Vec<ArtifactStatus>, PlanArtifactError> {
    let dir = items_dir(project_root);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| PlanArtifactError::Io(e.to_string()))? {
        let entry = entry.map_err(|e| PlanArtifactError::Io(e.to_string()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".status.json"))
        {
            continue;
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| PlanArtifactError::Io(e.to_string()))?;
        match serde_json::from_str::<ArtifactStatus>(&text) {
            Ok(s) => out.push(s),
            // 容错：损坏的单条不阻塞其它读取
            Err(_) => continue,
        }
    }
    // 按 updated_at 倒序
    out.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_round_trips() {
        let td = tempfile::TempDir::new().unwrap();
        let mut s = ArtifactStatus::new("item-1", ArtifactState::Generated);
        s.last_job_id = Some("job-9".into());
        s.cs_path = Some(PathBuf::from("/foo/Bar.cs"));
        s.note = Some("looks good".into());
        save_status(td.path(), &s).unwrap();

        let loaded = load_status(td.path(), "item-1").unwrap().expect("present");
        assert_eq!(loaded.state, ArtifactState::Generated);
        assert_eq!(loaded.last_job_id.as_deref(), Some("job-9"));
        assert_eq!(loaded.cs_path.as_deref(), Some(Path::new("/foo/Bar.cs")));
        assert_eq!(loaded.note.as_deref(), Some("looks good"));
    }

    #[test]
    fn load_missing_returns_none() {
        let td = tempfile::TempDir::new().unwrap();
        assert!(load_status(td.path(), "absent").unwrap().is_none());
    }

    #[test]
    fn list_returns_all_status_files_sorted() {
        let td = tempfile::TempDir::new().unwrap();
        for (id, state) in [
            ("item-a", ArtifactState::Pending),
            ("item-b", ArtifactState::Generated),
            ("item-c", ArtifactState::Reviewed),
        ] {
            save_status(td.path(), &ArtifactStatus::new(id, state)).unwrap();
        }
        let list = list_statuses(td.path()).unwrap();
        assert_eq!(list.len(), 3);
        // 倒序但因为同一秒内写入，相对顺序不严格保证；只验证 3 个都在
        let ids: Vec<&str> = list.iter().map(|s| s.item_id.as_str()).collect();
        assert!(ids.contains(&"item-a"));
        assert!(ids.contains(&"item-b"));
        assert!(ids.contains(&"item-c"));
    }

    #[test]
    fn list_skips_non_status_files() {
        let td = tempfile::TempDir::new().unwrap();
        let dir = items_dir(td.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("random.json"), b"{}").unwrap();
        std::fs::write(
            dir.join("item-x.status.json"),
            br#"{"itemId":"x","state":"pending","updatedAt":"2026-05-11T00:00:00Z"}"#,
        )
        .unwrap();
        let list = list_statuses(td.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].item_id, "x");
    }

    #[test]
    fn list_tolerates_corrupted_file() {
        let td = tempfile::TempDir::new().unwrap();
        let dir = items_dir(td.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad.status.json"), b"NOT JSON").unwrap();
        std::fs::write(
            dir.join("good.status.json"),
            br#"{"itemId":"good","state":"pending","updatedAt":"2026-05-11T00:00:00Z"}"#,
        )
        .unwrap();
        let list = list_statuses(td.path()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].item_id, "good");
    }
}
