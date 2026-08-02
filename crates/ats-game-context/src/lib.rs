//! Validated Game Pack contribution and immutable Truth Evidence contracts.

mod contribution;
mod pack;
mod template;
mod truth;

pub use ats_kernel::GamePackId;
pub use contribution::{
    ContributionRequirement, ContributionResolver, ContributionResolverError,
    VerifiedContributionSet,
};
pub use pack::{
    GAME_PACK_SCHEMA_VERSION, GamePackLoadError, GamePackLoader, GamePackRegistry,
    GamePackRegistryError, LoadedGamePack, PackContribution,
};
pub use template::{ProjectTemplateError, built_in_project_template};
pub use truth::{
    EvidenceQuery, EvidenceQueryError, TRUTH_SNAPSHOT_SCHEMA_VERSION, TruthEvidenceRecord,
    TruthSnapshotIndex, TruthSnapshotManifest, TruthSnapshotRepository, TruthSnapshotSource,
    TruthStoreError, VerifiedGameContext, VerifiedGameContextError, VerifiedTruthSnapshot,
    normalize_relative_path,
};
