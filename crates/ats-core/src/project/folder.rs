//! ProjectFolder —— 工程目录抽象。
//!
//! 工程目录布局（来自计划 §2.3.0）：
//! ```text
//! <project_root>/
//! ├── project.json                # ProjectMeta 序列化
//! ├── plan.json                   # ModPlan（后续模块写入）
//! ├── items/                      # 每 item 状态（后续模块）
//! ├── artifacts/                  # 生成产物（后续模块）
//! ├── history/                    # 任务历史（后续模块）
//! └── .ats/
//!     ├── lock                    # 进程级排他锁（RAII）
//!     └── version                 # schema 版本号
//! ```
//!
//! 当前 stage 只保证 project.json + .ats/{lock,version} + 各空目录创建；
//! 业务持久化随后续模块陆续填入。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use super::error::{ProjectError, ProjectResult};

pub const PROJECT_SCHEMA_VERSION: u32 = 1;

const PROJECT_JSON: &str = "project.json";
const ATS_DIR: &str = ".ats";
const LOCK_FILE: &str = "lock";
const VERSION_FILE: &str = "version";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ProjectMeta {
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub schema_version: u32,
    pub sts2_path: Option<String>,
    pub template_version: Option<String>,
}

impl Default for ProjectMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            created_at: chrono::Utc::now(),
            schema_version: PROJECT_SCHEMA_VERSION,
            sts2_path: None,
            template_version: None,
        }
    }
}

#[derive(Debug)]
pub struct ProjectFolder {
    path: PathBuf,
    meta: ProjectMeta,
    _lock: ProjectLock,
}

impl ProjectFolder {
    /// 在 `parent_dir/<name>` 处创建全新工程。`name` 校验：非空、不含 `/\:*?"<>|`。
    pub fn create(parent_dir: &Path, name: &str) -> ProjectResult<Self> {
        validate_name(name)?;
        if !parent_dir.is_dir() {
            return Err(ProjectError::NotADirectory(parent_dir.display().to_string()));
        }
        let project_root = parent_dir.join(name);
        if project_root.exists() {
            return Err(ProjectError::AlreadyExists(project_root.display().to_string()));
        }
        fs::create_dir_all(&project_root)?;
        for sub in ["items", "artifacts", "history", ATS_DIR] {
            fs::create_dir_all(project_root.join(sub))?;
        }
        let meta = ProjectMeta {
            name: name.to_string(),
            ..ProjectMeta::default()
        };
        write_json_atomic(&project_root.join(PROJECT_JSON), &meta)?;
        write_text_atomic(
            &project_root.join(ATS_DIR).join(VERSION_FILE),
            &PROJECT_SCHEMA_VERSION.to_string(),
        )?;
        let lock = ProjectLock::acquire(&project_root)?;
        Ok(Self {
            path: project_root,
            meta,
            _lock: lock,
        })
    }

