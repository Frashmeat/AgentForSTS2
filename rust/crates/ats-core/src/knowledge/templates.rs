//! STS2 提示词模板内嵌。
//!
//! 与现 Python `backend/app/modules/knowledge/templates/sts2/*.md` 同源同步；
//! 用 include_str! 打进二进制，避免运行时依赖 backend/ 目录的相对路径。
//! 增减模板时同步改 `TEMPLATE_SLOTS` 与 match 分支。

const COMMON_MD: &str = include_str!("../../templates/sts2/common.md");
const CARD_MD: &str = include_str!("../../templates/sts2/card.md");
const POWER_MD: &str = include_str!("../../templates/sts2/power.md");
const RELIC_MD: &str = include_str!("../../templates/sts2/relic.md");
const CUSTOM_CODE_MD: &str = include_str!("../../templates/sts2/custom_code.md");
const CHARACTER_MD: &str = include_str!("../../templates/sts2/character.md");
const POTION_MD: &str = include_str!("../../templates/sts2/potion.md");
const PLANNER_GUIDANCE_MD: &str = include_str!("../../templates/sts2/planner_guidance.md");

/// 所有可用 slot 名（前端 / planner 可遍历）。
pub const TEMPLATE_SLOTS: &[&str] = &[
    "common",
    "card",
    "power",
    "relic",
    "custom_code",
    "character",
    "potion",
    "planner_guidance",
];

/// 返回 slot 对应的内嵌模板内容；未知 slot 返回 None。
#[must_use]
pub fn get_template(slot: &str) -> Option<&'static str> {
    match slot {
        "common" => Some(COMMON_MD),
        "card" => Some(CARD_MD),
        "power" => Some(POWER_MD),
        "relic" => Some(RELIC_MD),
        "custom_code" => Some(CUSTOM_CODE_MD),
        "character" => Some(CHARACTER_MD),
        "potion" => Some(POTION_MD),
        "planner_guidance" => Some(PLANNER_GUIDANCE_MD),
        _ => None,
    }
}
