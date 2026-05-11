//! STS2 提示词材料聚合——把内嵌的 sts2 模板按 asset_type 拼接成完整 guidance 文本。
//!
//! 镜像 Python `backend/agents/sts2_guidance.py`。Python 端读磁盘，本端用 include_str!
//! 内嵌的模板（`ats-core::knowledge::templates`）。

use crate::knowledge::templates::get_template;

const SEP: &str = "\n";

/// 按 asset_type 装载 common.md + 类型专属模板，组合成 prompt 可直接消费的字符串。
///
/// 返回值始终包含 `common.md`；未识别 asset_type 时只返回 common。
#[must_use]
pub fn guidance_for_asset_type(asset_type: &str) -> String {
    let common = template_or_empty("common");
    let extra = match asset_type {
        "card" | "card_fullscreen" => template_or_empty("card"),
        "relic" => template_or_empty("relic"),
        "power" => template_or_empty("power"),
        "potion" => template_or_empty("potion"),
        "character" => template_or_empty("character"),
        "custom_code" => {
            // custom_code 历史上需要把 potion + character 也带上，因为自定义代码常常
            // 触及这两类资源的运行时模型。
            let mut buf = template_or_empty("custom_code");
            buf.push_str(SEP);
            buf.push_str(&template_or_empty("potion"));
            buf.push_str(SEP);
            buf.push_str(&template_or_empty("character"));
            buf
        }
        _ => String::new(),
    };
    if extra.is_empty() {
        common
    } else {
        format!("{common}{extra}")
    }
}

/// Planner 阶段使用的 guidance（只包含 planner_guidance.md）。
#[must_use]
pub fn planner_guidance() -> String {
    template_or_empty("planner_guidance")
}

/// 把内置模板加 `\n` 结尾返回（与 Python `_load_resource_text` 行为一致）。
fn template_or_empty(slot: &str) -> String {
    match get_template(slot) {
        Some(content) => {
            let trimmed = content.trim_end();
            format!("{trimmed}\n")
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_guidance_includes_common_and_card() {
        let g = guidance_for_asset_type("card");
        assert!(g.contains("common"), "expected common.md content");
        // card.md 不一定包含字符串 "card"，但应该是 common.md 之后还有内容
        let common_only = template_or_empty("common");
        assert!(g.len() > common_only.len());
    }

    #[test]
    fn unknown_asset_type_returns_common_only() {
        let g = guidance_for_asset_type("unknown");
        let common_only = template_or_empty("common");
        assert_eq!(g, common_only);
    }

    #[test]
    fn custom_code_includes_potion_and_character() {
        let g = guidance_for_asset_type("custom_code");
        let common_only = template_or_empty("common");
        let custom_only = template_or_empty("custom_code");
        // 应当 = common + custom_code + potion + character，比单独的 custom_code 大很多
        assert!(g.len() > common_only.len() + custom_only.len());
    }

    #[test]
    fn planner_guidance_non_empty() {
        let g = planner_guidance();
        assert!(!g.is_empty());
    }
}
