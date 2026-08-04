use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use ats_kernel::{PrimitiveId, Sha256Digest};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{GamePackId, GamePackRegistry, GamePackRegistryError, LoadedGamePack};

pub const TRUTH_SNAPSHOT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TruthSnapshotSource {
    pub id: String,
    pub kind: String,
    pub version: Option<String>,
    pub relative_path: String,
    pub sha256: Sha256Digest,
    pub byte_length: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TruthSnapshotIndex {
    pub id: String,
    pub provider: PrimitiveId,
    pub relative_path: String,
    pub sha256: Sha256Digest,
    pub record_count: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TruthSnapshotManifest {
    schema_version: u32,
    snapshot_id: Sha256Digest,
    game_pack_id: GamePackId,
    game_pack_schema_version: u32,
    game_pack_sha256: Sha256Digest,
    sources: Vec<TruthSnapshotSource>,
    indexes: Vec<TruthSnapshotIndex>,
    tool_versions: BTreeMap<String, String>,
    created_at: DateTime<Utc>,
}

impl TruthSnapshotManifest {
    pub fn new(
        pack: &LoadedGamePack,
        sources: Vec<TruthSnapshotSource>,
        indexes: Vec<TruthSnapshotIndex>,
        tool_versions: BTreeMap<String, String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, TruthStoreError> {
        let mut sources = sources;
        sources.sort_by(|left, right| left.id.cmp(&right.id));
        let mut indexes = indexes;
        indexes.sort_by(|left, right| left.id.cmp(&right.id));
        let mut manifest = Self {
            schema_version: TRUTH_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: Sha256Digest::parse("0".repeat(64))
                .expect("fixed placeholder digest is valid"),
            game_pack_id: pack.id().clone(),
            game_pack_schema_version: crate::GAME_PACK_SCHEMA_VERSION,
            game_pack_sha256: pack.content_sha256().clone(),
            sources,
            indexes,
            tool_versions,
            created_at,
        };
        manifest.validate_structure()?;
        manifest.snapshot_id = manifest.compute_identity()?;
        Ok(manifest)
    }

    pub fn verify_for_pack(&self, pack: &LoadedGamePack) -> Result<(), TruthStoreError> {
        self.validate_structure()?;
        if self.game_pack_id != *pack.id()
            || self.game_pack_schema_version != crate::GAME_PACK_SCHEMA_VERSION
            || self.game_pack_sha256 != *pack.content_sha256()
        {
            return Err(TruthStoreError::PackIdentityMismatch);
        }
        if self.compute_identity()? != self.snapshot_id {
            return Err(TruthStoreError::SnapshotIdentityMismatch);
        }
        Ok(())
    }

    fn validate_structure(&self) -> Result<(), TruthStoreError> {
        if self.schema_version != TRUTH_SNAPSHOT_SCHEMA_VERSION {
            return Err(TruthStoreError::UnsupportedSchema);
        }
        if self.sources.is_empty() || self.indexes.is_empty() {
            return Err(TruthStoreError::InvalidManifest);
        }
        let mut source_ids = BTreeSet::new();
        let mut source_paths = BTreeSet::new();
        for source in &self.sources {
            if !valid_local_id(&source.id)
                || !valid_local_id(&source.kind)
                || source.byte_length == 0
                || normalize_relative_path(Path::new(&source.relative_path))?
                    != source.relative_path
                || !source.relative_path.starts_with("sources/")
                || !source_ids.insert(source.id.as_str())
                || !source_paths.insert(source.relative_path.as_str())
                || source
                    .version
                    .as_ref()
                    .is_some_and(|version| version.is_empty() || version.len() > 128)
            {
                return Err(TruthStoreError::InvalidManifest);
            }
        }
        if !self
            .sources
            .windows(2)
            .all(|items| items[0].id < items[1].id)
        {
            return Err(TruthStoreError::InvalidManifest);
        }
        let mut index_ids = BTreeSet::new();
        let mut index_paths = BTreeSet::new();
        for index in &self.indexes {
            if !valid_local_id(&index.id)
                || index.record_count == 0
                || normalize_relative_path(Path::new(&index.relative_path))? != index.relative_path
                || !index.relative_path.starts_with("indexes/")
                || !index_ids.insert(index.id.as_str())
                || !index_paths.insert(index.relative_path.as_str())
            {
                return Err(TruthStoreError::InvalidManifest);
            }
        }
        if !self
            .indexes
            .windows(2)
            .all(|items| items[0].id < items[1].id)
        {
            return Err(TruthStoreError::InvalidManifest);
        }
        if self.tool_versions.iter().any(|(tool, version)| {
            !valid_local_id(tool)
                || version.is_empty()
                || version.len() > 128
                || version.chars().any(char::is_control)
        }) {
            return Err(TruthStoreError::InvalidManifest);
        }
        Ok(())
    }

    fn compute_identity(&self) -> Result<Sha256Digest, TruthStoreError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Identity<'a> {
            schema_version: u32,
            game_pack_id: &'a GamePackId,
            game_pack_schema_version: u32,
            game_pack_sha256: &'a Sha256Digest,
            sources: &'a [TruthSnapshotSource],
            indexes: &'a [TruthSnapshotIndex],
            tool_versions: &'a BTreeMap<String, String>,
        }

        let bytes = serde_json::to_vec(&Identity {
            schema_version: self.schema_version,
            game_pack_id: &self.game_pack_id,
            game_pack_schema_version: self.game_pack_schema_version,
            game_pack_sha256: &self.game_pack_sha256,
            sources: &self.sources,
            indexes: &self.indexes,
            tool_versions: &self.tool_versions,
        })
        .map_err(|_| TruthStoreError::InvalidManifest)?;
        Ok(sha256_bytes(&bytes))
    }

    #[must_use]
    pub fn snapshot_id(&self) -> &Sha256Digest {
        &self.snapshot_id
    }

    #[must_use]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub fn game_pack_id(&self) -> &GamePackId {
        &self.game_pack_id
    }

    #[must_use]
    pub fn game_pack_sha256(&self) -> &Sha256Digest {
        &self.game_pack_sha256
    }

    #[must_use]
    pub fn sources(&self) -> &[TruthSnapshotSource] {
        &self.sources
    }

    #[must_use]
    pub fn indexes(&self) -> &[TruthSnapshotIndex] {
        &self.indexes
    }

    #[must_use]
    pub fn tool_versions(&self) -> &BTreeMap<String, String> {
        &self.tool_versions
    }

    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

impl<'de> Deserialize<'de> for TruthSnapshotManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Wire {
            schema_version: u32,
            snapshot_id: Sha256Digest,
            game_pack_id: GamePackId,
            game_pack_schema_version: u32,
            game_pack_sha256: Sha256Digest,
            sources: Vec<TruthSnapshotSource>,
            indexes: Vec<TruthSnapshotIndex>,
            tool_versions: BTreeMap<String, String>,
            created_at: DateTime<Utc>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let manifest = Self {
            schema_version: wire.schema_version,
            snapshot_id: wire.snapshot_id,
            game_pack_id: wire.game_pack_id,
            game_pack_schema_version: wire.game_pack_schema_version,
            game_pack_sha256: wire.game_pack_sha256,
            sources: wire.sources,
            indexes: wire.indexes,
            tool_versions: wire.tool_versions,
            created_at: wire.created_at,
        };
        manifest
            .validate_structure()
            .map_err(serde::de::Error::custom)?;
        Ok(manifest)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TruthEvidenceRecord {
    pub source_id: String,
    pub symbol: String,
    pub purpose: String,
    pub bounded_excerpt: String,
    pub relative_path: String,
}

impl TruthEvidenceRecord {
    fn validate(&self, source_ids: &BTreeSet<&str>) -> Result<(), TruthStoreError> {
        if !source_ids.contains(self.source_id.as_str())
            || self.symbol.trim().is_empty()
            || self.symbol.len() > 256
            || self.purpose.trim().is_empty()
            || self.purpose.len() > 512
            || self.bounded_excerpt.is_empty()
            || self.bounded_excerpt.len() > 2_000
            || normalize_relative_path(Path::new(&self.relative_path))? != self.relative_path
        {
            return Err(TruthStoreError::InvalidEvidence);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct VerifiedTruthSnapshot {
    manifest: TruthSnapshotManifest,
    evidence: Vec<TruthEvidenceRecord>,
}

impl VerifiedTruthSnapshot {
    pub fn verify(
        pack: &LoadedGamePack,
        manifest: TruthSnapshotManifest,
        evidence_by_index: BTreeMap<String, Vec<TruthEvidenceRecord>>,
    ) -> Result<Self, TruthStoreError> {
        manifest.verify_for_pack(pack)?;
        if evidence_by_index.len() != manifest.indexes.len() {
            return Err(TruthStoreError::InvalidEvidence);
        }
        let source_ids: BTreeSet<&str> = manifest
            .sources
            .iter()
            .map(|source| source.id.as_str())
            .collect();
        let mut evidence = Vec::new();
        for index in &manifest.indexes {
            let records = evidence_by_index
                .get(&index.id)
                .ok_or(TruthStoreError::InvalidEvidence)?;
            if usize::try_from(index.record_count).ok() != Some(records.len()) {
                return Err(TruthStoreError::InvalidEvidence);
            }
            for record in records {
                record.validate(&source_ids)?;
                evidence.push(record.clone());
            }
        }
        evidence.sort_by(|left, right| {
            (&left.source_id, &left.symbol, &left.relative_path).cmp(&(
                &right.source_id,
                &right.symbol,
                &right.relative_path,
            ))
        });
        Ok(Self { manifest, evidence })
    }

    #[must_use]
    pub fn manifest(&self) -> &TruthSnapshotManifest {
        &self.manifest
    }

    pub fn query(
        &self,
        query: &EvidenceQuery,
    ) -> Result<Vec<TruthEvidenceRecord>, EvidenceQueryError> {
        query.validate()?;
        let symbols: Vec<String> = query
            .symbols
            .iter()
            .map(|value| value.to_lowercase())
            .collect();
        let terms: Vec<String> = query
            .terms
            .iter()
            .map(|value| value.to_lowercase())
            .collect();
        Ok(self
            .evidence
            .iter()
            .filter(|record| {
                let symbol = record.symbol.to_lowercase();
                let searchable = format!(
                    "{} {} {}",
                    symbol,
                    record.purpose.to_lowercase(),
                    record.bounded_excerpt.to_lowercase()
                );
                (symbols.is_empty() || symbols.iter().any(|value| symbol.contains(value)))
                    && (terms.is_empty() || terms.iter().all(|value| searchable.contains(value)))
            })
            .take(usize::from(query.limit))
            .cloned()
            .collect())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EvidenceQuery {
    pub symbols: Vec<String>,
    pub terms: Vec<String>,
    pub limit: u16,
}

impl EvidenceQuery {
    fn validate(&self) -> Result<(), EvidenceQueryError> {
        if (self.symbols.is_empty() && self.terms.is_empty())
            || self.limit == 0
            || self.limit > 50
            || self
                .symbols
                .iter()
                .chain(&self.terms)
                .any(|value| value.trim().is_empty() || value.len() > 128)
        {
            Err(EvidenceQueryError::InvalidQuery)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum EvidenceQueryError {
    #[error("truth evidence query is invalid")]
    InvalidQuery,
}

#[derive(Debug, Error)]
pub enum TruthStoreError {
    #[error("truth snapshot schema version is unsupported")]
    UnsupportedSchema,
    #[error("truth snapshot manifest is invalid")]
    InvalidManifest,
    #[error("truth snapshot Pack identity does not match")]
    PackIdentityMismatch,
    #[error("truth snapshot content identity does not match")]
    SnapshotIdentityMismatch,
    #[error("truth snapshot file hash does not match")]
    HashMismatch,
    #[error("truth snapshot path is invalid")]
    PathInvalid,
    #[error("truth evidence index is invalid")]
    InvalidEvidence,
    #[error("truth snapshot I/O failed during {operation}")]
    Io {
        operation: &'static str,
        kind: std::io::ErrorKind,
        #[source]
        source: std::io::Error,
    },
    #[error("truth snapshot JSON is invalid")]
    Json(#[source] serde_json::Error),
}

pub trait TruthSnapshotRepository: Send + Sync {
    fn open_current(
        &self,
        pack: &LoadedGamePack,
    ) -> Result<Option<VerifiedTruthSnapshot>, TruthStoreError>;
}

#[derive(Debug, Clone)]
pub struct VerifiedGameContext {
    pack: LoadedGamePack,
    snapshot: VerifiedTruthSnapshot,
}

impl VerifiedGameContext {
    pub fn open_current<R>(
        registry: &GamePackRegistry,
        repository: &R,
        game_pack_id: &GamePackId,
    ) -> Result<Self, VerifiedGameContextError>
    where
        R: TruthSnapshotRepository,
    {
        let pack = registry.require(game_pack_id)?.clone();
        let snapshot = repository
            .open_current(&pack)?
            .ok_or(VerifiedGameContextError::MissingCurrent)?;
        Ok(Self { pack, snapshot })
    }

    #[must_use]
    pub fn pack(&self) -> &LoadedGamePack {
        &self.pack
    }

    #[must_use]
    pub fn snapshot(&self) -> &VerifiedTruthSnapshot {
        &self.snapshot
    }
}

#[derive(Debug, Error)]
pub enum VerifiedGameContextError {
    #[error(transparent)]
    Registry(#[from] GamePackRegistryError),
    #[error(transparent)]
    Truth(#[from] TruthStoreError),
    #[error("game pack has no verified current truth snapshot")]
    MissingCurrent,
}

pub fn normalize_relative_path(path: &Path) -> Result<String, TruthStoreError> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(TruthStoreError::PathInvalid);
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let part = part.to_str().ok_or(TruthStoreError::PathInvalid)?;
                if part.is_empty() || part.contains('/') || part.contains('\\') {
                    return Err(TruthStoreError::PathInvalid);
                }
                parts.push(part);
            }
            _ => return Err(TruthStoreError::PathInvalid),
        }
    }
    if parts.is_empty() {
        return Err(TruthStoreError::PathInvalid);
    }
    Ok(parts.join("/"))
}

fn valid_local_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn sha256_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter always returns a valid digest")
}

#[cfg(test)]
mod tests {
    use ats_kernel::{SchemaId, SchemaRef, SchemaVersion};

    use crate::{GamePackLoader, GamePackRegistry};

    use super::*;

    fn pack() -> LoadedGamePack {
        let json = br#"{"schemaVersion":3,"id":"fixture-game","displayName":"Fixture","itemTypes":[{"id":"fixture_item","displayNames":{"eng":"Fixture item"},"evidenceQueries":[{"symbols":["Player.StartTurn"],"terms":[]}]}],"contributions":[]}"#;
        GamePackLoader::load(json, &sha256_bytes(json)).unwrap()
    }

    fn verified_snapshot(pack: &LoadedGamePack) -> VerifiedTruthSnapshot {
        let records = vec![TruthEvidenceRecord {
            source_id: "game".into(),
            symbol: "Player.StartTurn".into(),
            purpose: "turn lifecycle".into(),
            bounded_excerpt: "StartTurn calls ResetEnergy before drawing cards".into(),
            relative_path: "indexes/game/Player.cs".into(),
        }];
        let index_bytes = serde_json::to_vec(&records).unwrap();
        let manifest = TruthSnapshotManifest::new(
            pack,
            vec![TruthSnapshotSource {
                id: "game".into(),
                kind: "local_file".into(),
                version: Some("1".into()),
                relative_path: "sources/game.dll".into(),
                sha256: sha256_bytes(b"game"),
                byte_length: 4,
            }],
            vec![TruthSnapshotIndex {
                id: "game-code".into(),
                provider: PrimitiveId::parse("truth.code-facts").unwrap(),
                relative_path: "indexes/game.json".into(),
                sha256: sha256_bytes(&index_bytes),
                record_count: 1,
            }],
            BTreeMap::from([("indexer".into(), "1".into())]),
            Utc::now(),
        )
        .unwrap();
        VerifiedTruthSnapshot::verify(
            pack,
            manifest,
            BTreeMap::from([("game-code".into(), records)]),
        )
        .unwrap()
    }

    #[test]
    fn manifest_identity_and_evidence_query_are_deterministic() {
        let pack = pack();
        let snapshot = verified_snapshot(&pack);
        snapshot.manifest.verify_for_pack(&pack).unwrap();
        let results = snapshot
            .query(&EvidenceQuery {
                symbols: vec!["startturn".into()],
                terms: vec!["energy".into()],
                limit: 5,
            })
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_id, "game");
    }

    #[test]
    fn tampered_identity_evidence_and_query_are_rejected() {
        let pack = pack();
        let snapshot = verified_snapshot(&pack);
        let mut value = serde_json::to_value(snapshot.manifest()).unwrap();
        value["toolVersions"]["indexer"] = serde_json::json!("2");
        let tampered: TruthSnapshotManifest = serde_json::from_value(value).unwrap();
        assert!(matches!(
            tampered.verify_for_pack(&pack),
            Err(TruthStoreError::SnapshotIdentityMismatch)
        ));
        assert_eq!(
            snapshot.query(&EvidenceQuery {
                symbols: vec![],
                terms: vec![],
                limit: 0,
            }),
            Err(EvidenceQueryError::InvalidQuery)
        );
    }

    struct FixtureRepository(Option<VerifiedTruthSnapshot>);

    impl TruthSnapshotRepository for FixtureRepository {
        fn open_current(
            &self,
            _pack: &LoadedGamePack,
        ) -> Result<Option<VerifiedTruthSnapshot>, TruthStoreError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn verified_context_requires_registry_pack_and_current_snapshot() {
        let pack = pack();
        let id = pack.id().clone();
        let snapshot = verified_snapshot(&pack);
        let mut registry = GamePackRegistry::new();
        registry.register(pack).unwrap();
        let context =
            VerifiedGameContext::open_current(&registry, &FixtureRepository(Some(snapshot)), &id)
                .unwrap();
        assert_eq!(context.pack().id(), &id);
        assert!(matches!(
            VerifiedGameContext::open_current(&registry, &FixtureRepository(None), &id),
            Err(VerifiedGameContextError::MissingCurrent)
        ));
    }

    #[test]
    fn schema_ref_fixture_remains_qualified() {
        let schema = SchemaRef {
            id: SchemaId::parse("truth.snapshot").unwrap(),
            version: SchemaVersion::new(2).unwrap(),
        };
        assert_eq!(schema.version.get(), 2);
    }
}
