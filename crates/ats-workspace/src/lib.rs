//! Project and versioned resource workspace contracts.

mod item;
mod project;
mod resource;

pub use item::{
    ITEM_DEFINITION_SCHEMA_VERSION, ItemDefinition, ItemDefinitionError, ItemFieldValue,
    ItemLocalization, ItemResourceBinding, LocalizationStatus,
};

pub use project::{
    AppDataPaths, PROJECT_SCHEMA_VERSION, ProjectError, ProjectFolder, ProjectMeta, RecentEntry,
    RecentProjects, derive_project_identifier,
};

pub use resource::{
    RESOURCE_ASSET_SCHEMA_VERSION, ResourceAsset, ResourceBlob, ResourceBytesIngestRequest,
    ResourceDeriveRequest, ResourceIngestRequest, ResourceOrigin, ResourceRepository,
    ResourceVersion, ResourceVersionProvenance, WorkspaceError, normalize_relative_path,
};
