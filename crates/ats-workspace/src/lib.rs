//! Project and versioned resource workspace contracts.

mod resource;

pub use resource::{
    RESOURCE_ASSET_SCHEMA_VERSION, ResourceAsset, ResourceBlob, ResourceDeriveRequest,
    ResourceIngestRequest, ResourceOrigin, ResourceRepository, ResourceVersion,
    ResourceVersionProvenance, WorkspaceError, normalize_relative_path,
};
