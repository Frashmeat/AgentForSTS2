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
