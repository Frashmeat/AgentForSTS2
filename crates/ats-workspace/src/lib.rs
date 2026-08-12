//! Project and versioned resource workspace contracts.

mod composition;
mod item;
mod project;
mod resource;

pub use composition::{
    COMPOSITION_DRAFT_SCHEMA_VERSION, CompositionDraft, CompositionDraftCreateOrMatch,
    CompositionDraftError, CompositionDraftNode, CompositionDraftRepository,
    CompositionDraftRepositoryErrorKind,
};
pub use item::{
    AtomicItemRepository, AtomicItemSaveError, AtomicItemSaveRequest,
    ITEM_DEFINITION_SCHEMA_VERSION, ItemCompositionProfile, ItemCompositionSource, ItemDefinition,
    ItemDefinitionError, ItemFieldValue, ItemLocalization, ItemReferenceBinding, ItemRepository,
    ItemRepositoryErrorKind, ItemResourceBinding, LocalizationStatus, StoredItemDefinition,
};

pub use project::{
    AppDataPaths, LocalBuildPaths, PROJECT_SCHEMA_VERSION, ProjectError, ProjectFolder,
    ProjectLocalConfigError, ProjectMeta, RecentEntry, RecentProjects, derive_project_identifier,
    sync_project_local_props,
};

pub use resource::{
    PreparedResourceMedia, RESOURCE_ASSET_SCHEMA_VERSION, ResourceAsset, ResourceBlob,
    ResourceBytesIngestRequest, ResourceDeriveRequest, ResourceMediaProcessor, ResourceOrigin,
    ResourceRepository, ResourceTransformOperation, ResourceVersion, ResourceVersionProvenance,
    WorkspaceError, normalize_relative_path,
};