    /// 打开已有工程：读 project.json + 持有 lock。
    pub fn open(path: &Path) -> ProjectResult<Self> {
        if !path.is_dir() {
            return Err(ProjectError::NotADirectory(path.display().to_string()));
        }
        let project_json = path.join(PROJECT_JSON);
        if !project_json.is_file() {
            return Err(ProjectError::Missing(project_json.display().to_string()));
        }
        let text = fs::read_to_string(&project_json)?;
        let meta: ProjectMeta = serde_json::from_str(&text)?;
        // 工程目录可能在历史版本中未创建 .ats/，幂等补齐。
        let ats_dir = path.join(ATS_DIR);
        fs::create_dir_all(&ats_dir)?;
        let lock = ProjectLock::acquire(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            meta,
            _lock: lock,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn meta(&self) -> &ProjectMeta {
        &self.meta
    }

    pub fn save_meta(&mut self, meta: ProjectMeta) -> ProjectResult<()> {
        write_json_atomic(&self.path.join(PROJECT_JSON), &meta)?;
        self.meta = meta;
        Ok(())
    }

    #[must_use]
    pub fn plan_json_path(&self) -> PathBuf {
        self.path.join("plan.json")
    }
    #[must_use]
    pub fn items_dir(&self) -> PathBuf {
        self.path.join("items")
    }
    #[must_use]
    pub fn artifacts_dir(&self) -> PathBuf {
        self.path.join("artifacts")
    }
    #[must_use]
    pub fn history_dir(&self) -> PathBuf {
        self.path.join("history")
    }
}

fn validate_name(name: &str) -> ProjectResult<()> {
    if name.trim().is_empty() {
        return Err(ProjectError::InvalidName(name.to_string()));
    }
    if name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
        return Err(ProjectError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// 原子写入：先写 `<path>.tmp` 再 rename 到目标，避免半截文件。
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> ProjectResult<()> {
    let serialized = serde_json::to_vec_pretty(value)?;
    write_bytes_atomic(path, &serialized)
}

fn write_text_atomic(path: &Path, text: &str) -> ProjectResult<()> {
    write_bytes_atomic(path, text.as_bytes())
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> ProjectResult<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all().ok(); // best effort，部分平台/网络盘可能不支持
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// 简单 lock：存在 lock 文件即视为已锁。Drop 时移除。
///
/// **限制**：当前不做跨进程真锁——并发开启同一目录会让先创建者保持，
/// 后开启者看到 Locked 错误。stage 5 之前升级到 fs2 `lock_exclusive`。
#[derive(Debug)]
struct ProjectLock {
    path: PathBuf,
    released: AtomicBool,
}

impl ProjectLock {
    fn acquire(project_root: &Path) -> ProjectResult<Self> {
        let lock_path = project_root.join(ATS_DIR).join(LOCK_FILE);
        if lock_path.exists() {
            return Err(ProjectError::Locked(project_root.display().to_string()));
        }
        // 父目录可能在 create() / open() 已经创建，但保险起见再 ensure。
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let pid_text = format!("{}", std::process::id());
        write_text_atomic(&lock_path, &pid_text)?;
        Ok(Self {
            path: lock_path,
            released: AtomicBool::new(false),
        })
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        if !self.released.swap(true, Ordering::SeqCst) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempdir")
    }

    #[test]
    fn create_then_open_round_trip() {
        let td = tempdir();
        let parent = td.path();
        let pf = ProjectFolder::create(parent, "demo").expect("create");
        assert_eq!(pf.meta().name, "demo");
        assert!(pf.path().join("project.json").is_file());
        assert!(pf.path().join(".ats/lock").is_file());
        assert!(pf.items_dir().is_dir());
        let path = pf.path().to_path_buf();
        drop(pf); // 释放 lock

        let reopened = ProjectFolder::open(&path).expect("open");
        assert_eq!(reopened.meta().name, "demo");
    }

    #[test]
    fn create_rejects_existing_dir() {
        let td = tempdir();
        fs::create_dir_all(td.path().join("dup")).unwrap();
        let err = ProjectFolder::create(td.path(), "dup").unwrap_err();
        assert!(matches!(err, ProjectError::AlreadyExists(_)));
    }

    #[test]
    fn open_locked_project_fails() {
        let td = tempdir();
        let pf = ProjectFolder::create(td.path(), "x").unwrap();
        let path = pf.path().to_path_buf();
        let err = ProjectFolder::open(&path).unwrap_err();
        assert!(matches!(err, ProjectError::Locked(_)));
        drop(pf);
        // 释放后可以重新打开
        let _again = ProjectFolder::open(&path).expect("reopen after drop");
    }

    #[test]
    fn invalid_name_rejected() {
        let td = tempdir();
        assert!(matches!(
            ProjectFolder::create(td.path(), "bad/name").unwrap_err(),
            ProjectError::InvalidName(_)
        ));
        assert!(matches!(
            ProjectFolder::create(td.path(), "").unwrap_err(),
            ProjectError::InvalidName(_)
        ));
    }

    #[test]
    fn save_meta_persists() {
        let td = tempdir();
        let mut pf = ProjectFolder::create(td.path(), "demo").unwrap();
        let mut meta = pf.meta().clone();
        meta.sts2_path = Some("E:/STS2".into());
        pf.save_meta(meta).unwrap();
        let path = pf.path().to_path_buf();
        drop(pf);
        let reopened = ProjectFolder::open(&path).unwrap();
        assert_eq!(reopened.meta().sts2_path.as_deref(), Some("E:/STS2"));
    }
}
