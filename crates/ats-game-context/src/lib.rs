//! Validated Game Pack contribution and immutable Truth Evidence contracts.

mod asset;
mod behavior;
mod contribution;
mod item;
mod pack;
mod pipeline;
mod template;
mod truth;

pub use asset::{GamePackAssetError, built_in_game_pack_asset};
pub use ats_kernel::GamePackId;
pub use behavior::{
    BEHAVIOR_PROPOSAL_SCHEMA_VERSION, BehaviorAdapterError, BehaviorAdapterIdentity,
    BehaviorAdapterRegistry, BehaviorAdapterRegistryError, BehaviorCapabilitySet, BehaviorIssue,
    BehaviorIssueCode, BehaviorItemContext, BehaviorItemReference, BehaviorProposal,
    BehaviorRenderContext, BehaviorResourceBinding, CAPABILITY_CATALOG_SCHEMA_VERSION,
    CapabilityCatalog, CapabilityCatalogIdentity, CapabilityInvocation, CapabilityParameterSpec,
    CapabilityParameterType, CapabilitySpec, CapabilityValue, GameBehaviorAdapter,
    PackBehaviorContract, RENDERED_ITEM_BUNDLE_SCHEMA_VERSION, RenderedFile, RenderedFileMerge,
    RenderedFileMergeKeyPolicy, RenderedItemBundle,
};
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
pub use pipeline::{
    GamePipelineProvider, GamePipelineRegistry, PipelineCheckpointPolicy, PipelineGraphError,
    PipelineNode, PipelineNodePhase, PipelineNodeScope, PipelinePrimitiveBinding,
    PipelineProviderError, PipelineProviderIdentity, PipelinePublishBarrier, PipelineRegistryError,
    PipelineResolveRequest, PipelineRetryClass, PipelineSelection, PipelineValueContract,
    PipelineWorkItem, ResolvedPipelineGraph,
};
pub use template::{ProjectTemplateError, built_in_project_template};
pub use truth::{
    EvidenceQuery, EvidenceQueryError, TRUTH_SNAPSHOT_SCHEMA_VERSION, TruthEvidenceRecord,
    TruthSnapshotIndex, TruthSnapshotManifest, TruthSnapshotRepository, TruthSnapshotSource,
    TruthStoreError, VerifiedGameContext, VerifiedGameContextError, VerifiedTruthSnapshot,
    normalize_relative_path,
};
