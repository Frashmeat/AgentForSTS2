//! Codegen 请求载体。镜像 Python `codegen/domain/models.py`。
//!
//! 设计取向：尽量贴近 Python 字段名（snake_case），便于前端复用原 schema。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::planning::PlanItem;

/// 把资产名转换成本地化 key 使用的 ASCII `UPPER_SNAKE_CASE` 片段。
/// Prompt 契约与落盘校验必须共用这一实现，避免模型看到的 key 与后端验收 key 漂移。
#[must_use]
pub(crate) fn asset_localization_key_segment(raw: &str) -> String {
    let mut out = String::new();
    let mut previous_is_lower_or_digit = false;
    let chars: Vec<char> = raw.chars().collect();
    for (index, ch) in chars.iter().copied().enumerate() {
        if ch.is_ascii_alphanumeric() {
            let acronym_boundary = ch.is_ascii_uppercase()
                && index > 0
                && chars[index - 1].is_ascii_uppercase()
                && chars.get(index + 1).is_some_and(char::is_ascii_lowercase);
            if ch.is_ascii_uppercase()
                && (previous_is_lower_or_digit || acronym_boundary)
                && !out.ends_with('_')
            {
                out.push('_');
            }
            out.push(ch.to_ascii_uppercase());
            previous_is_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
            previous_is_lower_or_digit = false;
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod asset_kind_tests {
    use super::asset_localization_key_segment;

    #[test]
    fn localization_key_segment_is_shared_and_stable() {
        assert_eq!(
            asset_localization_key_segment("EnergySeed Relic"),
            "ENERGY_SEED_RELIC"
        );
        assert_eq!(
            asset_localization_key_segment("E2EEnergySeedRelic"),
            "E2_E_ENERGY_SEED_RELIC"
        );
        assert_eq!(asset_localization_key_segment("XMLParser"), "XML_PARSER");
        assert_eq!(
            asset_localization_key_segment(" energy--seed "),
            "ENERGY_SEED"
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct AssetCodegenRequest {
    pub design_description: String,
    pub asset_type: String,
    pub asset_name: String,
    pub image_paths: Vec<PathBuf>,
    pub project_root: PathBuf,
    pub name_zhs: String,
    /// 控制后续正式 publish 编排；结构化资产生成始终执行隔离的 compile gate。
    pub skip_build: bool,
}

impl Default for AssetCodegenRequest {
    fn default() -> Self {
        Self {
            design_description: String::new(),
            asset_type: String::new(),
            asset_name: String::new(),
            image_paths: Vec::new(),
            project_root: PathBuf::new(),
            name_zhs: String::new(),
            skip_build: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct CustomCodegenRequest {
    pub description: String,
    pub implementation_notes: String,
    pub name: String,
    pub project_root: PathBuf,
    pub skip_build: bool,
}

impl Default for CustomCodegenRequest {
    fn default() -> Self {
        Self {
            description: String::new(),
            implementation_notes: String::new(),
            name: String::new(),
            project_root: PathBuf::new(),
            skip_build: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct AssetGroupItem {
    pub item: PlanItem,
    pub image_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct AssetGroupRequest {
    pub assets: Vec<AssetGroupItem>,
    pub project_root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "snake_case")]
pub struct ModProjectRequest {
    pub project_name: String,
    pub target_dir: PathBuf,
}
