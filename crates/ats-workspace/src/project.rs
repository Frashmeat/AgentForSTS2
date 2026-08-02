use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ats_kernel::{GamePackId, ProjectTemplateBundle};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROJECT_SCHEMA_VERSION: u32 = 2;
const MAX_RECENTS: usize = 10;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project path is invalid")]
    InvalidPath,
    #[error("project name is invalid")]
    InvalidName,
    #[error("project already exists")]
    AlreadyExists,
    #[error("project metadata is missing or invalid")]
    InvalidMetadata,
    #[error("project schema is unsupported")]
    UnsupportedSchema,
    #[error("project is already open")]
    Locked,
    #[error("project template is invalid")]
    InvalidTemplate,
    #[error("project filesystem operation failed")]
    Io(#[from] io::Error),
    #[error("project JSON is invalid")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ProjectMeta {
    pub name: String,
    pub csharp_name: String,
    pub game_id: String,
    pub scaffolded: bool,
    #[serde(default)]
    pub generated_files: Vec<String>,
    #[serde(default)]
    pub build_output_dir: Option<String>,
}

#[derive(Debug)]
pub struct ProjectFolder {
    root: PathBuf,
    meta: ProjectMeta,
    _lock: fs::File,
}

impl ProjectFolder {
    pub fn create(
        parent: &Path,
        name: &str,
        game_id: &GamePackId,
        template: &ProjectTemplateBundle,
    ) -> Result<Self, ProjectError> {
        validate_name(name)?;
        template
            .validate()
            .map_err(|_| ProjectError::InvalidTemplate)?;
        if template.game_pack_id != *game_id || !is_plain_directory(parent) {
            return Err(ProjectError::InvalidPath);
        }
        let root = parent.join(name);
        if root.exists() {
            return Err(ProjectError::AlreadyExists);
        }
        fs::create_dir(&root)?;
        let result = (|| {
            for relative in ["items", "artifacts", ".ats", ".ats/runs-v3"] {
                fs::create_dir_all(root.join(relative))?;
            }
            let csharp_name = derive_project_identifier(name);
            write_template(&root, &csharp_name, template)?;
            let meta = ProjectMeta {
                name: name.to_owned(),
                csharp_name,
                game_id: game_id.as_str().to_owned(),
                scaffolded: true,
                generated_files: Vec::new(),
                build_output_dir: None,
            };
            write_json_atomic(&root.join("project.json"), &meta)?;
            write_atomic(
                &root.join(".ats/version"),
                PROJECT_SCHEMA_VERSION.to_string().as_bytes(),
            )?;
            let lock = acquire_lock(&root)?;
            Ok(Self {
                root: root.clone(),
                meta,
                _lock: lock,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&root);
        }
        result
    }

    pub fn open(root: &Path) -> Result<Self, ProjectError> {
        if !is_plain_directory(root) {
            return Err(ProjectError::InvalidPath);
        }
        let meta: ProjectMeta = serde_json::from_slice(&fs::read(root.join("project.json"))?)?;
        GamePackId::parse(meta.game_id.clone()).map_err(|_| ProjectError::InvalidMetadata)?;
        validate_name(&meta.name)?;
        let version = fs::read_to_string(root.join(".ats/version"))?;
        if version.trim() != PROJECT_SCHEMA_VERSION.to_string() {
            return Err(ProjectError::UnsupportedSchema);
        }
        fs::create_dir_all(root.join(".ats/runs-v3"))?;
        let lock = acquire_lock(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            meta,
            _lock: lock,
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn meta(&self) -> &ProjectMeta {
        &self.meta
    }

    #[must_use]
    pub fn run_history_dir(&self) -> PathBuf {
        self.root.join(".ats/runs-v3")
    }

    #[must_use]
    pub fn artifacts_dir(&self) -> PathBuf {
        self.root.join("artifacts")
    }
}

#[derive(Debug, Clone)]
pub struct AppDataPaths {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub recent_projects_path: PathBuf,
    pub logs_root: PathBuf,
}

impl AppDataPaths {
    #[must_use]
    pub fn resolve() -> Self {
        let root = dirs::data_dir()
            .or_else(dirs::home_dir)
            .map(|path| path.join("AgentTheSpire"))
            .unwrap_or_else(|| PathBuf::from("./AgentTheSpire"));
        Self::from_root(root)
    }

    #[must_use]
    pub fn from_root(root: PathBuf) -> Self {
        Self {
            config_path: root.join("config.json"),
            recent_projects_path: root.join("recent_projects.json"),
            logs_root: root.join("logs"),
            root,
        }
    }

    pub fn ensure_dirs(&self) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        fs::create_dir_all(&self.logs_root)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct RecentEntry {
    pub path: String,
    pub name: String,
    pub last_opened_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct RecentProjects {
    pub items: Vec<RecentEntry>,
}

impl RecentProjects {
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(bytes) = fs::read(path) else {
            return Self::default();
        };
        serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            let _ = fs::rename(path, path.with_extension("json.corrupt"));
            Self::default()
        })
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_json_atomic(path, self)
    }

    pub fn record(&mut self, root: &Path, meta: &ProjectMeta) {
        let value = root.display().to_string();
        self.items.retain(|item| item.path != value);
        self.items.insert(
            0,
            RecentEntry {
                path: value,
                name: meta.name.clone(),
                last_opened_at: Utc::now(),
            },
        );
        self.items.truncate(MAX_RECENTS);
    }

    pub fn forget(&mut self, root: &Path) {
        let value = root.display().to_string();
        self.items.retain(|item| item.path != value);
    }
}

#[must_use]
pub fn derive_project_identifier(value: &str) -> String {
    let value: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let value = value.trim_matches('_');
    if value.is_empty() {
        "MyMod".into()
    } else if value.as_bytes()[0].is_ascii_digit() {
        format!("Mod{value}")
    } else {
        value.into()
    }
}

fn write_template(
    root: &Path,
    identifier: &str,
    template: &ProjectTemplateBundle,
) -> Result<(), ProjectError> {
    for file in &template.files {
        let relative = file
            .relative_path
            .replace(&template.placeholder, identifier);
        let target = root.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = std::str::from_utf8(&file.bytes).map_or_else(
            |_| file.bytes.clone(),
            |text| text.replace(&template.placeholder, identifier).into_bytes(),
        );
        fs::write(target, bytes)?;
    }
    Ok(())
}

fn acquire_lock(root: &Path) -> Result<fs::File, ProjectError> {
    let path = root.join(".ats/lock");
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => return Err(ProjectError::Locked),
        Err(fs::TryLockError::Error(error)) => return Err(ProjectError::Io(error)),
    }
    file.set_len(0)?;
    file.write_all(std::process::id().to_string().as_bytes())?;
    file.sync_all()?;
    Ok(file)
}

fn write_json_atomic<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), ProjectError> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let temporary = path.with_extension("ats-tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, path)?;
    Ok(())
}

fn is_plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn validate_name(name: &str) -> Result<(), ProjectError> {
    if name.trim().is_empty()
        || name.len() > 128
        || name.chars().any(|character| {
            matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
    {
        Err(ProjectError::InvalidName)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ats_kernel::{ProjectTemplateFile, Sha256Digest};
    use sha2::{Digest, Sha256};

    use super::*;

    fn template() -> ProjectTemplateBundle {
        let bytes = b"namespace ModTemplate;".to_vec();
        ProjectTemplateBundle {
            game_pack_id: GamePackId::parse("fixture").unwrap(),
            template_id: "fixture.default".into(),
            placeholder: "ModTemplate".into(),
            files: vec![ProjectTemplateFile {
                relative_path: "ModTemplate.cs".into(),
                sha256: Sha256Digest::parse(format!("{:x}", Sha256::digest(&bytes))).unwrap(),
                bytes,
            }],
        }
    }

    #[test]
    fn create_open_and_relock_preserve_existing_project_contract() {
        let temp = tempfile::TempDir::new().unwrap();
        let game = GamePackId::parse("fixture").unwrap();
        let project = ProjectFolder::create(temp.path(), "Sample", &game, &template()).unwrap();
        let root = project.path().to_path_buf();
        assert_eq!(
            fs::read_to_string(root.join("Sample.cs")).unwrap(),
            "namespace Sample;"
        );
        assert!(matches!(
            ProjectFolder::open(&root),
            Err(ProjectError::Locked)
        ));
        drop(project);
        assert!(ProjectFolder::open(&root).is_ok());
        assert!(root.join(".ats/runs-v3").is_dir());
    }
}
