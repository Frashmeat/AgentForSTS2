//! Streaming SHA-256 helpers for source files and immutable index trees.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::error::{TruthSnapshotError, TruthSnapshotResult};

const BUFFER_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileDigest {
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TreeDigest {
    pub sha256: String,
    pub file_count: u32,
    pub cs_file_count: u32,
    pub total_bytes: u64,
}

pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn digest_file(path: &Path) -> TruthSnapshotResult<FileDigest> {
    let file = File::open(path).map_err(|source| TruthSnapshotError::Io {
        action: "open file",
        path: path.to_path_buf(),
        source,
    })?;
    digest_reader(BufReader::new(file), path)
}

pub(super) fn copy_and_digest(
    source: &Path,
    destination: &Path,
) -> TruthSnapshotResult<FileDigest> {
    if !source.is_file() {
        return Err(TruthSnapshotError::SourceNotFile(source.to_path_buf()));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|source| TruthSnapshotError::Io {
            action: "create source staging directory",
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let input = File::open(source).map_err(|source_error| TruthSnapshotError::Io {
        action: "open truth source",
        path: source.to_path_buf(),
        source: source_error,
    })?;
    let output = File::create(destination).map_err(|source_error| TruthSnapshotError::Io {
        action: "create staged truth source",
        path: destination.to_path_buf(),
        source: source_error,
    })?;
    let mut reader = BufReader::new(input);
    let mut writer = BufWriter::new(output);
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|source_error| TruthSnapshotError::Io {
                action: "read truth source",
                path: source.to_path_buf(),
                source: source_error,
            })?;
        if count == 0 {
            break;
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|source_error| TruthSnapshotError::Io {
                action: "write staged truth source",
                path: destination.to_path_buf(),
                source: source_error,
            })?;
        hasher.update(&buffer[..count]);
        total += count as u64;
    }
    writer
        .flush()
        .map_err(|source_error| TruthSnapshotError::Io {
            action: "flush staged truth source",
            path: destination.to_path_buf(),
            source: source_error,
        })?;
    writer
        .get_ref()
        .sync_all()
        .map_err(|source_error| TruthSnapshotError::Io {
            action: "sync staged truth source",
            path: destination.to_path_buf(),
            source: source_error,
        })?;
    Ok(FileDigest {
        sha256: format!("{:x}", hasher.finalize()),
        size_bytes: total,
    })
}

fn digest_reader(mut reader: impl Read, path: &Path) -> TruthSnapshotResult<FileDigest> {
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|source| TruthSnapshotError::Io {
                action: "read file",
                path: path.to_path_buf(),
                source,
            })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        total += count as u64;
    }
    Ok(FileDigest {
        sha256: format!("{:x}", hasher.finalize()),
        size_bytes: total,
    })
}

pub(super) fn digest_tree(root: &Path) -> TruthSnapshotResult<TreeDigest> {
    let root_meta = fs::symlink_metadata(root).map_err(|source| TruthSnapshotError::Io {
        action: "inspect truth index root",
        path: root.to_path_buf(),
        source,
    })?;
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Err(TruthSnapshotError::InvalidIndexEntry {
            path: root.to_path_buf(),
            reason: "index root must be a real directory".into(),
        });
    }

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| TruthSnapshotError::InvalidIndexEntry {
            path: error.path().unwrap_or(root).to_path_buf(),
            reason: error.to_string(),
        })?;
        if entry.path() == root {
            continue;
        }
        let file_type = entry.file_type();
        if file_type.is_symlink() {
            return Err(TruthSnapshotError::InvalidIndexEntry {
                path: entry.path().to_path_buf(),
                reason: "symbolic links are not allowed".into(),
            });
        }
        if file_type.is_dir() {
            continue;
        }
        if !file_type.is_file() {
            return Err(TruthSnapshotError::InvalidIndexEntry {
                path: entry.path().to_path_buf(),
                reason: "only regular files are allowed".into(),
            });
        }
        let relative =
            entry
                .path()
                .strip_prefix(root)
                .map_err(|_| TruthSnapshotError::PathOutsideRoot {
                    root: root.to_path_buf(),
                    relative: entry.path().to_path_buf(),
                })?;
        let relative = relative
            .to_str()
            .ok_or_else(|| TruthSnapshotError::InvalidIndexEntry {
                path: entry.path().to_path_buf(),
                reason: "index paths must be valid UTF-8".into(),
            })?
            .replace('\\', "/");
        files.push((relative, entry.path().to_path_buf()));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut tree_hasher = Sha256::new();
    let mut total_bytes = 0_u64;
    let mut cs_file_count = 0_u32;
    for (relative, path) in &files {
        let digest = digest_file(path)?;
        tree_hasher.update((relative.len() as u64).to_be_bytes());
        tree_hasher.update(relative.as_bytes());
        tree_hasher.update(digest.size_bytes.to_be_bytes());
        tree_hasher.update(digest.sha256.as_bytes());
        total_bytes += digest.size_bytes;
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("cs"))
        {
            cs_file_count += 1;
        }
    }

    Ok(TreeDigest {
        sha256: format!("{:x}", tree_hasher.finalize()),
        file_count: u32::try_from(files.len()).map_err(|_| {
            TruthSnapshotError::InvalidIndexEntry {
                path: root.to_path_buf(),
                reason: "index contains more than u32::MAX files".into(),
            }
        })?,
        cs_file_count,
        total_bytes,
    })
}
