//! Knowledge selection and stable STS2 guidance over verified Truth Snapshots.
//!
//! The legacy runtime knowledge layout remains test-only for semantic equivalence fixtures.

mod contracts;
mod decompile;
mod game_pack_guidance_provider;
#[cfg(test)]
mod models;
#[cfg(test)]
mod paths;
mod sts2_code_facts_provider;
mod sts2_knowledge_resolver;
mod sts2_lookup_provider;
#[cfg(test)]
pub(crate) mod test_support;

pub use contracts::{
    KnowledgeFactItem, KnowledgeGuidanceItem, KnowledgeLookupItem, KnowledgePacket, KnowledgeQuery,
    KnowledgeScenario,
};
pub use decompile::{
    DecompileError, DecompileStats, default_dotnet_tools_dirs, discover_ilspycmd,
    discover_ilspycmd_in, run_decompile_file, run_decompile_project,
};
#[cfg(test)]
pub use models::{BaselibStatus, GameStatus, KnowledgeStatus, OverallState, SourceMode};
#[cfg(test)]
pub use paths::KnowledgePaths;
#[cfg(test)]
pub fn ensure_dirs(paths: &KnowledgePaths) -> std::io::Result<()> {
    for directory in [
        &paths.root,
        &paths.game_dir,
        &paths.baselib_dir,
        &paths.resources_dir,
        &paths.cache_dir,
        &paths.packs_dir,
    ] {
        std::fs::create_dir_all(directory)?;
    }
    Ok(())
}
pub use game_pack_guidance_provider::GamePackGuidanceProvider;
pub use sts2_code_facts_provider::{SnapshotCodeFactsError, Sts2CodeFactsProvider};
pub use sts2_knowledge_resolver::Sts2KnowledgeResolver;
pub use sts2_lookup_provider::Sts2LookupProvider;
