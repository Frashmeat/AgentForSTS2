use std::path::PathBuf;
use std::sync::Arc;

use ats_adapters::{RuntimeConfig, Settings, SettingsStore};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_shell::ShellExt;

use crate::AppConfig;
use crate::commands::failure::{CommandFailure, CommandResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub config_path: Option<String>,
    pub config_loaded: bool,
    pub config_errors: Vec<String>,
    pub llm: LlmSnapshot,
    pub image_gen: ImageGenSnapshot,
    pub runtime_workstation: RuntimeSnapshot,
    pub runtime_web: RuntimeSnapshot,
    pub knowledge: KnowledgeSnapshot,
    pub toolchain: ToolchainSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSnapshot {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub custom_prompt: String,
    pub max_output_tokens: Option<u32>,
    pub api_key_masked: String,
    pub api_key_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageGenSnapshot {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub size: String,
    pub protocol: String,
    pub api_key_masked: String,
    pub api_key_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSnapshot {
    pub sts2_dll_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainSnapshot {
    pub godot_exe_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub host: String,
    pub port: u16,
    pub mount_frontend: bool,
    pub requires_database: bool,
    pub github_token_masked: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub llm: Option<LlmPatch>,
    pub image_gen: Option<ImageGenPatch>,
    pub runtime_workstation: Option<RuntimePatch>,
    pub knowledge: Option<KnowledgePatch>,
    pub toolchain: Option<ToolchainPatch>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct LlmPatch {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub custom_prompt: Option<String>,
    pub max_output_tokens: Option<Option<u32>>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageGenPatch {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub size: Option<String>,
    pub protocol: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePatch {
    pub github_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgePatch {
    pub sts2_dll_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolchainPatch {
    pub godot_exe_path: Option<String>,
}

#[tauri::command]
pub fn get_settings_snapshot(config: State<'_, Arc<AppConfig>>) -> SettingsSnapshot {
    snapshot(&config)
}

#[tauri::command]
pub fn save_settings_patch(
    config: State<'_, Arc<AppConfig>>,
    patch: SettingsPatch,
) -> CommandResult<SettingsSnapshot> {
    let settings = merge_settings_patch(config.settings_snapshot(), patch)?;
    let path = config
        .status_snapshot()
        .path
        .map(PathBuf::from)
        .ok_or_else(|| CommandFailure::storage("settings.path"))?;
    SettingsStore::save(&path, &settings).map_err(|_| CommandFailure::storage("settings.save"))?;
    config.replace_settings(settings);
    Ok(snapshot(&config))
}

#[tauri::command]
pub async fn open_config_in_editor(
    app: AppHandle,
    config: State<'_, Arc<AppConfig>>,
) -> CommandResult<String> {
    let path = config
        .status_snapshot()
        .path
        .ok_or_else(|| CommandFailure::storage("settings.open"))?;
    if !PathBuf::from(&path).is_file() {
        return Err(CommandFailure::storage("settings.open"));
    }
    #[allow(deprecated)]
    app.shell()
        .open(path.clone(), None)
        .map_err(|_| CommandFailure::unclassified("settings.shell_open"))?;
    Ok(path)
}

fn merge_settings_patch(mut settings: Settings, patch: SettingsPatch) -> CommandResult<Settings> {
    if let Some(value) = patch.llm {
        replace(&mut settings.llm.provider, value.provider);
        replace(&mut settings.llm.model, value.model);
        replace(&mut settings.llm.base_url, value.base_url);
        replace(&mut settings.llm.custom_prompt, value.custom_prompt);
        if let Some(max_output_tokens) = value.max_output_tokens {
            settings.llm.max_output_tokens = max_output_tokens;
        }
        replace(&mut settings.llm.api_key, value.api_key);
    }
    if let Some(value) = patch.image_gen {
        replace(&mut settings.image_gen.provider, value.provider);
        replace(&mut settings.image_gen.model, value.model);
        replace(&mut settings.image_gen.base_url, value.base_url);
        replace(&mut settings.image_gen.size, value.size);
        replace(&mut settings.image_gen.protocol, value.protocol);
        replace(&mut settings.image_gen.api_key, value.api_key);
    }
    if let Some(value) = patch.runtime_workstation {
        replace(
            &mut settings.runtime.workstation.github_token,
            value.github_token,
        );
    }
    if let Some(value) = patch.knowledge {
        replace(&mut settings.knowledge.sts2_dll_path, value.sts2_dll_path);
    }
    if let Some(value) = patch.toolchain {
        replace(&mut settings.toolchain.godot_exe_path, value.godot_exe_path);
    }
    if settings.llm.custom_prompt.chars().count() > 4_000
        || settings.llm.custom_prompt.contains('\0')
    {
        return Err(CommandFailure::invalid_input("settings.custom_prompt"));
    }
    settings
        .llm
        .model_request_limits()
        .map_err(|_| CommandFailure::model_configuration("settings.max_output_tokens"))?;
    Ok(settings)
}

fn replace(target: &mut String, value: Option<String>) {
    if let Some(value) = value {
        *target = value;
    }
}

fn snapshot(config: &AppConfig) -> SettingsSnapshot {
    let (settings, status) = config.snapshot();
    SettingsSnapshot {
        config_path: status.path,
        config_loaded: status.loaded,
        config_errors: status.errors,
        llm: LlmSnapshot {
            provider: settings.llm.provider,
            model: settings.llm.model,
            base_url: settings.llm.base_url,
            custom_prompt: settings.llm.custom_prompt,
            max_output_tokens: settings.llm.max_output_tokens,
            api_key_masked: masked_secret(&settings.llm.api_key),
            api_key_configured: !settings.llm.api_key.is_empty(),
        },
        image_gen: ImageGenSnapshot {
            provider: settings.image_gen.provider,
            model: settings.image_gen.model,
            base_url: settings.image_gen.base_url,
            size: settings.image_gen.size,
            protocol: settings.image_gen.protocol,
            api_key_masked: masked_secret(&settings.image_gen.api_key),
            api_key_configured: !settings.image_gen.api_key.is_empty(),
        },
        runtime_workstation: snapshot_runtime(&settings.runtime.workstation),
        runtime_web: snapshot_runtime(&settings.runtime.web),
        knowledge: KnowledgeSnapshot {
            sts2_dll_path: settings.knowledge.sts2_dll_path,
        },
        toolchain: ToolchainSnapshot {
            godot_exe_path: settings.toolchain.godot_exe_path,
        },
    }
}

fn snapshot_runtime(value: &RuntimeConfig) -> RuntimeSnapshot {
    RuntimeSnapshot {
        host: value.host.clone(),
        port: value.port,
        mount_frontend: value.mount_frontend,
        requires_database: value.requires_database,
        github_token_masked: masked_secret(&value.github_token),
    }
}

fn masked_secret(raw: &str) -> String {
    let characters = raw.chars().collect::<Vec<_>>();
    match characters.len() {
        0 => "<empty>".into(),
        length @ 1..=8 => format!("**** ({length} chars)"),
        length => format!(
            "{}...{} ({length} chars)",
            characters.iter().take(4).collect::<String>(),
            characters.iter().skip(length - 4).collect::<String>()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_patch_preserves_updates_and_clears_explicitly() {
        let mut settings = Settings::default();
        settings.llm.api_key = "secret".into();
        let preserved = merge_settings_patch(settings.clone(), SettingsPatch::default()).unwrap();
        assert_eq!(preserved.llm.api_key, "secret");
        let cleared = merge_settings_patch(
            settings,
            SettingsPatch {
                llm: Some(LlmPatch {
                    api_key: Some(String::new()),
                    ..LlmPatch::default()
                }),
                ..SettingsPatch::default()
            },
        )
        .unwrap();
        assert!(cleared.llm.api_key.is_empty());
    }

    #[test]
    fn model_output_budget_patch_preserves_sets_clears_and_rejects_invalid_values() {
        let mut settings = Settings::default();
        settings.llm.max_output_tokens = Some(8_192);
        let preserved = merge_settings_patch(settings.clone(), SettingsPatch::default()).unwrap();
        assert_eq!(preserved.llm.max_output_tokens, Some(8_192));

        let set = merge_settings_patch(
            settings.clone(),
            SettingsPatch {
                llm: Some(LlmPatch {
                    max_output_tokens: Some(Some(4_096)),
                    ..LlmPatch::default()
                }),
                ..SettingsPatch::default()
            },
        )
        .unwrap();
        assert_eq!(set.llm.max_output_tokens, Some(4_096));

        let cleared = merge_settings_patch(
            settings.clone(),
            SettingsPatch {
                llm: Some(LlmPatch {
                    max_output_tokens: Some(None),
                    ..LlmPatch::default()
                }),
                ..SettingsPatch::default()
            },
        )
        .unwrap();
        assert_eq!(cleared.llm.max_output_tokens, None);

        let invalid = merge_settings_patch(
            settings,
            SettingsPatch {
                llm: Some(LlmPatch {
                    max_output_tokens: Some(Some(0)),
                    ..LlmPatch::default()
                }),
                ..SettingsPatch::default()
            },
        )
        .unwrap_err();
        assert_eq!(invalid.0.code.as_str(), "model.configuration");
    }
}
