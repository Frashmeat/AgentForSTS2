//! Serializable truth snapshot manifest and verified runtime handle.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const TRUTH_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TruthSnapshotSource {
    pub id: String,
    pub kind: String,
    pub version: Option<String>,
    pub relative_path: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TruthSnapshotIndex {
    pub source_id: String,
    pub indexer: String,
    pub provider: String,
    pub relative_root: String,
    pub file_count: u32,
    pub cs_file_count: u32,
    pub total_bytes: u64,
    pub tree_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TruthSnapshotManifest {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub game_pack_id: String,
    pub game_pack_schema_version: u32,
    pub game_pack_sha256: String,
    pub sources: Vec<TruthSnapshotSource>,
    pub indexes: Vec<TruthSnapshotIndex>,
    pub tool_versions: BTreeMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TruthSnapshotIdentity<'a> {
    pub schema_version: u32,
    pub game_pack_id: &'a str,
    pub game_pack_schema_version: u32,
    pub game_pack_sha256: &'a str,
    pub sources: &'a [TruthSnapshotSource],
    pub indexes: &'a [TruthSnapshotIndex],
    pub tool_versions: &'a BTreeMap<String, String>,
}

impl TruthSnapshotManifest {
    pub(super) fn identity(&self) -> TruthSnapshotIdentity<'_> {
        TruthSnapshotIdentity {
            schema_version: self.schema_version,
            game_pack_id: &self.game_pack_id,
            game_pack_schema_version: self.game_pack_schema_version,
            game_pack_sha256: &self.game_pack_sha256,
            sources: &self.sources,
            indexes: &self.indexes,
            tool_versions: &self.tool_versions,
        }
    }
}

/// An immutable snapshot handle. Only successful store verification can construct it.
#[derive(Debug, Clone)]
pub struct VerifiedTruthSnapshot {
    root: PathBuf,
    manifest: TruthSnapshotManifest,
}

impl VerifiedTruthSnapshot {
    pub(super) fn new(root: PathBuf, manifest: TruthSnapshotManifest) -> Self {
        Self { root, manifest }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    #[must_use]
    pub fn manifest(&self) -> &TruthSnapshotManifest {
        &self.manifest
    }

    #[must_use]
    pub fn snapshot_id(&self) -> &str {
        &self.manifest.snapshot_id
    }

    #[must_use]
    pub fn game_pack_id(&self) -> &str {
        &self.manifest.game_pack_id
    }

    #[must_use]
    pub fn source_path(&self, source_id: &str) -> Option<PathBuf> {
        self.manifest
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .map(|source| self.root.join(&source.relative_path))
    }

    #[must_use]
    pub fn index_root(&self, source_id: &str) -> Option<PathBuf> {
        self.manifest
            .indexes
            .iter()
            .find(|index| index.source_id == source_id)
            .map(|index| self.root.join(&index.relative_root))
    }
}
