//! Generic guidance selection over resources verified by the active Game Pack.

use std::collections::BTreeSet;

use crate::game_pack::{GuidanceScenario, LoadedGamePack};
use crate::knowledge::contracts::{KnowledgeGuidanceItem, KnowledgeQuery, KnowledgeScenario};

#[derive(Debug, Default, Clone)]
pub struct GamePackGuidanceProvider;

impl GamePackGuidanceProvider {
    #[must_use]
    pub fn build_guidance(
        &self,
        query: &KnowledgeQuery,
        pack: &LoadedGamePack,
    ) -> Vec<KnowledgeGuidanceItem> {
        let Some(guidance) = &pack.guidance else {
            return Vec::new();
        };
        let Some(scenario) = guidance_scenario(query.scenario.as_ref()) else {
            return Vec::new();
        };
        let asset_types = query_asset_types(query);
        guidance
            .items
            .iter()
            .filter(|item| item.scenarios.contains(&scenario))
            .filter(|item| {
                item.always
                    || item
                        .asset_types
                        .iter()
                        .any(|asset_type| asset_types.contains(asset_type))
            })
            .map(|item| KnowledgeGuidanceItem {
                key: format!("{}.guidance.{}", pack.id, item.id),
                title: item.title.clone(),
                body: item.body.clone(),
                source_path: item.source_path.clone(),
                asset_types: item.asset_types.clone(),
            })
            .collect()
    }
}

fn guidance_scenario(scenario: Option<&KnowledgeScenario>) -> Option<GuidanceScenario> {
    match scenario? {
        KnowledgeScenario::Planner => Some(GuidanceScenario::Planner),
        KnowledgeScenario::AssetCodegen => Some(GuidanceScenario::AssetCodegen),
        KnowledgeScenario::CustomCodeCodegen => Some(GuidanceScenario::CustomCodeCodegen),
        KnowledgeScenario::AssetGroupCodegen => Some(GuidanceScenario::AssetGroupCodegen),
    }
}

fn query_asset_types(query: &KnowledgeQuery) -> BTreeSet<String> {
    let mut asset_types = BTreeSet::new();
    if let Some(asset_type) = &query.asset_type
        && !asset_type.trim().is_empty()
    {
        asset_types.insert(normalize_asset_type(asset_type));
    }
    asset_types.extend(
        query
            .group_asset_types
            .iter()
            .map(|asset_type| normalize_asset_type(asset_type)),
    );
    if asset_types.is_empty()
        && matches!(query.scenario, Some(KnowledgeScenario::CustomCodeCodegen))
    {
        asset_types.insert("custom_code".into());
    }
    asset_types
}

fn normalize_asset_type(raw: &str) -> String {
    let compact: String = raw
        .trim()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect();
    match compact.as_str() {
        "cardfullscreen" => "card_fullscreen".into(),
        "customcode" => "custom_code".into(),
        _ => raw.trim().to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_pack::GamePackRegistry;

    fn sts2_pack() -> LoadedGamePack {
        GamePackRegistry::built_in()
            .unwrap()
            .require("sts2")
            .unwrap()
            .clone()
    }

    #[test]
    fn planner_scenario_returns_only_planner_item() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::Planner),
            ..Default::default()
        };
        let items = GamePackGuidanceProvider.build_guidance(&query, &sts2_pack());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].key, "sts2.guidance.planner");
    }

    #[test]
    fn card_asset_type_yields_common_plus_card() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetCodegen),
            asset_type: Some("CARD-FULLSCREEN".into()),
            ..Default::default()
        };
        let items = GamePackGuidanceProvider.build_guidance(&query, &sts2_pack());
        let keys: Vec<_> = items.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, ["sts2.guidance.common", "sts2.guidance.card"]);
    }

    #[test]
    fn relic_guidance_has_no_persisted_energy_hook_recipe() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::AssetCodegen),
            asset_type: Some("relic".into()),
            ..Default::default()
        };
        let items = GamePackGuidanceProvider.build_guidance(&query, &sts2_pack());
        let relic = items
            .iter()
            .find(|item| item.key == "sts2.guidance.relic")
            .unwrap();
        assert!(!relic.body.contains("BeforeCombatStart"));
        assert!(!relic.body.contains("PlayerCmd.GainEnergy"));
        assert!(relic.body.contains("using MegaCrit.Sts2.Core.Combat;"));
        assert!(relic.body.contains("Behavior evidence — REQUIRED"));
    }

    #[test]
    fn custom_code_bundle_is_declared_by_the_pack() {
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::CustomCodeCodegen),
            ..Default::default()
        };
        let items = GamePackGuidanceProvider.build_guidance(&query, &sts2_pack());
        let keys: Vec<_> = items.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "sts2.guidance.common",
                "sts2.guidance.character",
                "sts2.guidance.custom_code",
                "sts2.guidance.potion"
            ]
        );
    }
}
