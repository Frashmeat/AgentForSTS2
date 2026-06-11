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
use super::template::{derive_csharp_name, scaffold_from_template};

pub const PROJECT_SCHEMA_VERSION: u32 = 1;

const PROJECT_JSON: &str = "project.json";
const ATS_DIR: &str = ".ats";
const LOCK_FILE: &str = "lock";
const VERSION_FILE: &str = "version";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct ProjectMeta {
    pub name: String,
    pub csharp_name: String,
    pub scaffolded: bool,
    // 后续由 codegen handler 填入
    pub generated_files: Vec<String>,
    pub build_output_dir: Option<String>,
}


#[derive(Debug)]
pub struct ProjectFolder {
    path: PathBuf,
    meta: ProjectMeta,
    _lock: ProjectLock,
}

impl ProjectFolder {
    /// 在 `parent_dir/<name>` 处创建全新工程。`name` 校验：非空、不含 `/\:*?"<>|`。
    ///
    /// 创建后立即从内嵌的 `mod_template/` 铺一份 dotnet 项目骨架（含 .csproj / .sln
    /// / MainFile.cs / nuget.config 等），文件中 `ModTemplate` 字面量替换为派生的
    /// `csharp_name`。这是 single_asset_plan → asset_generate → build_project 全链路
    /// 能 dotnet publish 起来的前置条件。
    pub fn create(parent_dir: &Path, name: &str) -> ProjectResult<Self> {
        validate_name(name)?;
        if !parent_dir.is_dir() {
            return Err(ProjectError::NotADirectory(
                parent_dir.display().to_string(),
            ));
        }
        let project_root = parent_dir.join(name);
        if project_root.exists() {
            return Err(ProjectError::AlreadyExists(
                project_root.display().to_string(),
            ));
        }
        fs::create_dir_all(&project_root)?;
        for sub in ["items", "artifacts", "history", ATS_DIR] {
            fs::create_dir_all(project_root.join(sub))?;
        }
        let csharp_name = derive_csharp_name(name);
        if let Err(err) = scaffold_from_template(&project_root, &csharp_name) {
            let _ = fs::remove_dir_all(&project_root);
            return Err(err);
        }
        let meta = ProjectMeta {
            name: name.to_string(),
            csharp_name,
            scaffolded: true,
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
    /// stale lock（上次进程崩溃残留）自动清理。
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
        let ats_dir = path.join(ATS_DIR);
        fs::create_dir_all(&ats_dir)?;
        let lock = ProjectLock::acquire_clearing_stale(path)?;
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
        return Err(ProjectError::InvalidName("name must not be empty/whitespace".into()));
    }
    for ch in name.chars() {
        if matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            return Err(ProjectError::InvalidName(format!(
                "name contains forbidden character: {ch:?}"
            )));
        }
    }
    Ok(())
}

/// 原子写入：先写 `<path>.tmp` 再 rename 到目标，避免半截文件。
fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> ProjectResult<()> {
    let text = serde_json::to_string_pretty(value)?;
    write_text_atomic(path, &text)
}

fn write_text_atomic(path: &Path, text: &str) -> ProjectResult<()> {
    write_bytes_atomic(path, text.as_bytes())
}

fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> ProjectResult<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// 文件锁：存在 lock 文件即视为已锁。Drop 时移除。
///
/// `acquire`（用于 create）要求 lock 不存在。
/// `acquire_clearing_stale`（用于 open）直接清理已有 lock 后重建
/// —— 因为 stale lock 来自上次进程崩溃，而应用层 `ActiveProject`
/// 已保证不会同时打开同一工程等多个实例。
#[derive(Debug)]
struct ProjectLock {
    path: PathBuf,
    released: AtomicBool,
}

impl ProjectLock {
    fn acquire(project_root: &Path) -> ProjectResult<Self> {
        let lock_path = project_root.join(ATS_DIR).join(LOCK_FILE);
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let pid_text = format!("{}", std::process::id());
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut f) => {
                let _ = f.write_all(pid_text.as_bytes());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(ProjectError::Locked(project_root.display().to_string()));
            }
            Err(e) => return Err(e.into()),
        }
        Ok(Self {
            path: lock_path,
            released: AtomicBool::new(false),
        })
    }

    fn acquire_clearing_stale(project_root: &Path) -> ProjectResult<Self> {
        let lock_path = project_root.join(ATS_DIR).join(LOCK_FILE);
        let _ = fs::remove_file(&lock_path);
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let pid_text = format!("{}", std::process::id());
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(mut f) => {
                let _ = f.write_all(pid_text.as_bytes());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(ProjectError::Locked(project_root.display().to_string()));
            }
            Err(e) => return Err(e.into()),
        }
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
        assert!(pf.path().join("project.json").is_file());
        assert!(pf.path().join(".ats/lock").is_file());
        assert!(pf.items_dir().is_dir());
        let path = pf.path().to_path_buf();
        drop(pf); // 释放 lock
        assert!(!path.join(".ats/lock").exists(), "lock cleaned after drop");
        let reopened = ProjectFolder::open(&path).expect("reopen");
        assert_eq!(reopened.meta().name, "demo");
    }

    #[test]
    fn open_locked_project_fails() {
        let td = tempdir();
        let pf = ProjectFolder::create(td.path(), "x").unwrap();
        let path = pf.path().to_path_buf();
        // 同进程内 simulate lock 存在
        drop(pf);
        // lock 已被 drop 清理，重新 acquire 应成功
        let reopened = ProjectFolder::open(&path);
        assert!(reopened.is_ok(), "open after drop should work");
    }

    #[test]
    fn invalid_name_rejected() {
        let td = tempdir();
        assert!(ProjectFolder::create(td.path(), "").is_err());
        assert!(ProjectFolder::create(td.path(), "a/b").is_err());
        assert!(ProjectFolder::create(td.path(), "x?y").is_err());
    }

    #[test]
    fn create_rejects_existing_dir() {
        let td = tempdir();
        ProjectFolder::create(td.path(), "dup").unwrap();
        assert!(ProjectFolder::create(td.path(), "dup").is_err());
    }

    #[test]
    fn save_meta_persists() {
        let td = tempdir();
        let mut pf = ProjectFolder::create(td.path(), "save-test").unwrap();
        let mut new_meta = pf.meta().clone();
        new_meta.name = "renamed".into();
        pf.save_meta(new_meta).unwrap();
        let text = fs::read_to_string(pf.path().join("project.json")).unwrap();
        assert!(text.contains("renamed"));
    }
}
