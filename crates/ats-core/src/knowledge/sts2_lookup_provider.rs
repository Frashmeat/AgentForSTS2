//! 装配 sts2 知识查找指引（lookup items）。
//!
//! Lookup coordinates name immutable Snapshot indexes; no mutable cache or
//! decompiler fallback is exposed to generation.

use crate::game_pack::VerifiedGameContext;
use crate::knowledge::contracts::{KnowledgeLookupItem, KnowledgeQuery};

#[derive(Debug, Default, Clone)]
pub struct Sts2LookupProvider;

impl Sts2LookupProvider {
    pub fn build_lookup(
        &self,
        _query: &KnowledgeQuery,
        context: &VerifiedGameContext,
    ) -> Vec<KnowledgeLookupItem> {
        context
            .snapshot()
            .manifest()
            .indexes
            .iter()
            .map(|index| KnowledgeLookupItem {
                key: format!("sts2.lookup.snapshot.{}", index.source_id),
                title: format!("Verified {} source index", index.source_id),
                path: format!("snapshot://{}/{}/", context.snapshot_id(), index.source_id),
                note: format!(
                    "Immutable source index verified by `{}` for this generation context.",
                    index.indexer
                ),
                keywords: vec![
                    "verified".into(),
                    "snapshot".into(),
                    index.source_id.clone(),
                    index.provider.clone(),
                ],
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::test_support::fixture_game_context;

    #[test]
    fn lookup_uses_only_verified_snapshot_coordinates() {
        let provider = Sts2LookupProvider;
        let temp = tempfile::TempDir::new().unwrap();
        let context = fixture_game_context(temp.path(), &[], &[]);
        let items = provider.build_lookup(&KnowledgeQuery::default(), &context);
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(
            keys,
            ["sts2.lookup.snapshot.baselib", "sts2.lookup.snapshot.game"]
        );
        assert!(
            items
                .iter()
                .all(|item| item.path.starts_with("snapshot://"))
        );
        assert!(items.iter().all(|item| !item.path.contains("ilspy")));
    }
}
