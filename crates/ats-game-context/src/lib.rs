//! Validated Game Pack contribution and immutable Truth Evidence contracts.

mod asset;
mod contribution;
mod item;
mod pack;
mod template;
mod truth;

pub use asset::{GamePackAssetError, built_in_game_pack_asset};
pub use ats_kernel::GamePackId;
pub use contribution::{
    ContributionRequirement, ContributionResolver, ContributionResolverError,
    VerifiedContributionSet,
};
pub use item::{
    CapabilityEvaluationError, CompositionConstraintSpec, CompositionParameterSpec,
    CompositionProfileError, CompositionProfileSet, CompositionProfileSpec, ItemCapabilityBlocker,
    ItemCapabilityCatalog, ItemCatalogError, ItemChoiceOption, ItemEvidenceQuery, ItemFieldSpec,
    ItemFieldValueSpec, ItemReferenceKind, ItemReferenceSlotSpec, ItemResourceProfileSpec,
    ItemTypeCapability, ItemTypeDescriptor, LocalizationFieldSpec,
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
