//! Knowledge manifest 持久化：记录上次反编译的输入 DLL / 时间 / 产出文件计数。
//!
//! 调用方（KnowledgeRefreshHandler）用 manifest 判断是否需要重新反编译：
//! - DLL 路径变了 → refresh
//! - DLL size / mtime 变了 → refresh
//! - 显式 force=true → refresh
//!
//! Schema v1 当前只覆盖 game（sts2.dll）；baselib 字段预留给后续 stage（GitHub
//! Releases 接入后填充）。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeManifest {
    pub schema_version: u32,
    #[serde(default)]
    pub game: Option<DecompileRecord>,
    /// 预留给 BaseLib 反编译记录；本 stage 始终为 None。
    #[serde(default)]
    pub baselib: Option<DecompileRecord>,
}

impl Default for KnowledgeManifest {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            game: None,
            baselib: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecompileRecord {
    pub source_path: PathBuf,
    pub source_size_bytes: u64,
    pub source_mtime: Option<DateTime<Utc>>,
    pub decompiled_at: DateTime<Utc>,
    pub cs_file_count: u32,
    pub total_bytes: u64,
}

impl DecompileRecord {
    /// 当前 source 是否与 record 一致（路径 + 大小 + mtime）。
    /// path 不一致直接 stale；大小 / mtime 任一变化也算 stale。
    #[must_use]
    pub fn matches_current_source(&self, source: &Path) -> bool {
        if self.source_path != source {
            return false;
        }
        let Ok(meta) = std::fs::metadata(source) else {
            return false;
        };
        if meta.len() != self.source_size_bytes {
            return false;
        }
        let Ok(mtime) = meta.modified() else {
            return self.source_mtime.is_none();
        };
        match self.source_mtime {
            Some(recorded) => approx_eq_system_time(mtime, recorded),
            None => false,
        }
    }
}

/// 从源文件元数据构造一条 DecompileRecord。
///
/// # Errors
/// 读取 metadata 失败时返回 io 错误。
pub fn build_record(
    source: &Path,
    cs_file_count: u32,
    total_bytes: u64,
) -> std::io::Result<DecompileRecord> {
    let meta = std::fs::metadata(source)?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| Some(DateTime::<Utc>::from(t)));
    Ok(DecompileRecord {
        source_path: source.to_path_buf(),
        source_size_bytes: meta.len(),
        source_mtime: mtime,
        decompiled_at: Utc::now(),
        cs_file_count,
        total_bytes,
    })
}

/// 从指定路径读取 manifest。
///
/// 文件不存在 → Ok(None)；存在但解析失败 → Err。
///
/// # Errors
/// io 读错误或 JSON 解析错误。
pub fn read_manifest(path: &Path) -> std::io::Result<Option<KnowledgeManifest>> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let m: KnowledgeManifest = serde_json::from_str(&text).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
            })?;
            Ok(Some(m))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// 把 manifest 原子写到指定路径（tmp + rename）。
///
/// # Errors
/// io 写错误或 JSON 序列化错误。
pub fn write_manifest(path: &Path, manifest: &KnowledgeManifest) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(manifest)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// mtime 比较：允许 ±1 秒漂移（Windows FAT/NTFS 精度差异 + serde 序列化截断）。
fn approx_eq_system_time(a: SystemTime, b: DateTime<Utc>) -> bool {
    let a_dt: DateTime<Utc> = a.into();
    (a_dt.timestamp() - b.timestamp()).abs() <= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_manifest_has_correct_schema_version() {
        let m = KnowledgeManifest::default();
        assert_eq!(m.schema_version, SCHEMA_VERSION);
        assert!(m.game.is_none() && m.baselib.is_none());
    }

    #[test]
    fn read_manifest_returns_none_when_missing() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("manifest.json");
        let result = read_manifest(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn write_then_read_round_trips() {
        let td = tempfile::TempDir::new().unwrap();
        let src = td.path().join("fake.dll");
        std::fs::write(&src, b"MZ--placeholder").unwrap();

        let record = build_record(&src, 42, 12345).unwrap();
        let mut m = KnowledgeManifest::default();
        m.game = Some(record.clone());

        let manifest_path = td.path().join("knowledge-manifest.json");
        write_manifest(&manifest_path, &m).unwrap();

        let loaded = read_manifest(&manifest_path).unwrap().expect("file present");
        let g = loaded.game.expect("game record");
        assert_eq!(g.source_path, src);
        assert_eq!(g.cs_file_count, 42);
        assert_eq!(g.total_bytes, 12345);
        assert_eq!(g.source_size_bytes, record.source_size_bytes);
    }

    #[test]
    fn matches_current_source_detects_size_change() {
        let td = tempfile::TempDir::new().unwrap();
        let src = td.path().join("fake.dll");
        std::fs::write(&src, b"small").unwrap();

        let record = build_record(&src, 1, 1).unwrap();
        assert!(record.matches_current_source(&src));

        std::fs::write(&src, b"bigger now--").unwrap();
        assert!(
            !record.matches_current_source(&src),
            "size change should be detected"
        );
    }

    #[test]
    fn matches_current_source_returns_false_when_path_differs() {
        let td = tempfile::TempDir::new().unwrap();
        let src = td.path().join("a.dll");
        std::fs::write(&src, b"a").unwrap();
        let other = td.path().join("b.dll");
        std::fs::write(&other, b"a").unwrap();

        let record = build_record(&src, 1, 1).unwrap();
        assert!(!record.matches_current_source(&other));
    }

    #[test]
    fn matches_current_source_returns_false_when_source_missing() {
        let td = tempfile::TempDir::new().unwrap();
        let src = td.path().join("vanishing.dll");
        std::fs::write(&src, b"x").unwrap();
        let record = build_record(&src, 1, 1).unwrap();
        std::fs::remove_file(&src).unwrap();
        assert!(!record.matches_current_source(&src));
    }

    #[test]
    fn read_manifest_returns_err_on_invalid_json() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("bad.json");
        std::fs::write(&path, b"{not json").unwrap();
        let err = read_manifest(&path).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }
}
