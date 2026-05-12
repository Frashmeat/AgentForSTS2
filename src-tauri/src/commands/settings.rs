//! Settings 命令：让前端能"看一眼"当前生效配置 + 打开 OS 编辑器手动编辑文件。
//!
//! **不暴露 api_key 等敏感字段** —— 用 `masked_secret` 把 32 字符以上压成
//! `<prefix>...<suffix>` + 长度提示，让用户能确认填了，但不会被截图泄漏。
//!
//! 编辑用 `tauri_plugin_shell::ShellExt::shell().open`，跨平台用 OS 默认编辑器。
//! 改完后用户需要重启 app 让 figment 重新加载（Stage 5 之后再做 hot-reload）。

use std::path::PathBuf;

use ats_core::config::Settings;
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

use crate::AppConfig;

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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSnapshot {
    pub provider: String,
    pub model: String,
    pub base_url: String,
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
    pub api_key_masked: String,
    pub api_key_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshot {
    pub host: String,
    pub port: u16,
    pub mount_frontend: bool,
    pub requires_database: bool,
}

#[tauri::command]
pub fn get_settings_snapshot(
    config: tauri::State<'_, AppConfig>,
) -> SettingsSnapshot {
    let s: &Settings = &config.settings;
    SettingsSnapshot {
        config_path: config.status.path.clone(),
        config_loaded: config.status.loaded,
        config_errors: config.status.errors.clone(),
        llm: LlmSnapshot {
            provider: s.llm.provider.clone(),
            model: s.llm.model.clone(),
            base_url: s.llm.base_url.clone(),
            api_key_masked: masked_secret(&s.llm.api_key),
            api_key_configured: !s.llm.api_key.is_empty(),
        },
        image_gen: ImageGenSnapshot {
            provider: s.image_gen.provider.clone(),
            model: s.image_gen.model.clone(),
            base_url: s.image_gen.base_url.clone(),
            size: s.image_gen.size.clone(),
            api_key_masked: masked_secret(&s.image_gen.api_key),
            api_key_configured: !s.image_gen.api_key.is_empty(),
        },
        runtime_workstation: snapshot_runtime(&s.runtime.workstation),
        runtime_web: snapshot_runtime(&s.runtime.web),
    }
}

#[tauri::command]
pub async fn open_config_in_editor(
    app: AppHandle,
    config: tauri::State<'_, AppConfig>,
) -> Result<String, String> {
    let path = config
        .status
        .path
        .clone()
        .ok_or_else(|| "no config path resolved — load 失败时不能打开".to_string())?;
    let resolved = PathBuf::from(&path);
    if !resolved.is_file() {
        return Err(format!("config file not found: {path}"));
    }
    // shell.open 在 tauri 2.x 标记为 deprecated（建议用 tauri-plugin-opener）。
    // 现在还能用，等真切换 opener plugin 时一起改；CI 用 -D warnings 这里 allow
    // 一下避免无意义阻塞。
    #[allow(deprecated)]
    app.shell()
        .open(path.clone(), None)
        .map_err(|e| format!("shell::open: {e}"))?;
    Ok(path)
}

fn snapshot_runtime(r: &ats_core::config::RuntimeConfig) -> RuntimeSnapshot {
    RuntimeSnapshot {
        host: r.host.clone(),
        port: r.port,
        mount_frontend: r.mount_frontend,
        requires_database: r.requires_database,
    }
}

/// 把敏感字符串脱敏：
/// - 长度 0 → `<empty>`
/// - 长度 ≤ 8 → 全屏蔽 `****` + 显长度
/// - 长度 > 8 → 前 4 字符 + `...` + 后 4 字符 + 长度
fn masked_secret(raw: &str) -> String {
    if raw.is_empty() {
        return "<empty>".into();
    }
    let chars: Vec<char> = raw.chars().collect();
    let n = chars.len();
    if n <= 8 {
        return format!("**** ({n} chars)");
    }
    let prefix: String = chars.iter().take(4).collect();
    let suffix: String = chars.iter().skip(n - 4).collect();
    format!("{prefix}...{suffix} ({n} chars)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_secret_empty() {
        assert_eq!(masked_secret(""), "<empty>");
    }

    #[test]
    fn masked_secret_short() {
        assert_eq!(masked_secret("abcd"), "**** (4 chars)");
        assert_eq!(masked_secret("12345678"), "**** (8 chars)");
    }

    #[test]
    fn masked_secret_long_keeps_head_and_tail() {
        let masked = masked_secret("sk-abcdefghijklmnopqrstuvwxyz1234567890");
        assert!(masked.starts_with("sk-a..."));
        assert!(masked.contains("7890"));
        assert!(masked.contains("(39 chars)"));
    }

    #[test]
    fn masked_secret_unicode_safe() {
        // 中文字符按 Unicode scalar 计数，不应被 byte-slice 撕裂
        let masked = masked_secret("apikey-中文测试-987654321XYZ");
        assert!(masked.contains("("));
        // 不 panic 就是过
    }
}
