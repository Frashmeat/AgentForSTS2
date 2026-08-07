use std::fs;
use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Env, Format, Json, Serialized};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const CONFIG_PATH_ENV: &str = "SPIREFORGE_CONFIG_PATH";
const ENV_PREFIX: &str = "SPIREFORGE_";
const DEFAULT_CONFIG_PATH: &str = "runtime/agentthespire.config.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Settings {
    pub runtime: RuntimeMap,
    pub llm: LlmConfig,
    pub image_gen: ImageGenerationConfig,
    pub knowledge: TruthSourceConfig,
    pub toolchain: ToolchainConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct RuntimeMap {
    pub workstation: RuntimeConfig,
    pub web: RuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct RuntimeConfig {
    pub host: String,
    pub port: u16,
    pub allow_loopback_origins: bool,
    pub cors_origins: Vec<String>,
    pub mount_frontend: bool,
    pub requires_database: bool,
    pub github_token: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LlmConfig {
    pub mode: String,
    pub agent_backend: String,
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub base_url: String,
    pub custom_prompt: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ImageGenerationConfig {
    pub provider: String,
    pub protocol: String,
    pub model: String,
    pub api_key: String,
    pub base_url: String,
    pub size: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct TruthSourceConfig {
    pub sts2_dll_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct ToolchainConfig {
    pub godot_exe_path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct AuthConfig {
    pub session_cookie_name: String,
    pub session_secret: String,
    pub credential_secret: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            runtime: RuntimeMap::default(),
            llm: LlmConfig {
                mode: "agent_cli".into(),
                agent_backend: "claude".into(),
                ..LlmConfig::default()
            },
            image_gen: ImageGenerationConfig::default(),
            knowledge: TruthSourceConfig::default(),
            toolchain: ToolchainConfig::default(),
            auth: AuthConfig {
                session_cookie_name: "agentthespire_session".into(),
                ..AuthConfig::default()
            },
        }
    }
}

impl Default for RuntimeMap {
    fn default() -> Self {
        Self {
            workstation: RuntimeConfig {
                host: "127.0.0.1".into(),
                port: 7860,
                allow_loopback_origins: true,
                cors_origins: loopback_origins(7860),
                mount_frontend: true,
                requires_database: false,
                github_token: String::new(),
            },
            web: RuntimeConfig {
                host: "127.0.0.1".into(),
                port: 7870,
                allow_loopback_origins: false,
                cors_origins: loopback_origins(7870),
                mount_frontend: false,
                requires_database: true,
                github_token: String::new(),
            },
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 0,
            allow_loopback_origins: false,
            cors_origins: Vec::new(),
            mount_frontend: false,
            requires_database: false,
            github_token: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigStatus {
    pub path: Option<String>,
    pub file_present: bool,
    pub loaded: bool,
    pub errors: Vec<String>,
}

#[derive(Debug, Error)]
pub enum SettingsStoreError {
    #[error("settings JSON is invalid")]
    Json(#[from] serde_json::Error),
    #[error("settings filesystem operation failed")]
    Io(#[from] std::io::Error),
}

pub struct SettingsStore;

impl SettingsStore {
    #[must_use]
    pub fn load(explicit_path: Option<&Path>) -> (Settings, ConfigStatus) {
        let path = resolve_path(explicit_path);
        Self::load_path(path)
    }

    #[must_use]
    pub fn load_desktop(
        default_path: &Path,
        legacy_path: Option<&Path>,
    ) -> (Settings, ConfigStatus) {
        let configured = std::env::var_os(CONFIG_PATH_ENV).filter(|value| !value.is_empty());
        Self::load_desktop_from(default_path, legacy_path, configured.as_deref())
    }

    fn load_desktop_from(
        default_path: &Path,
        legacy_path: Option<&Path>,
        configured: Option<&std::ffi::OsStr>,
    ) -> (Settings, ConfigStatus) {
        let path = resolve_default_path(default_path, configured);
        if configured.is_some() || path.is_file() {
            return Self::load_path(path);
        }
        let Some(legacy_path) = legacy_path.filter(|candidate| candidate.is_file()) else {
            return Self::load_path(path);
        };
        let (settings, legacy_status) = Self::load_path(legacy_path.to_path_buf());
        if !legacy_status.loaded || Self::save(&path, &settings).is_err() {
            return (settings, legacy_status);
        }
        Self::load_path(path)
    }

    #[must_use]
    pub fn desktop_legacy_config_path(executable_path: &Path) -> Option<PathBuf> {
        executable_path
            .parent()
            .map(|parent| parent.join(DEFAULT_CONFIG_PATH))
    }

    fn load_path(path: PathBuf) -> (Settings, ConfigStatus) {
        let present = path.is_file();
        let mut figment = Figment::new().merge(Serialized::defaults(Settings::default()));
        if present {
            figment = figment.merge(Json::file(&path));
        }
        figment = figment.merge(Env::prefixed(ENV_PREFIX).split("__"));
        let (settings, errors) = match figment.extract::<Settings>() {
            Ok(settings) => (settings, Vec::new()),
            Err(_) => (
                Settings::default(),
                vec!["Configuration JSON is invalid.".into()],
            ),
        };
        (
            settings,
            ConfigStatus {
                path: Some(path.display().to_string()),
                file_present: present,
                loaded: present && errors.is_empty(),
                errors,
            },
        )
    }

    pub fn save(path: &Path, settings: &Settings) -> Result<(), SettingsStoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.ats-tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(settings)?)?;
        if path.exists() {
            let backup = path.with_extension("json.ats-backup");
            let _ = fs::remove_file(&backup);
            fs::rename(path, &backup)?;
            if let Err(error) = fs::rename(&temporary, path) {
                let _ = fs::rename(&backup, path);
                return Err(SettingsStoreError::Io(error));
            }
            fs::remove_file(backup)?;
        } else {
            fs::rename(temporary, path)?;
        }
        Ok(())
    }
}

fn resolve_path(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return absolute(path);
    }
    if let Some(path) = std::env::var_os(CONFIG_PATH_ENV).filter(|value| !value.is_empty()) {
        return absolute(Path::new(&path));
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for ancestor in cwd.ancestors().take(5) {
        let candidate = ancestor.join(DEFAULT_CONFIG_PATH);
        if candidate.is_file() {
            return candidate;
        }
    }
    cwd.join(DEFAULT_CONFIG_PATH)
}

fn resolve_default_path(default_path: &Path, configured: Option<&std::ffi::OsStr>) -> PathBuf {
    configured
        .map(Path::new)
        .map(absolute)
        .unwrap_or_else(|| absolute(default_path))
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn loopback_origins(port: u16) -> Vec<String> {
    vec![
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
        "http://localhost:5173".into(),
        "http://127.0.0.1:5173".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn desktop_default_path_is_stable_and_allows_an_explicit_environment_override() {
        let root = std::env::temp_dir().join("agentthespire-config-path-fixture");
        let default = root.join("app-data/config.json");
        let configured = root.join("explicit/config.json");

        assert_eq!(resolve_default_path(&default, None), default);
        assert_eq!(
            resolve_default_path(&default, Some(configured.as_os_str())),
            configured
        );
    }

    #[test]
    fn desktop_migrates_a_valid_executable_sibling_config_without_removing_it() {
        let root = tempdir().unwrap();
        let executable = root.path().join("install/agentthespire-desktop.exe");
        let legacy = SettingsStore::desktop_legacy_config_path(&executable).unwrap();
        let default = root.path().join("app-data/config.json");
        let mut settings = Settings::default();
        settings.llm.model = "fixture-model".into();
        SettingsStore::save(&legacy, &settings).unwrap();

        let (loaded, status) = SettingsStore::load_desktop_from(&default, Some(&legacy), None);

        assert_eq!(loaded.llm.model, "fixture-model");
        assert!(status.loaded);
        assert_eq!(
            status.path.as_deref(),
            Some(default.to_string_lossy().as_ref())
        );
        assert!(default.is_file());
        assert!(legacy.is_file());
    }
}
