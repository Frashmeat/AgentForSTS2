//! Sts2 知识装配总入口：3 个 provider 的协调者。
//!
//! Production resolution is bound to one verified, immutable game context.

use crate::game_pack::VerifiedGameContext;
use crate::knowledge::contracts::{KnowledgePacket, KnowledgeQuery};
use crate::knowledge::game_pack_guidance_provider::GamePackGuidanceProvider;
use crate::knowledge::sts2_code_facts_provider::{SnapshotCodeFactsError, Sts2CodeFactsProvider};
use crate::knowledge::sts2_lookup_provider::Sts2LookupProvider;

#[derive(Debug, Default, Clone)]
pub struct Sts2KnowledgeResolver {
    pub code_facts: Sts2CodeFactsProvider,
    pub guidance: GamePackGuidanceProvider,
    pub lookup: Sts2LookupProvider,
}

impl Sts2KnowledgeResolver {
    pub fn resolve(
        &self,
        query: &KnowledgeQuery,
        context: &VerifiedGameContext,
    ) -> Result<KnowledgePacket, SnapshotCodeFactsError> {
        let (facts, warnings) = self.code_facts.build_facts_from_snapshot(
            query,
            context.snapshot(),
            "sts2_code_facts",
        )?;
        let guidance = self.guidance.build_guidance(query, context.pack());
        let lookup = self.lookup.build_lookup(query, context);

        let scenario = query
            .scenario
            .as_ref()
            .map(|s| format!("{:?}", s).to_ascii_lowercase())
            .unwrap_or_default();
        let summary = format!("{}:{}", query.domain, scenario);

        Ok(KnowledgePacket {
            domain: query.domain.clone(),
            scenario,
            summary,
            facts,
            guidance,
            lookup,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::KnowledgeScenario;
    use crate::knowledge::test_support::fixture_game_context;

    #[test]
    fn resolve_planner_returns_planner_guidance_and_lookup() {
        let resolver = Sts2KnowledgeResolver::default();
        let query = KnowledgeQuery {
            scenario: Some(KnowledgeScenario::Planner),
            domain: "sts2".into(),
            ..Default::default()
        };
        let temp = tempfile::TempDir::new().unwrap();
        let context = fixture_game_context(temp.path(), &[], &[]);
        let packet = resolver.resolve(&query, &context).unwrap();
        assert_eq!(packet.domain, "sts2");
        assert!(!packet.guidance.is_empty());
        assert!(!packet.lookup.is_empty());
        assert!(packet.facts.is_empty());
        assert!(!packet.warnings.is_empty());
        assert!(
            packet
                .lookup
                .iter()
                .all(|item| item.path.starts_with("snapshot://"))
        );
    }
}
