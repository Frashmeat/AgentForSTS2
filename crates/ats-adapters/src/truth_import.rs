use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use ats_game_context::{
    LoadedGamePack, TruthEvidenceRecord, TruthSnapshotIndex, TruthSnapshotManifest,
    TruthSnapshotSource,
};
use ats_kernel::{PrimitiveId, Sha256Digest};
use ats_runtime::CancellationToken;
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use walkdir::WalkDir;

const V1_SCHEMA: u32 = 1;

#[derive(Debug, Error)]
pub enum Sts2TruthImportError {
    #[error("no verified Stage 1 Truth Snapshot is available")]
    Missing,
    #[error("Stage 1 Truth Snapshot is invalid")]
    Invalid,
    #[error("Truth import was cancelled")]
    Cancelled,
    #[error("Truth import I/O failed")]
    Io(#[from] io::Error),
    #[error("Truth import JSON is invalid")]
    Json(#[from] serde_json::Error),
}

pub struct Sts2TruthImporter;

impl Sts2TruthImporter {
    pub fn import_current(
        runtime_root: &Path,
        pack: &LoadedGamePack,
        cancellation: &CancellationToken,
    ) -> Result<TruthSnapshotManifest, Sts2TruthImportError> {
        if pack.id().as_str() != "sts2" {
            return Err(Sts2TruthImportError::Invalid);
        }
        check_cancelled(cancellation)?;
        let old_pack_root = runtime_root.join("game-packs/sts2");
        let pointer_bytes = fs::read(old_pack_root.join("current.json")).map_err(missing_or_io)?;
        let pointer: OldPointer = serde_json::from_slice(&pointer_bytes)?;
        if pointer.schema_version != V1_SCHEMA || !valid_digest(&pointer.snapshot_id) {
            return Err(Sts2TruthImportError::Invalid);
        }
        let old_snapshot = old_pack_root.join("snapshots").join(&pointer.snapshot_id);
        let old_manifest_bytes = read_regular(&old_snapshot.join("snapshot.json"))?;
        if sha256(&old_manifest_bytes).as_str() != pointer.manifest_sha256.to_ascii_lowercase() {
            return Err(Sts2TruthImportError::Invalid);
        }
        let old: OldManifest = serde_json::from_slice(&old_manifest_bytes)?;
        if old.schema_version != V1_SCHEMA
            || old.snapshot_id != pointer.snapshot_id
            || old.game_pack_id != "sts2"
            || old.game_pack_schema_version == 0
            || !valid_digest(&old.game_pack_sha256)
            || old.sources.is_empty()
            || old.indexes.is_empty()
        {
            return Err(Sts2TruthImportError::Invalid);
        }

        let destination_pack = runtime_root.join("truth/sts2");
        let staging = destination_pack
            .join(".staging")
            .join(format!("import-{}", std::process::id()));
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(staging.join("sources"))?;
        fs::create_dir_all(staging.join("indexes"))?;

        let result = (|| {
            let mut sources = Vec::with_capacity(old.sources.len());
            for source in &old.sources {
                check_cancelled(cancellation)?;
                safe_relative(&source.relative_path)?;
                let input = old_snapshot.join(&source.relative_path);
                let bytes = read_regular(&input)?;
                if u64::try_from(bytes.len()).ok() != Some(source.size_bytes)
                    || sha256(&bytes).as_str() != source.sha256.to_ascii_lowercase()
                {
                    return Err(Sts2TruthImportError::Invalid);
                }
                let relative = format!("sources/{}.bin", source.id);
                fs::write(staging.join(&relative), &bytes)?;
                sources.push(TruthSnapshotSource {
                    id: normalize_id(&source.id)?,
                    kind: normalize_id(&source.kind)?,
                    version: source.version.clone(),
                    relative_path: relative,
                    sha256: sha256(&bytes),
                    byte_length: source.size_bytes,
                });
            }

            let source_ids = sources
                .iter()
                .map(|source| (source.id.clone(), source.id.clone()))
                .collect::<BTreeMap<_, _>>();
            let mut indexes = Vec::with_capacity(old.indexes.len());
            for (position, index) in old.indexes.iter().enumerate() {
                check_cancelled(cancellation)?;
                safe_relative(&index.relative_root)?;
                if index.indexer.is_empty()
                    || index.provider.is_empty()
                    || index.file_count == 0
                    || index.cs_file_count == 0
                    || index.cs_file_count > index.file_count
                    || index.total_bytes == 0
                    || !valid_digest(&index.tree_sha256)
                {
                    return Err(Sts2TruthImportError::Invalid);
                }
                let source_id = normalize_id(&index.source_id)?;
                if !source_ids.contains_key(&source_id) {
                    return Err(Sts2TruthImportError::Invalid);
                }
                let records = index_directory(
                    &old_snapshot.join(&index.relative_root),
                    &source_id,
                    cancellation,
                )?;
                if records.is_empty() {
                    return Err(Sts2TruthImportError::Invalid);
                }
                let bytes = serde_json::to_vec_pretty(&records)?;
                let id = format!("source-{position}");
                let relative = format!("indexes/{id}.json");
                fs::write(staging.join(&relative), &bytes)?;
                indexes.push(TruthSnapshotIndex {
                    id,
                    provider: PrimitiveId::parse("truth.csharp-symbols")
                        .expect("built-in Primitive ID is valid"),
                    relative_path: relative,
                    sha256: sha256(&bytes),
                    record_count: u32::try_from(records.len())
                        .map_err(|_| Sts2TruthImportError::Invalid)?,
                });
            }

            let mut tool_versions = old.tool_versions.clone();
            tool_versions.insert("stage2-importer".into(), "1".into());
            let manifest =
                TruthSnapshotManifest::new(pack, sources, indexes, tool_versions, old.created_at)
                    .map_err(|_| Sts2TruthImportError::Invalid)?;
            fs::write(
                staging.join("truth-snapshot.json"),
                serde_json::to_vec_pretty(&manifest)?,
            )?;
            let snapshots = destination_pack.join("snapshots");
            fs::create_dir_all(&snapshots)?;
            let final_root = snapshots.join(manifest.snapshot_id().as_str());
            if final_root.exists() {
                fs::remove_dir_all(&staging)?;
            } else {
                fs::rename(&staging, &final_root)?;
            }
            write_current(&destination_pack, manifest.snapshot_id())?;
            Ok(manifest)
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        result
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OldPointer {
    schema_version: u32,
    snapshot_id: String,
    manifest_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OldManifest {
    schema_version: u32,
    snapshot_id: String,
    game_pack_id: String,
    game_pack_schema_version: u32,
    game_pack_sha256: String,
    sources: Vec<OldSource>,
    indexes: Vec<OldIndex>,
    tool_versions: BTreeMap<String, String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OldSource {
    id: String,
    kind: String,
    version: Option<String>,
    relative_path: String,
    sha256: String,
    size_bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OldIndex {
    source_id: String,
    indexer: String,
    provider: String,
    relative_root: String,
    file_count: u32,
    cs_file_count: u32,
    total_bytes: u64,
    tree_sha256: String,
}

fn index_directory(
    root: &Path,
    source_id: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<TruthEvidenceRecord>, Sts2TruthImportError> {
    if !plain_directory(root) {
        return Err(Sts2TruthImportError::Invalid);
    }
    let type_pattern = Regex::new(r"\b(?:class|struct|interface|enum)\s+([A-Za-z_][A-Za-z0-9_]*)")
        .expect("built-in regex is valid");
    let method_pattern = Regex::new(
        r"\b(?:public|protected|internal|private)\s+(?:static\s+)?[A-Za-z_][A-Za-z0-9_<>,?.\[\]]*\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
    )
    .expect("built-in regex is valid");
    let mut records = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).sort_by_file_name() {
        check_cancelled(cancellation)?;
        let entry = entry.map_err(|_| Sts2TruthImportError::Invalid)?;
        if entry.file_type().is_symlink() {
            return Err(Sts2TruthImportError::Invalid);
        }
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("cs")
        {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| Sts2TruthImportError::Invalid)?
            .to_string_lossy()
            .replace('\\', "/");
        let text = fs::read_to_string(entry.path()).map_err(|_| Sts2TruthImportError::Invalid)?;
        let mut current_type: Option<String> = None;
        for line in text.lines() {
            if let Some(captures) = type_pattern.captures(line) {
                let symbol = captures[1].to_owned();
                current_type = Some(symbol.clone());
                push_record(&mut records, source_id, symbol, line, &relative);
            }
            if let Some(captures) = method_pattern.captures(line) {
                let name = captures[1].to_owned();
                let symbol = current_type
                    .as_ref()
                    .map_or_else(|| name.clone(), |kind| format!("{kind}.{name}"));
                push_record(&mut records, source_id, symbol, line, &relative);
            }
            if records.len() >= 50_000 {
                return Err(Sts2TruthImportError::Invalid);
            }
        }
    }
    records.sort_by(|left, right| {
        (&left.symbol, &left.relative_path).cmp(&(&right.symbol, &right.relative_path))
    });
    records.dedup_by(|left, right| {
        left.symbol == right.symbol && left.relative_path == right.relative_path
    });
    Ok(records)
}

fn push_record(
    records: &mut Vec<TruthEvidenceRecord>,
    source_id: &str,
    symbol: String,
    line: &str,
    relative: &str,
) {
    let excerpt = line.trim();
    if symbol.len() <= 256 && !excerpt.is_empty() {
        records.push(TruthEvidenceRecord {
            source_id: source_id.into(),
            symbol,
            purpose: "C# declaration imported from the verified Stage 1 snapshot".into(),
            bounded_excerpt: excerpt.chars().take(2_000).collect(),
            relative_path: format!("evidence/{source_id}/{relative}"),
        });
    }
}

fn write_current(root: &Path, id: &Sha256Digest) -> Result<(), Sts2TruthImportError> {
    fs::create_dir_all(root)?;
    let temporary = root.join("current.json.ats-tmp");
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "snapshotId": id,
        }))?,
    )?;
    let current = root.join("current.json");
    if current.exists() {
        let backup = root.join("current.json.ats-backup");
        let _ = fs::remove_file(&backup);
        fs::rename(&current, &backup)?;
        if let Err(error) = fs::rename(&temporary, &current) {
            let _ = fs::rename(&backup, &current);
            return Err(error.into());
        }
        fs::remove_file(backup)?;
    } else {
        fs::rename(temporary, current)?;
    }
    Ok(())
}

fn read_regular(path: &Path) -> Result<Vec<u8>, Sts2TruthImportError> {
    let metadata = fs::symlink_metadata(path).map_err(missing_or_io)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(Sts2TruthImportError::Invalid);
    }
    Ok(fs::read(path)?)
}

fn plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn normalize_id(value: &str) -> Result<String, Sts2TruthImportError> {
    let value = value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let value = value.trim_matches('-').to_owned();
    if value.is_empty() || value.len() > 128 {
        Err(Sts2TruthImportError::Invalid)
    } else {
        Ok(value)
    }
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), Sts2TruthImportError> {
    if cancellation.is_cancelled() {
        Err(Sts2TruthImportError::Cancelled)
    } else {
        Ok(())
    }
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(format!("{:x}", Sha256::digest(bytes))).expect("SHA-256 formatter is valid")
}

fn valid_digest(value: &str) -> bool {
    Sha256Digest::parse(value).is_ok()
}

fn safe_relative(value: &str) -> Result<(), Sts2TruthImportError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        Err(Sts2TruthImportError::Invalid)
    } else {
        Ok(())
    }
}

fn missing_or_io(error: io::Error) -> Sts2TruthImportError {
    if error.kind() == io::ErrorKind::NotFound {
        Sts2TruthImportError::Missing
    } else {
        Sts2TruthImportError::Io(error)
    }
}
