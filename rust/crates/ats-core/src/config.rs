//! 配置加载。
//!
//! 加载顺序（后者覆盖前者）：
//!   1. 内置默认值（`Settings::built_in_defaults`）
//!   2. JSON 文件——路径由 `--config` 参数 / `SPIREFORGE_CONFIG_PATH` 环境变量 / `runtime/agentthespire.config.json` 三级回退决定
//!   3. 环境变量（`SPIREFORGE_<SECTION>__<FIELD>` 格式，`__` 作子键分隔符）
//!
//! 字段命名采用 snake_case，与现 Python `agentthespire.config.example.json` 一致；
//! 见 `runtime/agentthespire.config.example.json` 模板。

use std::path::{Path, PathBuf};

use figment::{
    Figment,
    providers::{Env, Format, Json, Serialized},
};
use serde::{Deserialize, Serialize};

use crate::health::Role;

const DEFAULT_CONFIG_PATH: &str = "runtime/agentthespire.config.json";
const CONFIG_PATH_ENV: &str = "SPIREFORGE_CONFIG_PATH";
const ENV_PREFIX: &str = "SPIREFORGE_";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct Settings {
    pub runtime: RuntimeMap,
    pub llm: LlmConfig,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct LlmConfig {
    pub mode: String,
    pub agent_backend: String,
    pub provider: String,
    pub model: String,
    /// API key — never expose this in serialized output meant for the frontend.
    pub api_key: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct AuthConfig {
    pub session_cookie_name: String,
    pub session_secret: String,
    pub credential_secret: String,
}

impl Settings {
    /// Built-in defaults mirroring the Python `DEFAULT_CONFIG` baseline.
    #[must_use]
    pub fn built_in_defaults() -> Self {
        Self {
            runtime: RuntimeMap {
                workstation: RuntimeConfig {
                    host: "127.0.0.1".into(),
                    port: 7860,
                    allow_loopback_origins: true,
                    cors_origins: loopback_cors_defaults(7860),
                    mount_frontend: true,
                    requires_database: false,
                },
                web: RuntimeConfig {
                    host: "127.0.0.1".into(),
                    port: 7870,
                    allow_loopback_origins: false,
                    cors_origins: loopback_cors_defaults(7870),
                    mount_frontend: false,
                    requires_database: true,
                },
            },
            llm: LlmConfig {
                mode: "agent_cli".into(),
                agent_backend: "claude".into(),
                provider: String::new(),
                model: String::new(),
                api_key: String::new(),
                base_url: String::new(),
            },
            auth: AuthConfig {
                session_cookie_name: "agentthespire_session".into(),
                session_secret: String::new(),
                credential_secret: String::new(),
            },
        }
    }

    /// Validate that the loaded config satisfies the requirements of the given role.
    /// Returns a list of human-readable error strings (empty = OK).
    #[must_use]
    pub fn validate_for_role(&self, role: Role) -> Vec<String> {
        let mut errors = Vec::new();
        if matches!(role, Role::Web) {
            if self.auth.session_secret.is_empty() {
                errors.push("web role requires non-empty auth.session_secret".into());
            }
            if self.auth.credential_secret.is_empty() {
                errors.push("web role requires non-empty auth.credential_secret".into());
            }
        }
        errors
    }

    /// Load settings + report status.
    ///
    /// `explicit_path` overrides the env/default lookup when `Some`.
    /// The returned `ConfigStatus` is safe to expose to the frontend
    /// (no secrets, just metadata about the load).
    #[must_use]
    pub fn load(explicit_path: Option<&Path>) -> (Self, ConfigStatus) {
        let path = explicit_path
            .map(PathBuf::from)
            .or_else(|| std::env::var(CONFIG_PATH_ENV).ok().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));
        let absolute = std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or_else(|_| path.clone());
        let file_present = absolute.exists();

        let mut figment = Figment::new().merge(Serialized::defaults(Self::built_in_defaults()));
        if file_present {
            figment = figment.merge(Json::file(&absolute));
        }
        figment = figment.merge(Env::prefixed(ENV_PREFIX).split("__"));

        let mut errors = Vec::new();
        let settings = match figment.extract::<Settings>() {
            Ok(s) => s,
            Err(err) => {
                errors.push(err.to_string());
                Self::built_in_defaults()
            }
        };

        let status = ConfigStatus {
            path: Some(absolute.display().to_string()),
            file_present,
            loaded: file_present && errors.is_empty(),
            errors,
        };

        (settings, status)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self::built_in_defaults()
    }
}

// Implement Default for sub-structs so figment's `default` attribute can use
// them when extracting partial JSON files.

impl Default for RuntimeMap {
    fn default() -> Self {
        Settings::built_in_defaults().runtime
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
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Settings::built_in_defaults().llm
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Settings::built_in_defaults().auth
    }
}

fn loopback_cors_defaults(port: u16) -> Vec<String> {
    vec![
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
        "http://localhost:5173".into(),
        "http://127.0.0.1:5173".into(),
        "http://localhost:8080".into(),
        "http://127.0.0.1:8080".into(),
    ]
}

/// Result of attempting to load the config file. Safe to serialize to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigStatus {
    /// Absolute path the loader attempted (None only if cwd lookup failed).
    pub path: Option<String>,
    /// Whether the file existed on disk.
    pub file_present: bool,
    /// True iff file was present AND parsed without errors AND role validation passed.
    pub loaded: bool,
    /// Human-readable error strings (parse + role-validation).
    pub errors: Vec<String>,
}
