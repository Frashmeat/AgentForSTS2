//! 知识包 ZIP 导出 / 导入。
//!
//! 让用户把反编译好的 game/baselib + manifest 整体打包分享给其他用户/机器，
//! 避免每台机都跑一遍 ilspycmd。
//!
//! ZIP 内部结构：
//! ```text
//! game/                              # paths.game_dir 内容
//! baselib/BaseLib.decompiled.cs      # 若存在
//! knowledge-manifest.json            # paths.manifest_path
//! pack-info.json                     # 元信息（schema_version / exported_at）
//! ```
//!
//! 同步实现（IO 较快，不进 Job 框架）。调用方在 Tauri command 里用
//! `spawn_blocking` 包住即可。

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use super::paths::KnowledgePaths;

const PACK_INFO_NAME: &str = "pack-info.json";
const PACK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInfo {
    pub schema_version: u32,
    pub exported_at: DateTime<Utc>,
    pub source_machine_hint: Option<String>,
    pub game_file_count: u32,
    pub baselib_present: bool,
}

#[derive(Debug, Error)]
pub enum PackError {
    #[error("io: {0}")]
    Io(String),
    #[error("zip: {0}")]
    Zip(String),
    #[error("knowledge_root missing or empty: {0}")]
    EmptyKnowledge(PathBuf),
    #[error("input zip missing pack-info.json")]
    NotKnowledgePack,
    #[error("schema version mismatch: pack v{got}, supported v{expected}")]
    SchemaMismatch { got: u32, expected: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStats {
    pub output_path: PathBuf,
    pub zip_bytes: u64,
    pub game_files: u32,
    pub baselib_included: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStats {
    pub game_files_written: u32,
    pub baselib_written: bool,
    pub manifest_replaced: bool,
}

/// 把 paths 指向的知识库内容打包到 output_zip。
///
/// # Errors
/// - `EmptyKnowledge`：game_dir 不存在且无 baselib_decompiled.cs（无可打包内容）
/// - `Io`/`Zip`：底层错误
pub fn export(
    paths: &KnowledgePaths,
    output_zip: &Path,
    machine_hint: Option<String>,
) -> Result<ExportStats, PackError> {
    let game_exists = paths.game_dir.is_dir() && has_any_file(&paths.game_dir);
    let baselib_file = paths.baselib_decompiled_file();
    let baselib_exists = baselib_file.is_file();
    if !game_exists && !baselib_exists {
        return Err(PackError::EmptyKnowledge(paths.root.clone()));
    }

    if let Some(parent) = output_zip.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| PackError::Io(e.to_string()))?;
    }
    let file = File::create(output_zip).map_err(|e| PackError::Io(e.to_string()))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    let mut buffer = Vec::with_capacity(8192);
    let mut game_files: u32 = 0;

    // game 目录
    if game_exists {
        for entry in walkdir::WalkDir::new(&paths.game_dir)
            .into_iter()
            .filter_map(Result::ok)
        {
            let p = entry.path();
            let rel = match p.strip_prefix(&paths.game_dir) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if rel.as_os_str().is_empty() {
                continue;
            }
            let rel_str = rel
                .to_str()
                .ok_or_else(|| PackError::Zip(format!("non-utf8 path: {}", rel.display())))?
                .replace('\\', "/");
            if entry.file_type().is_dir() {
                writer
                    .add_directory(format!("game/{rel_str}/"), options)
                    .map_err(|e| PackError::Zip(e.to_string()))?;
            } else if entry.file_type().is_file() {
                writer
                    .start_file(format!("game/{rel_str}"), options)
                    .map_err(|e| PackError::Zip(e.to_string()))?;
                buffer.clear();
                File::open(p)
                    .and_then(|mut f| f.read_to_end(&mut buffer))
                    .map_err(|e| PackError::Io(e.to_string()))?;
                writer
                    .write_all(&buffer)
                    .map_err(|e| PackError::Io(e.to_string()))?;
                game_files += 1;
            }
        }
    }

    // baselib 单文件
    if baselib_exists {
        writer
            .start_file("baselib/BaseLib.decompiled.cs", options)
            .map_err(|e| PackError::Zip(e.to_string()))?;
        let mut f = File::open(&baselib_file).map_err(|e| PackError::Io(e.to_string()))?;
        buffer.clear();
        f.read_to_end(&mut buffer)
            .map_err(|e| PackError::Io(e.to_string()))?;
        writer
            .write_all(&buffer)
            .map_err(|e| PackError::Io(e.to_string()))?;
    }

    // manifest（若存在）
    if paths.manifest_path.is_file() {
        writer
            .start_file("knowledge-manifest.json", options)
            .map_err(|e| PackError::Zip(e.to_string()))?;
        let bytes = std::fs::read(&paths.manifest_path).map_err(|e| PackError::Io(e.to_string()))?;
        writer
            .write_all(&bytes)
            .map_err(|e| PackError::Io(e.to_string()))?;
    }

    // pack-info.json
    let info = PackInfo {
        schema_version: PACK_SCHEMA_VERSION,
        exported_at: Utc::now(),
        source_machine_hint: machine_hint,
        game_file_count: game_files,
        baselib_present: baselib_exists,
    };
    let info_json = serde_json::to_vec_pretty(&info).map_err(|e| PackError::Zip(e.to_string()))?;
    writer
        .start_file(PACK_INFO_NAME, options)
        .map_err(|e| PackError::Zip(e.to_string()))?;
    writer
        .write_all(&info_json)
        .map_err(|e| PackError::Io(e.to_string()))?;

    let final_file = writer.finish().map_err(|e| PackError::Zip(e.to_string()))?;
    let zip_bytes = final_file.metadata().map(|m| m.len()).unwrap_or(0);

    Ok(ExportStats {
        output_path: output_zip.to_path_buf(),
        zip_bytes,
        game_files,
        baselib_included: baselib_exists,
    })
}

/// 从 input_zip 导入到 paths。
///
/// `overwrite=true` 时直接覆盖现有 game_dir 内容（不删除其它文件，只覆盖同名）+
/// baselib + manifest。`overwrite=false` 时若 game_dir 已含文件或 baselib 已存在则
/// 报错 `EmptyKnowledge` 风格的友好提示。
///
/// # Errors
/// - `NotKnowledgePack`：zip 内无 pack-info.json（不是合法知识包）
/// - `SchemaMismatch`：pack_info.schema_version != PACK_SCHEMA_VERSION
/// - `Io`/`Zip`：底层错误
pub fn import(
    paths: &KnowledgePaths,
    input_zip: &Path,
    overwrite: bool,
) -> Result<ImportStats, PackError> {
    let file = File::open(input_zip).map_err(|e| PackError::Io(e.to_string()))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| PackError::Zip(e.to_string()))?;

