//! Sts2 知识装配总入口：3 个 provider 的协调者。
//!
//! 镜像 Python `Sts2KnowledgeResolver`。区别：本端要求调用方显式传入
//! `KnowledgePaths` + 游戏端 `SourceMode`，避免在 resolver 内部做 IO。

use crate::knowledge::contracts::{KnowledgePacket, KnowledgeQuery};
use crate::knowledge::models::SourceMode;
use crate::knowledge::paths::KnowledgePaths;
use crate::knowledge::sts2_code_facts_provider::Sts2CodeFactsProvider;
use crate::knowledge::sts2_guidance_provider::Sts2GuidanceProvider;
use crate::knowledge::sts2_lookup_provider::Sts2LookupProvider;

#[derive(Debug, Default, Clone)]
pub struct Sts2KnowledgeResolver {
    pub code_facts: Sts2CodeFactsProvider,
    pub guidance: Sts2GuidanceProvider,
    pub lookup: Sts2LookupProvider,
}

impl Sts2KnowledgeResolver {
    pub fn resolve(
        &self,
        query: &KnowledgeQuery,
        paths: &KnowledgePaths,
        game_mode: SourceMode,
    ) -> KnowledgePacket {
        let (facts, warnings) = self.code_facts.build_facts(query, paths, game_mode);
        let guidance = self.guidance.build_guidance(query);
        let lookup = self.lookup.build_lookup(query, paths, game_mode);

        let scenario = query
            .scenario
            .as_ref()
            .map(|s| format!("{:?}", s).to_ascii_lowercase())
            .unwrap_or_default();
        let summary = format!("{}:{}", query.domain, scenario);

        KnowledgePacket {
            domain: query.domain.clone(),
            scenario,
            summary,
            facts,
            guidance,
            lookup,
            warnings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::KnowledgeScenario;
    use std::path::Path;

    #[test]
    fn resolve_planner_returns_planner_guidance_and_lookup() {
        let resolver = Sts2KnowledgeResolver::default();
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::Planner),
            domain: "sts2".into(),
            ..Default::default()
        };
        let paths = KnowledgePaths::from_runtime_dir(Path::new("/tmp/runtime"));
        let packet = resolver.resolve(&query, &paths, SourceMode::Missing);
        assert_eq!(packet.domain, "sts2");
        assert!(!packet.guidance.is_empty());
        assert!(!packet.lookup.is_empty());
        // facts 空（stub），但应有 warning
        assert!(packet.facts.is_empty());
        assert!(!packet.warnings.is_empty());
    }
}
