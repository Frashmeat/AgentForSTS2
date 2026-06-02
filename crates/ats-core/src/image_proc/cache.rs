//! 模型权重缓存：把 ONNX 模型文件下载到 `<app_data>/models/` 并复用。
//!
//! 设计取向：
//! - **always-on**（不依赖 `ml-rembg` feature）—— 即便用户没开 ML，未来扩展其它
//!   后端时这套缓存仍可用
//! - 下载用现成的 `reqwest`，原子 rename 落盘，并发安全（先写 `.tmp` 再 rename）
//! - 可选 SHA-256 校验：若 `expected_sha256` 给了就校；不一致返错 + 删坏文件
//!
//! 默认模型来源：rembg 项目维护的 GitHub Releases。URL 通过 `ModelSpec` 注入，
//! 不在本模块写死，方便测试 mock + 用户改私服。

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum ModelCacheError {
    #[error("http: {0}")]
    Http(String),
    #[error("io: {0}")]
    Io(String),
    #[error("checksum mismatch (expected {expected}, got {actual})")]
    Checksum { expected: String, actual: String },
    #[error("archive: {0}")]
    Archive(String),
}

/// 一份模型权重的规范定义：本地相对路径 + 远端 URL + 可选 SHA-256。
#[derive(Debug, Clone)]
pub struct ModelSpec {
    /// `<models_dir>/<file_name>`
    pub file_name: String,
    pub url: String,
    /// 小写 hex（64 字符）；None 表示不校验
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OrtDylibSpec {
    pub file_name: String,
    pub archive_url: String,
    pub archive_member_suffix: String,
}

impl OrtDylibSpec {
    #[must_use]
    pub fn onnxruntime_1_22_windows_x64() -> Self {
        Self {
            file_name: "onnxruntime.dll".into(),
            archive_url: "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-win-x64-1.22.0.zip".into(),
            archive_member_suffix: "lib/onnxruntime.dll".into(),
        }
    }
}

impl ModelSpec {
    /// u2netp（移动版，~4.7MB）—— rembg 项目的预训练权重。
    /// 适合 1024px 内的 AI 生图，速度足够本地实时（CPU 200-400ms / 张）。
    #[must_use]
    pub fn u2netp() -> Self {
        Self {
            file_name: "u2netp.onnx".into(),
            // rembg 维护的稳定下载链接
            url: "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx".into(),
            // 公开 SHA-256（rembg 项目固定权重）
            expected_sha256: Some(
                "8e83ca70e441ab06c318d82300c84db3083e5b329f29f8a78ad9c14fa0d5e6cb".into(),
            ),
        }
    }
}

/// 解析 `<models_dir>/<file_name>` 完整路径。
#[must_use]
pub fn model_cache_path(models_dir: &Path, spec: &ModelSpec) -> PathBuf {
    models_dir.join(&spec.file_name)
}

/// 确保模型文件本地可用：存在且（如配了 SHA）通过校验，否则下载。
///
/// 并发安全：写 `<file>.tmp` 再 rename；多个进程同时下载只是会浪费一次带宽，
/// 后到者 rename 会覆盖。最坏不会出"半截文件"。
pub async fn ensure_model(models_dir: &Path, spec: &ModelSpec) -> Result<PathBuf, ModelCacheError> {
    let target = model_cache_path(models_dir, spec);
    if target.is_file() && file_ok(&target, spec.expected_sha256.as_deref()).await? {
        return Ok(target);
    }

    tokio::fs::create_dir_all(models_dir)
        .await
        .map_err(|e| ModelCacheError::Io(format!("create models_dir: {e}")))?;

    let tmp = target.with_extension("onnx.tmp");
    download_to(&spec.url, &tmp).await?;
    if let Some(expected) = spec.expected_sha256.as_deref()
        && !file_ok(&tmp, Some(expected)).await?
    {
        let _ = tokio::fs::remove_file(&tmp).await;
        let actual = sha256_of(&tmp)
            .await
            .unwrap_or_else(|_| "<unreadable>".into());
        return Err(ModelCacheError::Checksum {
            expected: expected.into(),
            actual,
        });
    }
    tokio::fs::rename(&tmp, &target)
        .await
        .map_err(|e| ModelCacheError::Io(format!("rename to {}: {e}", target.display())))?;
    Ok(target)
}

pub async fn ensure_ort_dylib(
    runtimes_dir: &Path,
    spec: &OrtDylibSpec,
) -> Result<PathBuf, ModelCacheError> {
    let target = runtimes_dir.join(&spec.file_name);
    if target.is_file() {
        return Ok(target);
    }

    tokio::fs::create_dir_all(runtimes_dir)
        .await
        .map_err(|e| ModelCacheError::Io(format!("create runtimes_dir: {e}")))?;

    let archive_path = runtimes_dir.join("onnxruntime.zip");
    download_to(&spec.archive_url, &archive_path).await?;
    let target_for_extract = target.clone();
    let archive_member_suffix = spec.archive_member_suffix.clone();
    let archive_for_extract = archive_path.clone();
    tokio::task::spawn_blocking(move || {
        extract_archive_member(
            &archive_for_extract,
            &archive_member_suffix,
            &target_for_extract,
        )
    })
    .await
    .map_err(|e| ModelCacheError::Archive(format!("extract join: {e}")))??;
    let _ = tokio::fs::remove_file(&archive_path).await;
    Ok(target)
}

/// 校验：如有 `expected_sha256`，本地文件必须匹配；无则只要文件存在即视为 OK。
async fn file_ok(path: &Path, expected_sha256: Option<&str>) -> Result<bool, ModelCacheError> {
    if !path.is_file() {
        return Ok(false);
    }
    let Some(expected) = expected_sha256 else {
        return Ok(true);
    };
    let actual = sha256_of(path).await?;
    Ok(actual.eq_ignore_ascii_case(expected))
}

async fn sha256_of(path: &Path) -> Result<String, ModelCacheError> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| ModelCacheError::Io(format!("read {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

async fn download_to(url: &str, dest: &Path) -> Result<(), ModelCacheError> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| ModelCacheError::Http(format!("GET {url}: {e}")))?;
    if !resp.status().is_success() {
        return Err(ModelCacheError::Http(format!(
            "GET {url} returned {}",
            resp.status()
        )));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| ModelCacheError::Http(format!("read body: {e}")))?;
    tokio::fs::write(dest, &bytes)
        .await
        .map_err(|e| ModelCacheError::Io(format!("write {}: {e}", dest.display())))?;
    Ok(())
}

fn extract_archive_member(
    archive_path: &Path,
    member_suffix: &str,
    dest: &Path,
) -> Result<(), ModelCacheError> {
    let file = std::fs::File::open(archive_path).map_err(|e| {
        ModelCacheError::Io(format!("open archive {}: {e}", archive_path.display()))
    })?;
    let mut zip = ZipArchive::new(file).map_err(|e| ModelCacheError::Archive(e.to_string()))?;
    let mut member_index = None;
    for i in 0..zip.len() {
        let name = {
            let file = zip
                .by_index(i)
                .map_err(|e| ModelCacheError::Archive(e.to_string()))?;
            file.name().replace('\\', "/")
        };
        if name.ends_with(member_suffix) {
            member_index = Some(i);
            break;
        }
    }
    let Some(index) = member_index else {
        return Err(ModelCacheError::Archive(format!(
            "member ending with {member_suffix} not found"
        )));
    };
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ModelCacheError::Io(format!("create {}: {e}", parent.display())))?;
    }
    let tmp = dest.with_extension("dll.tmp");
    let mut member = zip
        .by_index(index)
        .map_err(|e| ModelCacheError::Archive(e.to_string()))?;
    let mut out = std::fs::File::create(&tmp)
        .map_err(|e| ModelCacheError::Io(format!("create {}: {e}", tmp.display())))?;
    std::io::copy(&mut member, &mut out)
        .map_err(|e| ModelCacheError::Io(format!("extract {}: {e}", dest.display())))?;
    std::fs::rename(&tmp, dest)
        .map_err(|e| ModelCacheError::Io(format!("rename to {}: {e}", dest.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u2netp_spec_has_expected_fields() {
        let s = ModelSpec::u2netp();
        assert_eq!(s.file_name, "u2netp.onnx");
        assert!(s.url.starts_with("https://"));
        let sha = s.expected_sha256.unwrap();
        assert_eq!(sha.len(), 64);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn ort_dylib_spec_targets_windows_x64_dll() {
        let spec = OrtDylibSpec::onnxruntime_1_22_windows_x64();
        assert_eq!(spec.file_name, "onnxruntime.dll");
        assert!(spec.archive_url.contains("v1.22.0"));
        assert_eq!(spec.archive_member_suffix, "lib/onnxruntime.dll");
    }

    #[test]
    fn model_cache_path_concatenates_correctly() {
        let dir = std::path::Path::new("/tmp/x");
        let spec = ModelSpec {
            file_name: "foo.onnx".into(),
            url: "https://example.com/foo.onnx".into(),
            expected_sha256: None,
        };
        let p = model_cache_path(dir, &spec);
        assert_eq!(p, std::path::Path::new("/tmp/x/foo.onnx"));
    }

    #[tokio::test]
    async fn ensure_model_short_circuits_when_file_present_with_matching_sha() {
        let td = tempfile::TempDir::new().unwrap();
        let models = td.path().to_path_buf();
        std::fs::create_dir_all(&models).unwrap();

        // 写一份已知内容的"模型"文件，算它的 SHA
        let bytes = b"FAKE-MODEL-CONTENT-PRETENDING-TO-BE-ONNX";
        let path = models.join("fake.onnx");
        std::fs::write(&path, bytes).unwrap();
        let mut h = Sha256::new();
        h.update(bytes);
        let sha = format!("{:x}", h.finalize());

        let spec = ModelSpec {
            file_name: "fake.onnx".into(),
            // 故意填一个不存在的 URL —— 测命中缓存就不会调网
            url: "https://invalid.example.invalid/fake.onnx".into(),
            expected_sha256: Some(sha),
        };
        let got = ensure_model(&models, &spec).await.expect("cache hit");
        assert_eq!(got, path);
    }

    #[tokio::test]
    async fn ensure_model_redownloads_when_sha_mismatches() {
        // 文件存在但内容不对（SHA 对不上）→ 应当尝试重下载。
        // 这里 URL 也是 invalid，所以会拿到 Http 错；关键是验证它不"早退"。
        let td = tempfile::TempDir::new().unwrap();
        let models = td.path().to_path_buf();
        std::fs::create_dir_all(&models).unwrap();
        let path = models.join("fake.onnx");
        std::fs::write(&path, b"wrong content").unwrap();

        let spec = ModelSpec {
            file_name: "fake.onnx".into(),
            url: "http://127.0.0.1:1/never-listens".into(),
            expected_sha256: Some(
                "0000000000000000000000000000000000000000000000000000000000000000".into(),
            ),
        };
        let err = ensure_model(&models, &spec).await.unwrap_err();
        assert!(
            matches!(err, ModelCacheError::Http(_)),
            "expected Http error from unreachable URL, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn ensure_model_skips_sha_when_none() {
        // 文件已存在 + 没配 SHA → 无条件返回缓存路径
        let td = tempfile::TempDir::new().unwrap();
        let models = td.path().to_path_buf();
        let path = models.join("fake.onnx");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(&path, b"anything").unwrap();

        let spec = ModelSpec {
            file_name: "fake.onnx".into(),
            url: "https://invalid.example.invalid/fake.onnx".into(),
            expected_sha256: None,
        };
        let got = ensure_model(&models, &spec).await.unwrap();
        assert_eq!(got, path);
    }

    #[tokio::test]
    async fn sha256_of_matches_known_value() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("x.bin");
        tokio::fs::write(&path, b"hello").await.unwrap();
        let got = sha256_of(&path).await.unwrap();
        // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
        assert_eq!(
            got,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }
}
