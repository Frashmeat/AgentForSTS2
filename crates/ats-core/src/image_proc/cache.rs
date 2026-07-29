//! 模型权重缓存：把 ONNX 模型文件下载到 `<app_data>/models/` 并复用。
//!
//! 设计取向：
//! - **always-on**（不依赖 `ml-rembg` feature）—— 即便用户没开 ML，未来扩展其它
//!   后端时这套缓存仍可用
//! - 下载用现成的 `reqwest` 流式落盘，连接中断后从已有字节续传
//! - 模型通过临时文件 + rename 原子发布，Runtime 解压也先写临时文件
//! - 可选 SHA-256 校验：若 `expected_sha256` 给了就校；不一致返错 + 删坏文件
//!
//! 默认模型来源：rembg 项目维护的 GitHub Releases。URL 通过 `ModelSpec` 注入，
//! 不在本模块写死，方便测试 mock + 用户改私服。

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use zip::ZipArchive;

const DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_STALL_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const DOWNLOAD_PROGRESS_INTERVAL: u64 = 8 * 1024 * 1024;

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
    pub expected_archive_sha256: Option<String>,
    pub expected_dylib_sha256: Option<String>,
}

impl OrtDylibSpec {
    #[must_use]
    pub fn onnxruntime_1_22_windows_x64() -> Self {
        Self {
            file_name: "onnxruntime.dll".into(),
            archive_url: "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-win-x64-1.22.0.zip".into(),
            archive_member_suffix: "lib/onnxruntime.dll".into(),
            expected_archive_sha256: Some(
                "174c616efc0271194488642a72f1a514e01487da4dfe84c49296d66e40ebe0da".into(),
            ),
            expected_dylib_sha256: Some(
                "579b636403983254346a5c1d80bd28f1519cd1e284cd204f8d4ff41f8d711559".into(),
            ),
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
                "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8".into(),
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
        && let Err(err) = verify_sha256(&tmp, expected).await
    {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(err);
    }
    if target.is_file() {
        tokio::fs::remove_file(&target)
            .await
            .map_err(|e| ModelCacheError::Io(format!("remove {}: {e}", target.display())))?;
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
    if target.is_file() && file_ok(&target, spec.expected_dylib_sha256.as_deref()).await? {
        return Ok(target);
    }

    tokio::fs::create_dir_all(runtimes_dir)
        .await
        .map_err(|e| ModelCacheError::Io(format!("create runtimes_dir: {e}")))?;

    let archive_path = runtimes_dir.join("onnxruntime.zip");
    if !file_ok(&archive_path, spec.expected_archive_sha256.as_deref()).await? {
        download_to(&spec.archive_url, &archive_path).await?;
    }
    if let Some(expected) = spec.expected_archive_sha256.as_deref()
        && let Err(err) = verify_sha256(&archive_path, expected).await
    {
        let _ = tokio::fs::remove_file(&archive_path).await;
        return Err(err);
    }
    if target.is_file() {
        tokio::fs::remove_file(&target)
            .await
            .map_err(|e| ModelCacheError::Io(format!("remove {}: {e}", target.display())))?;
    }
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
    if let Some(expected) = spec.expected_dylib_sha256.as_deref()
        && let Err(err) = verify_sha256(&target, expected).await
    {
        let _ = tokio::fs::remove_file(&target).await;
        return Err(err);
    }
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
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| ModelCacheError::Io(format!("open {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .await
            .map_err(|e| ModelCacheError::Io(format!("read {}: {e}", path.display())))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn verify_sha256(path: &Path, expected: &str) -> Result<(), ModelCacheError> {
    let actual = sha256_of(path).await?;
    if actual.eq_ignore_ascii_case(expected) {
        return Ok(());
    }
    Err(ModelCacheError::Checksum {
        expected: expected.into(),
        actual,
    })
}

async fn download_to(url: &str, dest: &Path) -> Result<(), ModelCacheError> {
    let existing_len = tokio::fs::metadata(dest)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let client = reqwest::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .read_timeout(DOWNLOAD_STALL_TIMEOUT)
        .timeout(DOWNLOAD_TOTAL_TIMEOUT)
        .build()
        .map_err(|e| ModelCacheError::Http(format!("build download client: {e}")))?;
    let mut request = client.get(url);
    if existing_len > 0 {
        request = request.header(RANGE, format!("bytes={existing_len}-"));
    }
    let resp = request
        .send()
        .await
        .map_err(|e| ModelCacheError::Http(format!("GET {url}: {e}")))?;
    if resp.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
        && existing_len > 0
        && range_total(resp.headers().get(CONTENT_RANGE)) == Some(existing_len)
    {
        return Ok(());
    }
    if !resp.status().is_success() {
        return Err(ModelCacheError::Http(format!(
            "GET {url} returned {}",
            resp.status()
        )));
    }

    let append = existing_len > 0 && resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if append && range_start(resp.headers().get(CONTENT_RANGE)) != Some(existing_len) {
        return Err(ModelCacheError::Http(format!(
            "GET {url} returned an invalid Content-Range for offset {existing_len}"
        )));
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).write(true);
    if append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut file = options
        .open(dest)
        .await
        .map_err(|e| ModelCacheError::Io(format!("open {}: {e}", dest.display())))?;
    let mut stream = resp.bytes_stream();
    let mut written = if append { existing_len } else { 0 };
    let mut next_progress = written.saturating_add(DOWNLOAD_PROGRESS_INTERVAL);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ModelCacheError::Http(format!("read body: {e}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| ModelCacheError::Io(format!("write {}: {e}", dest.display())))?;
        written = written.saturating_add(chunk.len() as u64);
        if written >= next_progress {
            tracing::debug!(path = %dest.display(), bytes = written, "download progress");
            next_progress = written.saturating_add(DOWNLOAD_PROGRESS_INTERVAL);
        }
    }
    file.flush()
        .await
        .map_err(|e| ModelCacheError::Io(format!("flush {}: {e}", dest.display())))?;
    Ok(())
}

fn range_start(value: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    let value = value?.to_str().ok()?.strip_prefix("bytes ")?;
    value.split_once('-')?.0.parse().ok()
}

fn range_total(value: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    value?.to_str().ok()?.strip_prefix("bytes */")?.parse().ok()
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
    use std::io::{Read, Write};

    fn serve_download_once(
        status: &'static str,
        response_headers: &'static str,
        body: &'static [u8],
        expected_range: Option<&'static str>,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            if let Some(expected) = expected_range {
                assert!(request.contains(&format!("range: {expected}").to_ascii_lowercase()));
            }
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{response_headers}Connection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        (format!("http://{address}/download"), handle)
    }

    #[test]
    fn u2netp_spec_has_expected_fields() {
        let s = ModelSpec::u2netp();
        assert_eq!(s.file_name, "u2netp.onnx");
        assert!(s.url.starts_with("https://"));
        let sha = s.expected_sha256.unwrap();
        assert_eq!(
            sha,
            "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8"
        );
    }

    #[test]
    fn ort_dylib_spec_targets_windows_x64_dll() {
        let spec = OrtDylibSpec::onnxruntime_1_22_windows_x64();
        assert_eq!(spec.file_name, "onnxruntime.dll");
        assert!(spec.archive_url.contains("v1.22.0"));
        assert_eq!(spec.archive_member_suffix, "lib/onnxruntime.dll");
        assert_eq!(
            spec.expected_archive_sha256.as_deref(),
            Some("174c616efc0271194488642a72f1a514e01487da4dfe84c49296d66e40ebe0da")
        );
        assert_eq!(
            spec.expected_dylib_sha256.as_deref(),
            Some("579b636403983254346a5c1d80bd28f1519cd1e284cd204f8d4ff41f8d711559")
        );
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
    async fn ensure_model_replaces_invalid_cached_file() {
        let bytes = b"replacement model";
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let expected_sha256 = format!("{:x}", hasher.finalize());
        let (url, server) = serve_download_once("200 OK", "", bytes, None);
        let td = tempfile::TempDir::new().unwrap();
        let models = td.path().to_path_buf();
        let target = models.join("model.onnx");
        std::fs::write(&target, b"invalid cached model").unwrap();
        let spec = ModelSpec {
            file_name: "model.onnx".into(),
            url,
            expected_sha256: Some(expected_sha256),
        };

        let result = ensure_model(&models, &spec).await.unwrap();

        server.join().unwrap();
        assert_eq!(result, target);
        assert_eq!(tokio::fs::read(result).await.unwrap(), bytes);
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

    #[tokio::test]
    async fn verify_sha256_reports_actual_hash_on_mismatch() {
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("x.bin");
        tokio::fs::write(&path, b"hello").await.unwrap();

        let err = verify_sha256(
            &path,
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .await
        .unwrap_err();

        assert!(matches!(
            err,
            ModelCacheError::Checksum { actual, .. }
                if actual
                    == "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        ));
    }

    #[test]
    fn content_range_parsers_accept_resume_headers() {
        let partial = reqwest::header::HeaderValue::from_static("bytes 1024-2047/4096");
        let complete = reqwest::header::HeaderValue::from_static("bytes */4096");
        assert_eq!(range_start(Some(&partial)), Some(1024));
        assert_eq!(range_total(Some(&complete)), Some(4096));
        assert_eq!(range_start(Some(&complete)), None);
        assert_eq!(range_total(Some(&partial)), None);
    }

    #[tokio::test]
    async fn download_streams_response_to_new_file() {
        let (url, server) = serve_download_once("200 OK", "", b"hello world", None);
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("download.bin");

        download_to(&url, &path).await.unwrap();

        server.join().unwrap();
        assert_eq!(tokio::fs::read(path).await.unwrap(), b"hello world");
    }

    #[tokio::test]
    async fn download_resumes_existing_partial_file() {
        let (url, server) = serve_download_once(
            "206 Partial Content",
            "Content-Range: bytes 6-10/11\r\n",
            b"world",
            Some("bytes=6-"),
        );
        let td = tempfile::TempDir::new().unwrap();
        let path = td.path().join("download.bin");
        tokio::fs::write(&path, b"hello ").await.unwrap();

        download_to(&url, &path).await.unwrap();

        server.join().unwrap();
        assert_eq!(tokio::fs::read(path).await.unwrap(), b"hello world");
    }
}
