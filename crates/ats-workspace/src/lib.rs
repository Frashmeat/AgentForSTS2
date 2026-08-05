//! Project and versioned resource workspace contracts.

mod item;
mod project;
mod resource;

pub use item::{
    ITEM_DEFINITION_SCHEMA_VERSION, ItemCompositionProfile, ItemCompositionSource, ItemDefinition,
    ItemDefinitionError, ItemFieldValue, ItemLocalization, ItemReferenceBinding, ItemRepository,
    ItemResourceBinding, LocalizationStatus, StoredItemDefinition,
};

pub use project::{
    AppDataPaths, PROJECT_SCHEMA_VERSION, ProjectError, ProjectFolder, ProjectMeta, RecentEntry,
    RecentProjects, derive_project_identifier,
};

pub use resource::{
    PreparedResourceMedia, RESOURCE_ASSET_SCHEMA_VERSION, ResourceAsset, ResourceBlob,
    ResourceBytesIngestRequest, ResourceDeriveRequest, ResourceMediaProcessor, ResourceOrigin,
    ResourceRepository, ResourceTransformOperation, ResourceVersion, ResourceVersionProvenance,
    WorkspaceError, normalize_relative_path,
};
