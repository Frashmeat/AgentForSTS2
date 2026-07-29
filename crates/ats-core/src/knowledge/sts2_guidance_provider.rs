//! 把内嵌 sts2 模板按 `KnowledgeQuery` 装成 `KnowledgeGuidanceItem` 列表。
//!
//! 镜像 Python `Sts2GuidanceProvider`。Python 端从磁盘读 `.md`，本端读内嵌模板。
//! `source_path` 字段保留模板 slot 名，方便前端/日志定位。

use crate::knowledge::contracts::{KnowledgeGuidanceItem, KnowledgeQuery, KnowledgeScenario};
use crate::knowledge::templates::get_template;

#[derive(Debug, Default, Clone)]
pub struct Sts2GuidanceProvider;

impl Sts2GuidanceProvider {
    pub fn build_guidance(&self, query: &KnowledgeQuery) -> Vec<KnowledgeGuidanceItem> {
        if matches!(query.scenario, Some(KnowledgeScenario::Planner)) {
            return template_or_empty(
                "sts2.guidance.planner",
                "Planner hints",
                "planner_guidance",
                &["planner".to_string()],
            )
            .into_iter()
            .collect();
        }

        // common 永远先放
        let mut files: Vec<GuidanceFile> = vec![GuidanceFile {
            key: "sts2.guidance.common".into(),
            title: "Common guidance".into(),
            slot: "common".into(),
            asset_types: vec![
                "card".into(),
                "power".into(),
                "relic".into(),
                "custom_code".into(),
                "character".into(),
            ],
        }];

        for asset_type in iter_asset_types(query) {
            files.extend(files_for_asset_type(&asset_type));
        }

        let mut seen = std::collections::HashSet::new();
        let mut out: Vec<KnowledgeGuidanceItem> = Vec::new();
        for file in files {
            if !seen.insert(file.key.clone()) {
                continue;
            }
            if let Some(item) =
                template_or_empty(&file.key, &file.title, &file.slot, &file.asset_types)
            {
                out.push(item);
            }
        }
        out
    }
}

struct GuidanceFile {
    key: String,
    title: String,
    slot: String,
    asset_types: Vec<String>,
}

fn iter_asset_types(query: &KnowledgeQuery) -> Vec<String> {
    let mut ordered: Vec<String> = Vec::new();
    if let Some(at) = &query.asset_type
        && !at.is_empty()
    {
        ordered.push(normalize_asset_type(at));
    }
    for at in &query.group_asset_types {
        let normalized = normalize_asset_type(at);
        if !ordered.iter().any(|x| x == &normalized) {
            ordered.push(normalized);
        }
    }
    if ordered.is_empty() && matches!(query.scenario, Some(KnowledgeScenario::CustomCodeCodegen)) {
        ordered.push("custom_code".into());
    }
    ordered
}

fn normalize_asset_type(raw: &str) -> String {
    let compact: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    match compact.as_str() {
        "cardfullscreen" => "card_fullscreen".into(),
        "customcode" => "custom_code".into(),
        _ => raw.trim().to_ascii_lowercase(),
    }
}

fn files_for_asset_type(asset_type: &str) -> Vec<GuidanceFile> {
    match asset_type {
        "card" | "card_fullscreen" => vec![GuidanceFile {
            key: "sts2.guidance.card".into(),
            title: "Card guidance".into(),
            slot: "card".into(),
            asset_types: vec!["card".into(), "card_fullscreen".into()],
        }],
        "power" => vec![GuidanceFile {
            key: "sts2.guidance.power".into(),
            title: "Power guidance".into(),
            slot: "power".into(),
            asset_types: vec!["power".into()],
        }],
        "relic" => vec![GuidanceFile {
            key: "sts2.guidance.relic".into(),
            title: "Relic guidance".into(),
            slot: "relic".into(),
            asset_types: vec!["relic".into()],
        }],
        "character" => vec![GuidanceFile {
            key: "sts2.guidance.character".into(),
            title: "Character guidance".into(),
            slot: "character".into(),
            asset_types: vec!["character".into()],
        }],
        "custom_code" => vec![
            GuidanceFile {
                key: "sts2.guidance.custom_code".into(),
                title: "Custom code guidance".into(),
                slot: "custom_code".into(),
                asset_types: vec!["custom_code".into()],
            },
            GuidanceFile {
                key: "sts2.guidance.potion".into(),
                title: "Potion guidance".into(),
                slot: "potion".into(),
                asset_types: vec!["custom_code".into()],
            },
            GuidanceFile {
                key: "sts2.guidance.character".into(),
                title: "Character guidance".into(),
                slot: "character".into(),
                asset_types: vec!["custom_code".into(), "character".into()],
            },
        ],
        _ => Vec::new(),
    }
}

fn template_or_empty(
    key: &str,
    title: &str,
    slot: &str,
    asset_types: &[String],
) -> Option<KnowledgeGuidanceItem> {
    let body = get_template(slot)?.trim().to_string();
    if body.is_empty() {
        return None;
    }
    Some(KnowledgeGuidanceItem {
        key: key.into(),
        title: title.into(),
        body,
        source_path: format!("embedded:sts2/{slot}.md"),
        asset_types: asset_types.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::KnowledgeScenario;

    #[test]
    fn planner_scenario_returns_only_planner_item() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::Planner),
            ..Default::default()
        };
        let items = Sts2GuidanceProvider.build_guidance(&query);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "sts2.guidance.planner");
    }

    #[test]
    fn card_asset_type_yields_common_plus_card() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetCodegen),
            asset_type: Some("card".into()),
            ..Default::default()
        };
        let items = Sts2GuidanceProvider.build_guidance(&query);
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert!(keys.contains(&"sts2.guidance.common"));
        assert!(keys.contains(&"sts2.guidance.card"));
    }

    #[test]
    fn asset_type_matching_is_case_insensitive() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetCodegen),
            asset_type: Some("Relic".into()),
            ..Default::default()
        };
        let items = Sts2GuidanceProvider.build_guidance(&query);
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert!(keys.contains(&"sts2.guidance.relic"));
    }

    #[test]
    fn relic_guidance_is_stable_scaffold_without_energy_hook_recipe() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetCodegen),
            asset_type: Some("relic".into()),
            ..Default::default()
        };
        let items = Sts2GuidanceProvider.build_guidance(&query);
        let relic = items
            .iter()
            .find(|item| item.key == "sts2.guidance.relic")
            .expect("relic guidance");
        assert!(!relic.body.contains("BeforeCombatStart"));
        assert!(!relic.body.contains("PlayerCmd.GainEnergy"));
        assert!(relic.body.contains("Behavior evidence — REQUIRED"));
        assert!(relic.body.contains("lifecycle caller"));
    }

    #[test]
    fn custom_code_codegen_defaults_to_custom_code_bundle() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::CustomCodeCodegen),
            ..Default::default()
        };
        let items = Sts2GuidanceProvider.build_guidance(&query);
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert!(keys.contains(&"sts2.guidance.custom_code"));
        assert!(keys.contains(&"sts2.guidance.potion"));
        assert!(keys.contains(&"sts2.guidance.character"));
    }

    #[test]
    fn duplicate_asset_types_dedup() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetCodegen),
            asset_type: Some("card".into()),
            group_asset_types: vec!["card".into(), "card_fullscreen".into()],
            ..Default::default()
        };
        let items = Sts2GuidanceProvider.build_guidance(&query);
        let card_count = items
            .iter()
            .filter(|i| i.key == "sts2.guidance.card")
            .count();
        assert_eq!(card_count, 1);
    }
}
