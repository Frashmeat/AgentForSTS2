//! 装配 sts2 知识查找指引（lookup items）。
//!
//! 镜像 Python `Sts2LookupProvider`。Python 端从 `build_lookup_context()` 读
//! runtime 目录状态；本端要求调用方显式传入 `KnowledgePaths` + `SourceMode`，
//! 让函数纯化、便于测试。

use crate::knowledge::contracts::{KnowledgeLookupItem, KnowledgeQuery};
use crate::knowledge::models::SourceMode;
use crate::knowledge::paths::KnowledgePaths;

const ILSPY_EXAMPLE_DLL_PATH: &str = "<sts2_path>/data_sts2_windows_x86_64/sts2.dll";

#[derive(Debug, Default, Clone)]
pub struct Sts2LookupProvider;

impl Sts2LookupProvider {
    pub fn build_lookup(
        &self,
        _query: &KnowledgeQuery,
        paths: &KnowledgePaths,
        game_mode: SourceMode,
    ) -> Vec<KnowledgeLookupItem> {
        let baselib_decompiled = paths.baselib_decompiled_file();
        let baselib_path_str = baselib_decompiled.display().to_string();

        let mut items: Vec<KnowledgeLookupItem> = vec![KnowledgeLookupItem {
            key: "sts2.lookup.baselib".into(),
            title: "BaseLib local source".into(),
            path: baselib_path_str,
            note: "Read this local decompiled source for `CustomCardModel`, `CustomPotionModel`, `PlaceholderCharacterModel`, and related BaseLib wrappers.".into(),
            keywords: vec![
                "BaseLib".into(),
                "CustomCardModel".into(),
                "CustomPotionModel".into(),
                "PlaceholderCharacterModel".into(),
            ],
        }];

        if matches!(game_mode, SourceMode::RuntimeDecompiled) {
            items.push(KnowledgeLookupItem {
                key: "sts2.lookup.game_runtime".into(),
                title: "STS2 runtime knowledge directory".into(),
                path: paths.game_dir.display().to_string(),
                note: "Read or grep this runtime knowledge directory directly. Key subdirs include `MegaCrit.Sts2.Core.Commands`, `MegaCrit.Sts2.Core.Models.Cards`, and `MegaCrit.Sts2.Core.CardSelection`.".into(),
                keywords: vec![
                    "runtime".into(),
                    "knowledge".into(),
                    "DamageCmd".into(),
                    "PowerCmd".into(),
                    "CardSelectorPrefs".into(),
                ],
            });
        } else {
            items.push(KnowledgeLookupItem {
                key: "sts2.lookup.game_fallback".into(),
                title: "STS2 ilspy fallback".into(),
                path: ILSPY_EXAMPLE_DLL_PATH.into(),
                note: "If runtime-decompiled sources are missing, inspect the game DLL via `ilspycmd`.".into(),
                keywords: vec!["ilspycmd".into(), "sts2.dll".into()],
            });
        }

        items.push(KnowledgeLookupItem {
            key: "sts2.lookup.guidance_resources".into(),
            title: "STS2 guidance resources".into(),
            path: paths.resources_dir.display().to_string(),
            note: "Use these Markdown resources for conventions, common pitfalls, and summarized examples.".into(),
            keywords: vec![
                "guidance".into(),
                "common.md".into(),
                "card.md".into(),
                "power.md".into(),
                "relic.md".into(),
                "custom_code.md".into(),
            ],
        });

        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn paths() -> KnowledgePaths {
        KnowledgePaths::from_runtime_dir(Path::new("/tmp/runtime"))
    }

    #[test]
    fn missing_game_mode_emits_fallback_lookup() {
        let provider = Sts2LookupProvider;
        let items = provider.build_lookup(
            &KnowledgeQuery::default(),
            &paths(),
            SourceMode::Missing,
        );
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert!(keys.contains(&"sts2.lookup.baselib"));
        assert!(keys.contains(&"sts2.lookup.game_fallback"));
        assert!(!keys.contains(&"sts2.lookup.game_runtime"));
        assert!(keys.contains(&"sts2.lookup.guidance_resources"));
    }

    #[test]
    fn runtime_decompiled_emits_game_runtime_lookup() {
        let provider = Sts2LookupProvider;
        let items = provider.build_lookup(
            &KnowledgeQuery::default(),
            &paths(),
            SourceMode::RuntimeDecompiled,
        );
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert!(keys.contains(&"sts2.lookup.game_runtime"));
        assert!(!keys.contains(&"sts2.lookup.game_fallback"));
    }
}
