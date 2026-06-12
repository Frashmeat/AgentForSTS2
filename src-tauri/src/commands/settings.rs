//! Settings 命令：让前端能"看一眼"当前生效配置 + 打开 OS 编辑器手动编辑文件。
//!
//! **不暴露 api_key 等敏感字段** —— 用 `masked_secret` 把 32 字符以上压成
//! `<prefix>...<suffix>` + 长度提示，让用户能确认填了，但不会被截图泄漏。
//!
//! 编辑用 `tauri_plugin_shell::ShellExt::shell().open`，跨平台用 OS 默认编辑器。
//! 改完后用户需要重启 app 让 figment 重新加载（Stage 5 之后再做 hot-reload）。

use std::path::PathBuf;

use ats_core::config::Settings;
use serde::{Deserialize, Serialize};
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
    pub knowledge: KnowledgeSnapshot,
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
pub struct RuntimeSnapshot {
    pub host: String,
    pub port: u16,
    pub mount_frontend: bool,
    pub requires_database: bool,
    #[serde(rename = "githubToken")]
    pub github_token: String,
}

#[tauri::command]
pub fn get_settings_snapshot(config: tauri::State<'_, AppConfig>) -> SettingsSnapshot {
    let (s, status) = config.snapshot();
    SettingsSnapshot {
        config_path: status.path.clone(),
        config_loaded: status.loaded,
        config_errors: status.errors.clone(),
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
            protocol: s.image_gen.protocol.clone(),
            api_key_masked: masked_secret(&s.image_gen.api_key),
            api_key_configured: !s.image_gen.api_key.is_empty(),
        },
        runtime_workstation: snapshot_runtime(&s.runtime.workstation),
        runtime_web: snapshot_runtime(&s.runtime.web),
        knowledge: KnowledgeSnapshot {
            sts2_dll_path: s.knowledge.sts2_dll_path.clone(),
        },
    }
}

/// 表单提交的部分配置补丁：每个字段 None = 不改。
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct SettingsPatch {
    pub llm: Option<LlmPatch>,
    pub image_gen: Option<ImageGenPatch>,
    pub runtime_workstation: Option<RuntimePatch>,
    pub knowledge: Option<KnowledgePatch>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct LlmPatch {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// 用户不填 = 不改；空字符串 = 清空。前端用 `null` 表示"不改"。
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct ImageGenPatch {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub size: Option<String>,
    pub protocol: Option<String>,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct RuntimePatch {
    pub github_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct KnowledgePatch {
    pub sts2_dll_path: Option<String>,
}
/// 把 patch 合并进当前内存里的 settings + 写回 config.json，然后热替换。
///
/// 设计：
/// - 文件写入用 tempfile + rename 保原子
/// - 写盘失败 → 不动内存（让用户重试），返回 Err
/// - 写盘成功 → AppConfig::replace_settings 让下一次 build_client 拿到新值
#[tauri::command]
pub fn save_settings_patch(
    config: tauri::State<'_, AppConfig>,
    patch: SettingsPatch,
) -> Result<SettingsSnapshot, String> {
    let mut new_settings = config.settings_snapshot();
    if let Some(p) = patch.llm {
        if let Some(v) = p.provider {
            new_settings.llm.provider = v;
        }
        if let Some(v) = p.model {
            new_settings.llm.model = v;
        }
        if let Some(v) = p.base_url {
            new_settings.llm.base_url = v;
        }
        if let Some(v) = p.api_key {
            new_settings.llm.api_key = v;
        }
    }
    if let Some(p) = patch.image_gen {
        if let Some(v) = p.provider {
            new_settings.image_gen.provider = v;
        }
        if let Some(v) = p.model {
            new_settings.image_gen.model = v;
        }
        if let Some(v) = p.base_url {
            new_settings.image_gen.base_url = v;
        }
        if let Some(v) = p.size {
            new_settings.image_gen.size = v;
        }
        if let Some(v) = p.protocol {
            new_settings.image_gen.protocol = v;
        }
        if let Some(v) = p.api_key {
            new_settings.image_gen.api_key = v;
        }
    }
    if let Some(p) = patch.runtime_workstation
        && let Some(v) = p.github_token
    {
        new_settings.runtime.workstation.github_token = v;
    }
    if let Some(p) = patch.knowledge
        && let Some(v) = p.sts2_dll_path
    {
        new_settings.knowledge.sts2_dll_path = v;
    }

    let status = config.status_snapshot();
    let path = status
        .path
        .clone()
        .ok_or_else(|| "no config path resolved — can't save".to_string())?;
    write_settings_atomic(&PathBuf::from(&path), &new_settings)
        .map_err(|e| format!("write config: {e}"))?;
    config.replace_settings(new_settings);
    Ok(get_settings_snapshot(config))
}

fn write_settings_atomic(path: &std::path::Path, settings: &Settings) -> std::io::Result<()> {
    let body = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("json.tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&body)?;
        f.sync_all().ok();
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[tauri::command]
pub async fn open_config_in_editor(
    app: AppHandle,
    config: tauri::State<'_, AppConfig>,
) -> Result<String, String> {
    let path = config
        .status_snapshot()
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
        github_token: masked_secret(&r.github_token),
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


#[tauri::command]
pub fn discover_sts2_dll() -> Result<Option<String>, String> {
    Ok(ats_core::platform::discovery::discover_sts2_dll())
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
