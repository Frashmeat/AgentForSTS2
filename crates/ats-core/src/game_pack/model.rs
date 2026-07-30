//! Validated game-pack model exposed to consumers.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedGamePack {
    pub schema_version: u32,
    pub id: String,
    /// SHA-256 of the exact manifest bytes accepted by the loader.
    pub content_sha256: String,
    pub display_name: String,
    pub capabilities: Vec<String>,
    pub truth_sources: Vec<TruthSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruthSource {
    pub id: String,
    pub kind: TruthSourceKind,
    pub indexer: String,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TruthSourceKind {
    LocalFile {
        input_key: String,
    },
    GitHubReleaseAsset {
        repository: String,
        pinned_release: String,
        asset: String,
        sha256: String,
    },
}