    // 先读 pack-info 验证
    let info: PackInfo = {
        let mut entry = archive
            .by_name(PACK_INFO_NAME)
            .map_err(|_| PackError::NotKnowledgePack)?;
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| PackError::Io(e.to_string()))?;
        serde_json::from_slice(&buf).map_err(|e| PackError::Zip(format!("pack-info: {e}")))?
    };
    if info.schema_version != PACK_SCHEMA_VERSION {
        return Err(PackError::SchemaMismatch {
            got: info.schema_version,
            expected: PACK_SCHEMA_VERSION,
        });
    }

    if !overwrite && (has_any_file(&paths.game_dir) || paths.baselib_decompiled_file().is_file()) {
        return Err(PackError::Zip(
            "knowledge already populated; pass overwrite=true to replace".into(),
        ));
    }

    std::fs::create_dir_all(&paths.game_dir).map_err(|e| PackError::Io(e.to_string()))?;
    std::fs::create_dir_all(&paths.baselib_dir).map_err(|e| PackError::Io(e.to_string()))?;
    if let Some(parent) = paths.manifest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PackError::Io(e.to_string()))?;
    }

    let mut stats = ImportStats {
        game_files_written: 0,
        baselib_written: false,
        manifest_replaced: false,
    };

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| PackError::Zip(e.to_string()))?;
        let name = entry.name().to_string();
        if name == PACK_INFO_NAME {
            continue;
        }
        if entry.is_dir() {
            continue;
        }
        let target = if let Some(rest) = name.strip_prefix("game/") {
            paths.game_dir.join(rest)
        } else if name == "baselib/BaseLib.decompiled.cs" {
            paths.baselib_decompiled_file()
        } else if name == "knowledge-manifest.json" {
            paths.manifest_path.clone()
        } else {
            // 未知条目跳过（向前兼容未来 schema 扩展）
            continue;
        };
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PackError::Io(e.to_string()))?;
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| PackError::Io(e.to_string()))?;
        std::fs::write(&target, &buf).map_err(|e| PackError::Io(e.to_string()))?;

        if name.starts_with("game/") {
            stats.game_files_written += 1;
        } else if name == "baselib/BaseLib.decompiled.cs" {
            stats.baselib_written = true;
        } else if name == "knowledge-manifest.json" {
            stats.manifest_replaced = true;
        }
    }

    Ok(stats)
}

