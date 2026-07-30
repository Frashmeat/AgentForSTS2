//! Declarative game-pack identity and truth-source loading boundary.
//!
//! Core owns parsing and validation. Game-specific provider registrations and
//! declarations are assembled by [`GamePackRegistry`].

mod context;
mod error;
mod loader;
mod model;
mod registry;
mod truth_snapshot;

pub use context::{
    GameContextError, GameContextResult, VerifiedGameContext, VerifiedGameContextEvidence,
};
pub use error::{GamePackError, GamePackResult};
pub use loader::{GamePackLoadPolicy, GamePackLoader, resolve_pack_relative_path};
pub use model::{LoadedGamePack, TruthSource, TruthSourceKind};
pub use registry::GamePackRegistry;
pub use truth_snapshot::{
    GitHubReleaseAssetFetcher, IlspycmdTruthIndexer, RemoteTruthSourceFetcher, TruthSnapshotDraft,
    TruthSnapshotError, TruthSnapshotIndex, TruthSnapshotManifest, TruthSnapshotReadiness,
    TruthSnapshotRefreshError, TruthSnapshotRefreshOutcome, TruthSnapshotRefreshResult,
    TruthSnapshotRefresher, TruthSnapshotResult, TruthSnapshotSource, TruthSnapshotStatus,
    TruthSnapshotStore, TruthSourceIndexer, VerifiedTruthSnapshot, inspect_truth_snapshot,
    validate_truth_source_inputs,
};