fn has_any_file(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    walkdir::WalkDir::new(dir)
        .max_depth(8)
        .into_iter()
        .filter_map(Result::ok)
        .any(|e| e.file_type().is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_paths(td: &tempfile::TempDir) -> KnowledgePaths {
        let paths = KnowledgePaths::from_runtime_dir(td.path());
        super::super::runtime::ensure_dirs(&paths).unwrap();
        paths
    }

    fn populate_knowledge(paths: &KnowledgePaths) {
        fs::create_dir_all(paths.game_dir.join("Cards")).unwrap();
        fs::write(paths.game_dir.join("a.cs"), b"// a").unwrap();
        fs::write(paths.game_dir.join("Cards/b.cs"), b"// b").unwrap();
        fs::write(paths.baselib_decompiled_file(), b"// baselib").unwrap();
        fs::write(&paths.manifest_path, br#"{"schemaVersion":1}"#).unwrap();
    }

    #[test]
    fn export_then_import_round_trips() {
        let src = tempfile::TempDir::new().unwrap();
        let src_paths = make_paths(&src);
        populate_knowledge(&src_paths);

        let zip_path = src.path().join("knowledge.zip");
        let stats = export(&src_paths, &zip_path, Some("dev-pc".into())).unwrap();
        assert_eq!(stats.game_files, 2);
        assert!(stats.baselib_included);
        assert!(zip_path.exists());
        assert!(stats.zip_bytes > 0);

        // 导入到全新目录
        let dst = tempfile::TempDir::new().unwrap();
        let dst_paths = make_paths(&dst);
        let imp = import(&dst_paths, &zip_path, false).unwrap();
        assert_eq!(imp.game_files_written, 2);
        assert!(imp.baselib_written);
        assert!(imp.manifest_replaced);

        // 验证文件确实落地
        assert!(dst_paths.game_dir.join("a.cs").exists());
        assert!(dst_paths.game_dir.join("Cards/b.cs").exists());
        assert!(dst_paths.baselib_decompiled_file().exists());
        let manifest = fs::read_to_string(&dst_paths.manifest_path).unwrap();
        assert!(manifest.contains("schemaVersion"));
    }

    #[test]
    fn export_fails_when_knowledge_empty() {
        let td = tempfile::TempDir::new().unwrap();
        let paths = make_paths(&td);
        let out = td.path().join("x.zip");
        let err = export(&paths, &out, None).unwrap_err();
        assert!(matches!(err, PackError::EmptyKnowledge(_)));
    }

    #[test]
    fn import_refuses_when_populated_and_no_overwrite() {
        // 准备一个 pack
        let src = tempfile::TempDir::new().unwrap();
        let src_paths = make_paths(&src);
        populate_knowledge(&src_paths);
        let zip = src.path().join("k.zip");
        export(&src_paths, &zip, None).unwrap();

        // 目标已含文件
        let dst = tempfile::TempDir::new().unwrap();
        let dst_paths = make_paths(&dst);
        fs::write(dst_paths.game_dir.join("existing.cs"), b"// keep").unwrap();

        let err = import(&dst_paths, &zip, false).unwrap_err();
        assert!(matches!(err, PackError::Zip(msg) if msg.contains("overwrite")));

        // overwrite=true 应当成功
        let stats = import(&dst_paths, &zip, true).unwrap();
        assert_eq!(stats.game_files_written, 2);
    }

    #[test]
    fn import_rejects_non_pack_zip() {
        let td = tempfile::TempDir::new().unwrap();
        let bad_zip = td.path().join("plain.zip");
        // 一个不含 pack-info.json 的合法 zip
        {
            let f = File::create(&bad_zip).unwrap();
            let mut w = zip::ZipWriter::new(f);
            w.start_file("hello.txt", SimpleFileOptions::default()).unwrap();
            w.write_all(b"hi").unwrap();
            w.finish().unwrap();
        }
        let paths = make_paths(&td);
        let err = import(&paths, &bad_zip, true).unwrap_err();
        assert!(matches!(err, PackError::NotKnowledgePack));
    }

    #[test]
    fn import_rejects_schema_mismatch() {
        let td = tempfile::TempDir::new().unwrap();
        let bad_zip = td.path().join("v99.zip");
        {
            let f = File::create(&bad_zip).unwrap();
            let mut w = zip::ZipWriter::new(f);
            w.start_file("pack-info.json", SimpleFileOptions::default()).unwrap();
            w.write_all(br#"{"schemaVersion":99,"exportedAt":"2026-05-11T00:00:00Z","gameFileCount":0,"baselibPresent":false}"#).unwrap();
            w.finish().unwrap();
        }
        let paths = make_paths(&td);
        let err = import(&paths, &bad_zip, true).unwrap_err();
        assert!(
            matches!(err, PackError::SchemaMismatch { got: 99, expected: 1 }),
            "got: {err:?}"
        );
    }
}
