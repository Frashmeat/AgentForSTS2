//! Pack-driven acquisition and indexing for verified truth snapshots.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use super::hash::digest_file_cancellable;
use super::{TruthSnapshotError, TruthSnapshotIndex, TruthSnapshotSource, TruthSnapshotStore};
use crate::cancellation::CancellationToken;
use crate::controlled_process::{ControlledProcessResult, run_controlled_process};
use crate::game_pack::{LoadedGamePack, TruthSourceKind};
use crate::knowledge::{
    DecompileError, default_dotnet_tools_dirs, discover_ilspycmd, run_decompile_file,
    run_decompile_project,
};

const GITHUB_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const GITHUB_STALL_TIMEOUT: Duration = Duration::from_secs(45);
const GITHUB_TOTAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const ASSET_DOWNLOAD_MAX_ATTEMPTS: usize = 4;
const ASSET_DOWNLOAD_RETRY_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Error)]
pub enum TruthSnapshotRefreshError {
    #[error("truth snapshot refresh was cancelled")]
    Cancelled,
    #[error("truth snapshot refresh is already running for game pack `{0}`")]
    Busy(String),
    #[error("truth source `{source_id}` requires missing local input `{input_key}`")]
    MissingLocalInput {
        source_id: String,
        input_key: String,
    },
    #[error("truth source `{source_id}` local input is not a file: {path}")]
    LocalInputNotFile { source_id: String, path: PathBuf },
    #[error("fetch truth source `{source_id}`: {message}")]
    Fetch { source_id: String, message: String },
    #[error("index truth source `{source_id}` with `{indexer}`: {message}")]
    Index {
        source_id: String,
        indexer: String,
        message: String,
    },
    #[error("resolve truth indexer: {0}")]
    Tool(String),
    #[error("truth snapshot worker stopped unexpectedly: {0}")]
    Worker(String),
    #[error(transparent)]
    Snapshot(TruthSnapshotError),
    #[error("{action} `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

impl From<TruthSnapshotError> for TruthSnapshotRefreshError {
    fn from(error: TruthSnapshotError) -> Self {
        match error {
            TruthSnapshotError::Cancelled => Self::Cancelled,
            error => Self::Snapshot(error),
        }
    }
}

pub type TruthSnapshotRefreshResult<T> = Result<T, TruthSnapshotRefreshError>;

#[derive(Debug, Error)]
pub enum TruthSourceOperationError {
    #[error("truth source operation was cancelled")]
    Cancelled,
    #[error("{0}")]
    Failed(String),
}

pub type TruthSourceOperationResult<T> = Result<T, TruthSourceOperationError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruthSnapshotRefreshOutcome {
    pub snapshot_id: String,
    pub cache_hit: bool,
    pub source_count: usize,
    pub index_count: usize,
    pub tool_versions: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruthSnapshotReadiness {
    Ready,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruthSnapshotStatus {
    pub state: TruthSnapshotReadiness,
    pub game_pack_id: String,
    pub snapshot_id: Option<String>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub sources: Vec<TruthSnapshotSource>,
    pub indexes: Vec<TruthSnapshotIndex>,
    pub tool_versions: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

#[async_trait]
pub trait RemoteTruthSourceFetcher: Send + Sync {
    async fn fetch(
        &self,
        repository: &str,
        pinned_release: &str,
        asset: &str,
        destination: &Path,
        cancellation: &CancellationToken,
    ) -> TruthSourceOperationResult<()>;
}

#[async_trait]
pub trait TruthSourceIndexer: Send + Sync {
    async fn tool_versions(
        &self,
        cancellation: &CancellationToken,
    ) -> TruthSourceOperationResult<BTreeMap<String, String>>;

    async fn index(
        &self,
        indexer: &str,
        source: &Path,
        output_dir: &Path,
        cancellation: &CancellationToken,
    ) -> TruthSourceOperationResult<()>;
}

pub struct GitHubReleaseAssetFetcher {
    client: reqwest::Client,
    token: Option<String>,
    #[cfg(feature = "e2e")]
    release_url_override: Option<String>,
}

enum AssetDownloadAttemptError {
    Cancelled,
    Retryable(String),
    Fatal(String),
}

impl GitHubReleaseAssetFetcher {
    pub fn with_default_client(token: Option<String>) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .connect_timeout(GITHUB_CONNECT_TIMEOUT)
            .read_timeout(GITHUB_STALL_TIMEOUT)
            .timeout(GITHUB_TOTAL_TIMEOUT)
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            client,
            token: token.filter(|token| !token.is_empty()),
            #[cfg(feature = "e2e")]
            release_url_override: None,
        })
    }

    #[cfg(feature = "e2e")]
    #[must_use]
    pub fn with_release_url_override(mut self, url: String) -> Self {
        self.release_url_override = Some(url);
        self
    }

    fn release_url(&self, repository: &str, pinned_release: &str) -> Result<reqwest::Url, String> {
        #[cfg(feature = "e2e")]
        if let Some(url) = &self.release_url_override {
            return reqwest::Url::parse(url).map_err(|error| error.to_string());
        }

        let parts: Vec<_> = repository.split('/').collect();
        if parts.len() != 2 || parts.iter().any(|part| part.is_empty()) {
            return Err(format!(
                "GitHub repository must be exactly `owner/repository`, got `{repository}`"
            ));
        }
        let mut url =
            reqwest::Url::parse("https://api.github.com/").map_err(|error| error.to_string())?;
        url.path_segments_mut()
            .map_err(|_| "GitHub API URL cannot accept path segments".to_string())?
            .extend([
                "repos",
                parts[0],
                parts[1],
                "releases",
                "tags",
                pinned_release,
            ]);
        Ok(url)
    }

    fn api_request(&self, url: reqwest::Url) -> reqwest::RequestBuilder {
        let request = self
            .client
            .get(url)
            .header("User-Agent", "agentthespire-rust")
            .header("Accept", "application/vnd.github+json");
        match &self.token {
            Some(token) => request.header("Authorization", format!("Bearer {token}")),
            None => request,
        }
    }

    fn download_request(&self, url: reqwest::Url) -> reqwest::RequestBuilder {
        self.client
            .get(url)
            .header("User-Agent", "agentthespire-rust")
    }

    async fn download_asset(
        &self,
        url: &reqwest::Url,
        destination: &Path,
        cancellation: &CancellationToken,
    ) -> TruthSourceOperationResult<()> {
        if cancellation.is_cancelled() {
            return Err(TruthSourceOperationError::Cancelled);
        }
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                TruthSourceOperationError::Failed(format!("create download directory: {error}"))
            })?;
        }

        let mut last_error = String::new();
        for attempt in 1..=ASSET_DOWNLOAD_MAX_ATTEMPTS {
            match self
                .download_asset_attempt(url, destination, cancellation)
                .await
            {
                Ok(()) => return Ok(()),
                Err(AssetDownloadAttemptError::Cancelled) => {
                    return Err(TruthSourceOperationError::Cancelled);
                }
                Err(AssetDownloadAttemptError::Fatal(message)) => {
                    return Err(TruthSourceOperationError::Failed(message));
                }
                Err(AssetDownloadAttemptError::Retryable(message)) => {
                    last_error = message;
                    if attempt < ASSET_DOWNLOAD_MAX_ATTEMPTS {
                        tokio::select! {
                            _ = cancellation.cancelled() => {
                                return Err(TruthSourceOperationError::Cancelled);
                            }
                            _ = tokio::time::sleep(ASSET_DOWNLOAD_RETRY_DELAY) => {}
                        }
                    }
                }
            }
        }

        let downloaded = tokio::fs::metadata(destination)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        Err(TruthSourceOperationError::Failed(format!(
            "asset download failed after {ASSET_DOWNLOAD_MAX_ATTEMPTS} attempts at {downloaded} bytes: {last_error}"
        )))
    }

    async fn download_asset_attempt(
        &self,
        url: &reqwest::Url,
        destination: &Path,
        cancellation: &CancellationToken,
    ) -> Result<(), AssetDownloadAttemptError> {
        if cancellation.is_cancelled() {
            return Err(AssetDownloadAttemptError::Cancelled);
        }
        let existing_len = tokio::fs::metadata(destination)
            .await
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let mut request = self.download_request(url.clone());
        if existing_len > 0 {
            request = request.header(RANGE, format!("bytes={existing_len}-"));
        }
        let response = tokio::select! {
            _ = cancellation.cancelled() => {
                return Err(AssetDownloadAttemptError::Cancelled);
            }
            response = request.send() => response.map_err(|error| {
                AssetDownloadAttemptError::Retryable(format!("send asset request: {error}"))
            })?,
        };

        if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
            && existing_len > 0
            && unsatisfied_range_total(response.headers().get(CONTENT_RANGE)) == Some(existing_len)
        {
            return Ok(());
        }
        if !response.status().is_success() {
            let message = format!("GitHub asset download returned {}", response.status());
            return if response.status().is_server_error()
                || response.status() == reqwest::StatusCode::REQUEST_TIMEOUT
                || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
            {
                Err(AssetDownloadAttemptError::Retryable(message))
            } else {
                Err(AssetDownloadAttemptError::Fatal(message))
            };
        }

        let partial = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
        let expected_total = if partial {
            let Some((start, total)) = satisfied_range(response.headers().get(CONTENT_RANGE))
            else {
                return Err(AssetDownloadAttemptError::Fatal(format!(
                    "GitHub asset download returned invalid Content-Range for offset {existing_len}"
                )));
            };
            if start != existing_len {
                return Err(AssetDownloadAttemptError::Fatal(format!(
                    "GitHub asset download returned Content-Range start {start} for offset {existing_len}"
                )));
            }
            Some(total)
        } else {
            response.content_length()
        };

        let append = partial && existing_len > 0;
        let mut options = tokio::fs::OpenOptions::new();
        options.create(true).write(true);
        if append {
            options.append(true);
        } else {
            options.truncate(true);
        }
        let mut file = options.open(destination).await.map_err(|error| {
            AssetDownloadAttemptError::Fatal(format!(
                "open download destination {}: {error}",
                destination.display()
            ))
        })?;
        let mut written = if append { existing_len } else { 0 };
        let mut stream = response.bytes_stream();
        loop {
            let next = tokio::select! {
                _ = cancellation.cancelled() => {
                    return Err(AssetDownloadAttemptError::Cancelled);
                }
                next = stream.next() => next,
            };
            let Some(chunk) = next else {
                break;
            };
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    file.flush().await.map_err(|flush_error| {
                        AssetDownloadAttemptError::Fatal(format!(
                            "flush partial download {}: {flush_error}",
                            destination.display()
                        ))
                    })?;
                    return Err(AssetDownloadAttemptError::Retryable(format!(
                        "read asset body after {written} bytes: {error}"
                    )));
                }
            };
            tokio::select! {
                _ = cancellation.cancelled() => {
                    return Err(AssetDownloadAttemptError::Cancelled);
                }
                result = file.write_all(&chunk) => result.map_err(|error| {
                    AssetDownloadAttemptError::Fatal(format!(
                        "write downloaded asset {}: {error}",
                        destination.display()
                    ))
                })?,
            }
            written = written.saturating_add(chunk.len() as u64);
        }
        if cancellation.is_cancelled() {
            return Err(AssetDownloadAttemptError::Cancelled);
        }
        file.flush().await.map_err(|error| {
            AssetDownloadAttemptError::Fatal(format!(
                "flush downloaded asset {}: {error}",
                destination.display()
            ))
        })?;
        file.sync_all().await.map_err(|error| {
            AssetDownloadAttemptError::Fatal(format!(
                "sync downloaded asset {}: {error}",
                destination.display()
            ))
        })?;
        if let Some(expected) = expected_total
            && written != expected
        {
            return Err(AssetDownloadAttemptError::Retryable(format!(
                "asset body ended at {written} bytes, expected {expected}"
            )));
        }
        Ok(())
    }
}

fn satisfied_range(value: Option<&reqwest::header::HeaderValue>) -> Option<(u64, u64)> {
    let value = value?.to_str().ok()?.strip_prefix("bytes ")?;
    let (range, total) = value.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.parse().ok()?;
    let end: u64 = end.parse().ok()?;
    let total: u64 = total.parse().ok()?;
    (start <= end && end < total).then_some((start, total))
}

fn unsatisfied_range_total(value: Option<&reqwest::header::HeaderValue>) -> Option<u64> {
    value?.to_str().ok()?.strip_prefix("bytes */")?.parse().ok()
}

fn normalize_ilspycmd_version_line(line: &str) -> &str {
    line.split_once(':')
        .filter(|(label, value)| label.eq_ignore_ascii_case("ilspycmd") && !value.trim().is_empty())
        .map_or(line, |(_, value)| value.trim())
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[async_trait]
impl RemoteTruthSourceFetcher for GitHubReleaseAssetFetcher {
    async fn fetch(
        &self,
        repository: &str,
        pinned_release: &str,
        asset: &str,
        destination: &Path,
        cancellation: &CancellationToken,
    ) -> TruthSourceOperationResult<()> {
        let release_url = self
            .release_url(repository, pinned_release)
            .map_err(TruthSourceOperationError::Failed)?;
        let response = tokio::select! {
            _ = cancellation.cancelled() => {
                return Err(TruthSourceOperationError::Cancelled);
            }
            response = self.api_request(release_url).send() => response
                .map_err(|error| TruthSourceOperationError::Failed(error.to_string()))?,
        };
        if !response.status().is_success() {
            return Err(TruthSourceOperationError::Failed(format!(
                "GitHub release API returned {}",
                response.status()
            )));
        }
        let release: GitHubRelease = tokio::select! {
            _ = cancellation.cancelled() => {
                return Err(TruthSourceOperationError::Cancelled);
            }
            release = response.json() => release.map_err(|error| {
                TruthSourceOperationError::Failed(format!(
                    "parse GitHub release response: {error}"
                ))
            })?,
        };
        if release.tag_name != pinned_release {
            return Err(TruthSourceOperationError::Failed(format!(
                "GitHub release tag mismatch: expected `{pinned_release}`, got `{}`",
                release.tag_name
            )));
        }
        let release_asset = release
            .assets
            .iter()
            .find(|candidate| candidate.name == asset)
            .ok_or_else(|| {
                TruthSourceOperationError::Failed(format!(
                    "release `{pinned_release}` has no exact asset `{asset}`"
                ))
            })?;
        let download_url =
            reqwest::Url::parse(&release_asset.browser_download_url).map_err(|error| {
                TruthSourceOperationError::Failed(format!("invalid asset download URL: {error}"))
            })?;
        // Never forward the GitHub API bearer token to a URL selected by the
        // release response.
        self.download_asset(&download_url, destination, cancellation)
            .await
    }
}

#[derive(Debug, Clone)]
pub struct IlspycmdTruthIndexer {
    executable: PathBuf,
}

impl IlspycmdTruthIndexer {
    pub fn discover(explicit: Option<PathBuf>) -> Result<Self, String> {
        if let Some(executable) = explicit {
            if executable.is_file() {
                return Ok(Self { executable });
            }
            return Err(format!(
                "ilspycmd path is not a file: {}",
                executable.display()
            ));
        }
        let executable = discover_ilspycmd(&default_dotnet_tools_dirs()).ok_or_else(|| {
            "ilspycmd not found on PATH or in the default .NET tools directory".to_string()
        })?;
        Ok(Self { executable })
    }
}

#[async_trait]
impl TruthSourceIndexer for IlspycmdTruthIndexer {
    async fn tool_versions(
        &self,
        cancellation: &CancellationToken,
    ) -> TruthSourceOperationResult<BTreeMap<String, String>> {
        let args = [std::ffi::OsString::from("--version")];
        let cwd = self.executable.parent().unwrap_or_else(|| Path::new("."));
        let output =
            match run_controlled_process(self.executable.as_os_str(), &args, cwd, cancellation)
                .await
                .map_err(|error| {
                    TruthSourceOperationError::Failed(format!("run ilspycmd --version: {error}"))
                })? {
                ControlledProcessResult::Completed(output) => output,
                ControlledProcessResult::Cancelled(_) => {
                    return Err(TruthSourceOperationError::Cancelled);
                }
            };
        if !output.status.success() {
            return Err(TruthSourceOperationError::Failed(format!(
                "ilspycmd --version exited with code {}",
                output.status.code().unwrap_or(-1)
            )));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let version_line = stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
            .ok_or_else(|| {
                TruthSourceOperationError::Failed(
                    "ilspycmd --version returned no version text".to_string(),
                )
            })?;
        let version = normalize_ilspycmd_version_line(version_line);
        Ok(BTreeMap::from([("ilspycmd".into(), version.into())]))
    }

    async fn index(
        &self,
        indexer: &str,
        source: &Path,
        output_dir: &Path,
        cancellation: &CancellationToken,
    ) -> TruthSourceOperationResult<()> {
        match indexer {
            "dotnet_project" => {
                run_decompile_project(&self.executable, source, output_dir, cancellation)
                    .await
                    .map(|_| ())
                    .map_err(map_decompile_error)
            }
            "dotnet_file" => run_decompile_file(
                &self.executable,
                source,
                &output_dir.join("decompiled.cs"),
                cancellation,
            )
            .await
            .map(|_| ())
            .map_err(map_decompile_error),
            other => Err(TruthSourceOperationError::Failed(format!(
                "unsupported truth indexer `{other}`"
            ))),
        }
    }
}

fn map_decompile_error(error: DecompileError) -> TruthSourceOperationError {
    match error {
        DecompileError::Cancelled => TruthSourceOperationError::Cancelled,
        error => TruthSourceOperationError::Failed(error.to_string()),
    }
}

#[derive(Clone)]
pub struct TruthSnapshotRefresher {
    fetcher: Arc<dyn RemoteTruthSourceFetcher>,
    indexer: Arc<dyn TruthSourceIndexer>,
}

impl TruthSnapshotRefresher {
    #[must_use]
    pub fn new(
        fetcher: Arc<dyn RemoteTruthSourceFetcher>,
        indexer: Arc<dyn TruthSourceIndexer>,
    ) -> Self {
        Self { fetcher, indexer }
    }

    pub async fn refresh(
        &self,
        pack: &LoadedGamePack,
        store: &TruthSnapshotStore,
        local_inputs: &BTreeMap<String, PathBuf>,
        force: bool,
    ) -> TruthSnapshotRefreshResult<TruthSnapshotRefreshOutcome> {
        self.refresh_cancellable(pack, store, local_inputs, force, &CancellationToken::new())
            .await
    }

    pub async fn refresh_cancellable(
        &self,
        pack: &LoadedGamePack,
        store: &TruthSnapshotStore,
        local_inputs: &BTreeMap<String, PathBuf>,
        force: bool,
        cancellation: &CancellationToken,
    ) -> TruthSnapshotRefreshResult<TruthSnapshotRefreshOutcome> {
        if cancellation.is_cancelled() {
            return Err(TruthSnapshotRefreshError::Cancelled);
        }
        validate_truth_source_inputs(pack, local_inputs)?;
        let _refresh_lock = acquire_refresh_lock(store, pack)?;

        let tool_versions = self
            .indexer
            .tool_versions(cancellation)
            .await
            .map_err(|error| match error {
                TruthSourceOperationError::Cancelled => TruthSnapshotRefreshError::Cancelled,
                TruthSourceOperationError::Failed(message) => {
                    TruthSnapshotRefreshError::Tool(message)
                }
            })?;

        let mut warnings = Vec::new();
        let current_store = store.clone();
        let current_pack = pack.clone();
        let current_cancellation = cancellation.clone();
        let current = match tokio::task::spawn_blocking(move || {
            current_store.open_current_cancellable(&current_pack, &current_cancellation)
        })
        .await
        .map_err(|error| TruthSnapshotRefreshError::Worker(error.to_string()))?
        {
            Ok(current) => current,
            Err(TruthSnapshotError::Cancelled) => {
                return Err(TruthSnapshotRefreshError::Cancelled);
            }
            Err(error) => {
                warnings.push(format!(
                    "existing current snapshot is invalid and will not be reused: {error}"
                ));
                None
            }
        };
        if !force && let Some(current) = &current {
            let checked_current = current.clone();
            let checked_pack = pack.clone();
            let checked_inputs = local_inputs.clone();
            let checked_tools = tool_versions.clone();
            let checked_cancellation = cancellation.clone();
            let matches = tokio::task::spawn_blocking(move || {
                current_matches_inputs(
                    &checked_current,
                    &checked_pack,
                    &checked_inputs,
                    &checked_tools,
                    &checked_cancellation,
                )
            })
            .await
            .map_err(|error| TruthSnapshotRefreshError::Worker(error.to_string()))??;
            if matches {
                return Ok(outcome(current, true, warnings));
            }
        }

        let draft = store.begin(pack)?;
        let mut source_paths = BTreeMap::new();
        for declaration in &pack.truth_sources {
            if cancellation.is_cancelled() {
                return Err(TruthSnapshotRefreshError::Cancelled);
            }
            let source_path = match &declaration.kind {
                TruthSourceKind::LocalFile { input_key } => local_inputs[input_key].clone(),
                TruthSourceKind::GitHubReleaseAsset {
                    repository,
                    pinned_release,
                    asset,
                    ..
                } => {
                    if !force
                        && let Some(cached) = current
                            .as_ref()
                            .and_then(|snapshot| snapshot.source_path(&declaration.id))
                    {
                        cached
                    } else {
                        let destination = draft
                            .staging_root()
                            .join(".fetch")
                            .join(&declaration.id)
                            .join(asset);
                        self.fetcher
                            .fetch(
                                repository,
                                pinned_release,
                                asset,
                                &destination,
                                cancellation,
                            )
                            .await
                            .map_err(|error| match error {
                                TruthSourceOperationError::Cancelled => {
                                    TruthSnapshotRefreshError::Cancelled
                                }
                                TruthSourceOperationError::Failed(message) => {
                                    TruthSnapshotRefreshError::Fetch {
                                        source_id: declaration.id.clone(),
                                        message,
                                    }
                                }
                            })?;
                        destination
                    }
                }
            };
            source_paths.insert(declaration.id.clone(), source_path);
        }

        let staging_cancellation = cancellation.clone();
        let (draft, staged_sources) = tokio::task::spawn_blocking(move || {
            let mut draft = draft;
            let mut staged_sources = BTreeMap::new();
            for (source_id, source_path) in source_paths {
                let staged = draft.stage_source_cancellable(
                    &source_id,
                    &source_path,
                    &staging_cancellation,
                )?;
                staged_sources.insert(source_id, staged);
            }
            let fetch_root = draft.staging_root().join(".fetch");
            if fetch_root.exists() {
                if staging_cancellation.is_cancelled() {
                    return Err(TruthSnapshotRefreshError::Cancelled);
                }
                fs::remove_dir_all(&fetch_root).map_err(|source| {
                    TruthSnapshotRefreshError::Io {
                        action: "remove staged truth source download",
                        path: fetch_root,
                        source,
                    }
                })?;
            }
            Ok::<_, TruthSnapshotRefreshError>((draft, staged_sources))
        })
        .await
        .map_err(|error| TruthSnapshotRefreshError::Worker(error.to_string()))??;

        for declaration in &pack.truth_sources {
            if cancellation.is_cancelled() {
                return Err(TruthSnapshotRefreshError::Cancelled);
            }
            let source = staged_sources[&declaration.id].clone();
            let output = draft.index_output_dir(&declaration.id)?;
            let source_id = declaration.id.clone();
            self.indexer
                .index(&declaration.indexer, &source, &output, cancellation)
                .await
                .map_err(|error| match error {
                    TruthSourceOperationError::Cancelled => TruthSnapshotRefreshError::Cancelled,
                    TruthSourceOperationError::Failed(message) => {
                        TruthSnapshotRefreshError::Index {
                            source_id,
                            indexer: declaration.indexer.clone(),
                            message,
                        }
                    }
                })?;
        }

        let prepare_cancellation = cancellation.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            draft.prepare_cancellable(tool_versions, &prepare_cancellation)
        })
        .await
        .map_err(|error| TruthSnapshotRefreshError::Worker(error.to_string()))??;

        let activate_cancellation = cancellation.clone();
        let verified = tokio::task::spawn_blocking(move || {
            prepared.activate_cancellable(&activate_cancellation)
        })
        .await
        .map_err(|error| TruthSnapshotRefreshError::Worker(error.to_string()))??;
        Ok(outcome(&verified, false, warnings))
    }
}

pub fn inspect_truth_snapshot(
    pack: &LoadedGamePack,
    store: &TruthSnapshotStore,
) -> TruthSnapshotStatus {
    match store.open_current(pack) {
        Ok(Some(snapshot)) => {
            let manifest = snapshot.manifest();
            TruthSnapshotStatus {
                state: TruthSnapshotReadiness::Ready,
                game_pack_id: pack.id.clone(),
                snapshot_id: Some(manifest.snapshot_id.clone()),
                created_at: Some(manifest.created_at),
                sources: manifest.sources.clone(),
                indexes: manifest.indexes.clone(),
                tool_versions: manifest.tool_versions.clone(),
                warnings: Vec::new(),
            }
        }
        Ok(None) => TruthSnapshotStatus {
            state: TruthSnapshotReadiness::Missing,
            game_pack_id: pack.id.clone(),
            snapshot_id: None,
            created_at: None,
            sources: Vec::new(),
            indexes: Vec::new(),
            tool_versions: BTreeMap::new(),
            warnings: vec!["no current truth snapshot is active".into()],
        },
        Err(error) => TruthSnapshotStatus {
            state: TruthSnapshotReadiness::Invalid,
            game_pack_id: pack.id.clone(),
            snapshot_id: None,
            created_at: None,
            sources: Vec::new(),
            indexes: Vec::new(),
            tool_versions: BTreeMap::new(),
            warnings: vec![error.to_string()],
        },
    }
}

pub fn validate_truth_source_inputs(
    pack: &LoadedGamePack,
    local_inputs: &BTreeMap<String, PathBuf>,
) -> TruthSnapshotRefreshResult<()> {
    for source in &pack.truth_sources {
        if let TruthSourceKind::LocalFile { input_key } = &source.kind {
            let path = local_inputs.get(input_key).ok_or_else(|| {
                TruthSnapshotRefreshError::MissingLocalInput {
                    source_id: source.id.clone(),
                    input_key: input_key.clone(),
                }
            })?;
            if !path.is_file() {
                return Err(TruthSnapshotRefreshError::LocalInputNotFile {
                    source_id: source.id.clone(),
                    path: path.clone(),
                });
            }
        }
    }
    Ok(())
}

fn current_matches_inputs(
    current: &super::VerifiedTruthSnapshot,
    pack: &LoadedGamePack,
    local_inputs: &BTreeMap<String, PathBuf>,
    tool_versions: &BTreeMap<String, String>,
    cancellation: &CancellationToken,
) -> TruthSnapshotRefreshResult<bool> {
    if cancellation.is_cancelled() {
        return Err(TruthSnapshotRefreshError::Cancelled);
    }
    if &current.manifest().tool_versions != tool_versions {
        return Ok(false);
    }
    for declaration in &pack.truth_sources {
        if cancellation.is_cancelled() {
            return Err(TruthSnapshotRefreshError::Cancelled);
        }
        if let TruthSourceKind::LocalFile { input_key } = &declaration.kind {
            let digest = digest_file_cancellable(&local_inputs[input_key], cancellation)?;
            let Some(source) = current
                .manifest()
                .sources
                .iter()
                .find(|source| source.id == declaration.id)
            else {
                return Ok(false);
            };
            if digest.sha256 != source.sha256 || digest.size_bytes != source.size_bytes {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn outcome(
    snapshot: &super::VerifiedTruthSnapshot,
    cache_hit: bool,
    warnings: Vec<String>,
) -> TruthSnapshotRefreshOutcome {
    let manifest = snapshot.manifest();
    TruthSnapshotRefreshOutcome {
        snapshot_id: manifest.snapshot_id.clone(),
        cache_hit,
        source_count: manifest.sources.len(),
        index_count: manifest.indexes.len(),
        tool_versions: manifest.tool_versions.clone(),
        warnings,
    }
}

fn acquire_refresh_lock(
    store: &TruthSnapshotStore,
    pack: &LoadedGamePack,
) -> TruthSnapshotRefreshResult<std::fs::File> {
    fs::create_dir_all(store.root()).map_err(|source| TruthSnapshotRefreshError::Io {
        action: "create truth snapshot store",
        path: store.root().to_path_buf(),
        source,
    })?;
    let path = store.root().join("refresh.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|source| TruthSnapshotRefreshError::Io {
            action: "open truth snapshot refresh lock",
            path: path.clone(),
            source,
        })?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(fs::TryLockError::WouldBlock) => Err(TruthSnapshotRefreshError::Busy(pack.id.clone())),
        Err(fs::TryLockError::Error(source)) => Err(TruthSnapshotRefreshError::Io {
            action: "lock truth snapshot refresh",
            path,
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use sha2::{Digest, Sha256};
    use tokio::sync::Barrier;

    use super::*;
    use crate::game_pack::{GamePackLoadPolicy, GamePackLoader};
    use crate::platform::domain::CancellationReason;

    const GAME: &[u8] = b"fixture-game";
    const BASELIB: &[u8] = b"fixture-baselib";

    fn fixture_pack(remote_sha: &str) -> LoadedGamePack {
        let json = format!(
            r#"{{
              "schema_version": 1,
              "id": "fixture-game",
              "display_name": "Fixture Game",
              "capabilities": ["truth_sources"],
              "truth_sources": [
                {{"id":"game","kind":"local_file","input_key":"game_assembly","indexer":"dotnet_project","provider":"fixture_facts"}},
                {{"id":"library","kind":"github_release_asset","repository":"owner/repo","pinned_release":"v1.2.3","asset":"Library.dll","sha256":"{remote_sha}","indexer":"dotnet_file","provider":"fixture_facts"}}
              ]
            }}"#
        );
        GamePackLoader::new(GamePackLoadPolicy::new(
            ["truth_sources"],
            ["dotnet_project", "dotnet_file"],
            ["fixture_facts"],
        ))
        .load_str("fixture", &json)
        .unwrap()
    }

    #[derive(Default)]
    struct MockFetcher {
        calls: AtomicUsize,
    }

    #[async_trait]
    impl RemoteTruthSourceFetcher for MockFetcher {
        async fn fetch(
            &self,
            repository: &str,
            pinned_release: &str,
            asset: &str,
            destination: &Path,
            cancellation: &CancellationToken,
        ) -> TruthSourceOperationResult<()> {
            if cancellation.is_cancelled() {
                return Err(TruthSourceOperationError::Cancelled);
            }
            assert_eq!(repository, "owner/repo");
            assert_eq!(pinned_release, "v1.2.3");
            assert_eq!(asset, "Library.dll");
            self.calls.fetch_add(1, Ordering::SeqCst);
            fs::create_dir_all(destination.parent().unwrap()).unwrap();
            fs::write(destination, BASELIB).unwrap();
            Ok(())
        }
    }

    struct MockIndexer {
        calls: AtomicUsize,
        fail_on: Option<String>,
        version: String,
    }

    impl MockIndexer {
        fn good() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                fail_on: None,
                version: "9.1.0".into(),
            }
        }
    }

    #[async_trait]
    impl TruthSourceIndexer for MockIndexer {
        async fn tool_versions(
            &self,
            cancellation: &CancellationToken,
        ) -> TruthSourceOperationResult<BTreeMap<String, String>> {
            if cancellation.is_cancelled() {
                return Err(TruthSourceOperationError::Cancelled);
            }
            Ok(BTreeMap::from([("ilspycmd".into(), self.version.clone())]))
        }

        async fn index(
            &self,
            indexer: &str,
            source: &Path,
            output_dir: &Path,
            cancellation: &CancellationToken,
        ) -> TruthSourceOperationResult<()> {
            if cancellation.is_cancelled() {
                return Err(TruthSourceOperationError::Cancelled);
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_on.as_deref() == Some(indexer) {
                return Err(TruthSourceOperationError::Failed(
                    "injected index failure".into(),
                ));
            }
            fs::create_dir_all(output_dir).unwrap();
            let bytes = fs::read(source).unwrap();
            fs::write(
                output_dir.join(format!("{indexer}.cs")),
                format!("// {}", format!("{:x}", Sha256::digest(bytes))),
            )
            .unwrap();
            Ok(())
        }
    }

    struct BlockingIndexer {
        entered: Arc<Barrier>,
    }

    #[async_trait]
    impl TruthSourceIndexer for BlockingIndexer {
        async fn tool_versions(
            &self,
            cancellation: &CancellationToken,
        ) -> TruthSourceOperationResult<BTreeMap<String, String>> {
            if cancellation.is_cancelled() {
                return Err(TruthSourceOperationError::Cancelled);
            }
            Ok(BTreeMap::from([("ilspycmd".into(), "9.2.0".into())]))
        }

        async fn index(
            &self,
            _indexer: &str,
            _source: &Path,
            _output_dir: &Path,
            cancellation: &CancellationToken,
        ) -> TruthSourceOperationResult<()> {
            self.entered.wait().await;
            cancellation.cancelled().await;
            Err(TruthSourceOperationError::Cancelled)
        }
    }

    fn inputs(root: &Path, bytes: &[u8]) -> BTreeMap<String, PathBuf> {
        let game = root.join("game.dll");
        fs::write(&game, bytes).unwrap();
        BTreeMap::from([("game_assembly".into(), game)])
    }

    fn refresher(fetcher: Arc<MockFetcher>, indexer: Arc<MockIndexer>) -> TruthSnapshotRefresher {
        TruthSnapshotRefresher::new(fetcher, indexer)
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
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
        String::from_utf8(request).unwrap().to_ascii_lowercase()
    }

    #[tokio::test]
    async fn good_refresh_activates_verified_snapshot_and_cache_hit_skips_work() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        let fetcher = Arc::new(MockFetcher::default());
        let indexer = Arc::new(MockIndexer::good());
        let refresher = refresher(Arc::clone(&fetcher), Arc::clone(&indexer));
        let inputs = inputs(temp.path(), GAME);

        let first = refresher
            .refresh(&pack, &store, &inputs, false)
            .await
            .unwrap();
        assert!(!first.cache_hit);
        assert_eq!(first.source_count, 2);
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
        assert_eq!(indexer.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            inspect_truth_snapshot(&pack, &store).state,
            TruthSnapshotReadiness::Ready
        );

        let second = refresher
            .refresh(&pack, &store, &inputs, false)
            .await
            .unwrap();
        assert!(second.cache_hit);
        assert_eq!(second.snapshot_id, first.snapshot_id);
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
        assert_eq!(indexer.calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn cancellation_during_index_preserves_previous_current_snapshot() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        let fetcher = Arc::new(MockFetcher::default());
        let initial_inputs = inputs(temp.path(), GAME);
        let initial = refresher(Arc::clone(&fetcher), Arc::new(MockIndexer::good()))
            .refresh(&pack, &store, &initial_inputs, false)
            .await
            .unwrap();

        let changed_inputs = inputs(temp.path(), b"changed-game");
        let entered = Arc::new(Barrier::new(2));
        let blocking = TruthSnapshotRefresher::new(
            fetcher,
            Arc::new(BlockingIndexer {
                entered: Arc::clone(&entered),
            }),
        );
        let cancellation = CancellationToken::new();
        let worker_token = cancellation.clone();
        let worker_pack = pack.clone();
        let worker_store = store.clone();
        let task = tokio::spawn(async move {
            blocking
                .refresh_cancellable(
                    &worker_pack,
                    &worker_store,
                    &changed_inputs,
                    false,
                    &worker_token,
                )
                .await
        });

        entered.wait().await;
        assert!(cancellation.cancel(CancellationReason::ProjectClose));
        assert!(matches!(
            task.await.unwrap(),
            Err(TruthSnapshotRefreshError::Cancelled)
        ));
        let current = store.open_current(&pack).unwrap().unwrap();
        assert_eq!(current.snapshot_id(), initial.snapshot_id);
    }

    #[tokio::test]
    async fn changed_game_creates_new_snapshot_and_reuses_verified_remote_source() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        let fetcher = Arc::new(MockFetcher::default());
        let indexer = Arc::new(MockIndexer::good());
        let refresher = refresher(Arc::clone(&fetcher), indexer);
        let mut bound_inputs = inputs(temp.path(), GAME);
        let first = refresher
            .refresh(&pack, &store, &bound_inputs, false)
            .await
            .unwrap();

        bound_inputs = inputs(temp.path(), b"changed-game");
        let second = refresher
            .refresh(&pack, &store, &bound_inputs, false)
            .await
            .unwrap();
        assert_ne!(second.snapshot_id, first.snapshot_id);
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn forced_identical_refresh_reexecutes_work_and_deduplicates_snapshot() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        let fetcher = Arc::new(MockFetcher::default());
        let indexer = Arc::new(MockIndexer::good());
        let refresher = refresher(Arc::clone(&fetcher), Arc::clone(&indexer));
        let inputs = inputs(temp.path(), GAME);
        let first = refresher
            .refresh(&pack, &store, &inputs, false)
            .await
            .unwrap();

        let forced = refresher
            .refresh(&pack, &store, &inputs, true)
            .await
            .unwrap();
        assert!(!forced.cache_hit);
        assert_eq!(forced.snapshot_id, first.snapshot_id);
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
        assert_eq!(indexer.calls.load(Ordering::SeqCst), 4);
        assert_eq!(
            fs::read_dir(store.root().join("snapshots"))
                .unwrap()
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn bad_remote_checksum_never_replaces_current() {
        let temp = tempfile::TempDir::new().unwrap();
        let good_pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &good_pack);
        let fetcher = Arc::new(MockFetcher::default());
        let indexer = Arc::new(MockIndexer::good());
        let refresher = refresher(fetcher, indexer);
        let inputs = inputs(temp.path(), GAME);
        let first = refresher
            .refresh(&good_pack, &store, &inputs, false)
            .await
            .unwrap();

        let bad_pack = fixture_pack(&"0".repeat(64));
        let bad_store = TruthSnapshotStore::new(temp.path(), &bad_pack);
        let error = refresher
            .refresh(&bad_pack, &bad_store, &inputs, true)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            TruthSnapshotRefreshError::Snapshot(TruthSnapshotError::SourceChecksumMismatch { .. })
        ));
        assert_eq!(
            store
                .open_snapshot(&good_pack, &first.snapshot_id)
                .unwrap()
                .snapshot_id(),
            first.snapshot_id
        );
    }

    #[tokio::test]
    async fn index_failure_preserves_previous_current() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        let fetcher = Arc::new(MockFetcher::default());
        let good = refresher(Arc::clone(&fetcher), Arc::new(MockIndexer::good()));
        let inputs = inputs(temp.path(), GAME);
        let first = good.refresh(&pack, &store, &inputs, false).await.unwrap();

        let failing = refresher(
            fetcher,
            Arc::new(MockIndexer {
                calls: AtomicUsize::new(0),
                fail_on: Some("dotnet_file".into()),
                version: "9.2.0".into(),
            }),
        );
        let error = failing
            .refresh(&pack, &store, &inputs, false)
            .await
            .unwrap_err();
        assert!(matches!(error, TruthSnapshotRefreshError::Index { .. }));
        assert_eq!(
            store.open_current(&pack).unwrap().unwrap().snapshot_id(),
            first.snapshot_id
        );
    }

    #[tokio::test]
    async fn missing_local_input_is_rejected_before_fetch_or_index() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        let fetcher = Arc::new(MockFetcher::default());
        let indexer = Arc::new(MockIndexer::good());
        let refresher = refresher(Arc::clone(&fetcher), Arc::clone(&indexer));

        let error = refresher
            .refresh(&pack, &store, &BTreeMap::new(), false)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            TruthSnapshotRefreshError::MissingLocalInput { .. }
        ));
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
        assert_eq!(indexer.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn concurrent_refresh_is_rejected_by_store_lock() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        fs::create_dir_all(store.root()).unwrap();
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(store.root().join("refresh.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let fetcher = Arc::new(MockFetcher::default());
        let indexer = Arc::new(MockIndexer::good());
        let refresher = refresher(Arc::clone(&fetcher), Arc::clone(&indexer));

        let error = refresher
            .refresh(&pack, &store, &inputs(temp.path(), GAME), false)
            .await
            .unwrap_err();
        assert!(matches!(error, TruthSnapshotRefreshError::Busy(id) if id == pack.id));
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
        assert_eq!(indexer.calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn github_release_url_is_pinned_and_never_latest() {
        let fetcher = GitHubReleaseAssetFetcher::with_default_client(None).unwrap();
        let url = fetcher.release_url("owner/repo", "v1.2.3").unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.github.com/repos/owner/repo/releases/tags/v1.2.3"
        );
        assert!(!url.as_str().contains("latest"));
    }

    #[test]
    fn asset_download_does_not_forward_github_token() {
        let fetcher =
            GitHubReleaseAssetFetcher::with_default_client(Some("secret-token".into())).unwrap();
        let api_request = fetcher
            .api_request(reqwest::Url::parse("https://api.github.com/release").unwrap())
            .build()
            .unwrap();
        assert!(api_request.headers().contains_key("authorization"));

        let download_request = fetcher
            .download_request(reqwest::Url::parse("https://example.invalid/asset").unwrap())
            .build()
            .unwrap();
        assert!(!download_request.headers().contains_key("authorization"));
    }

    #[test]
    fn ilspycmd_version_value_does_not_repeat_tool_name() {
        assert_eq!(
            normalize_ilspycmd_version_line("ilspycmd: 9.1.0.7988"),
            "9.1.0.7988"
        );
        assert_eq!(normalize_ilspycmd_version_line("9.1.0.7988"), "9.1.0.7988");
    }

    #[tokio::test]
    async fn asset_download_resumes_after_interrupted_body() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut first, _) = listener.accept().unwrap();
            let first_request = read_http_request(&mut first);
            assert!(!first_request.contains("range:"));
            write!(
                first,
                "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            first.write_all(b"hello ").unwrap();
            first.flush().unwrap();
            drop(first);

            let (mut second, _) = listener.accept().unwrap();
            let second_request = read_http_request(&mut second);
            assert!(second_request.contains("range: bytes=6-"));
            write!(
                second,
                "HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\nContent-Range: bytes 6-10/11\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            second.write_all(b"world").unwrap();
        });
        let temp = tempfile::TempDir::new().unwrap();
        let destination = temp.path().join("Library.dll");
        let fetcher = GitHubReleaseAssetFetcher::with_default_client(None).unwrap();

        fetcher
            .download_asset(
                &reqwest::Url::parse(&format!("http://{address}/Library.dll")).unwrap(),
                &destination,
                &CancellationToken::new(),
            )
            .await
            .unwrap();

        server.join().unwrap();
        assert_eq!(tokio::fs::read(destination).await.unwrap(), b"hello world");
    }

    #[tokio::test]
    async fn asset_download_cancels_while_response_body_is_stalled() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (body_started_tx, body_started_rx) = tokio::sync::oneshot::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_http_request(&mut stream);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial"
            )
            .unwrap();
            stream.flush().unwrap();
            let _ = body_started_tx.send(());
            std::thread::sleep(Duration::from_millis(500));
        });
        let temp = tempfile::TempDir::new().unwrap();
        let destination = temp.path().join("Library.dll");
        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let task = tokio::spawn(async move {
            GitHubReleaseAssetFetcher::with_default_client(None)
                .unwrap()
                .download_asset(
                    &reqwest::Url::parse(&format!("http://{address}/Library.dll")).unwrap(),
                    &destination,
                    &worker_cancellation,
                )
                .await
        });

        body_started_rx.await.unwrap();
        cancellation.cancel(CancellationReason::ProjectClose);
        let result = tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("stalled response body did not observe cancellation")
            .unwrap();
        assert!(matches!(result, Err(TruthSourceOperationError::Cancelled)));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn asset_download_rejects_mismatched_content_range() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            assert!(request.contains("range: bytes=6-"));
            write!(
                stream,
                "HTTP/1.1 206 Partial Content\r\nContent-Length: 6\r\nContent-Range: bytes 5-10/11\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            stream.write_all(b" world").unwrap();
        });
        let temp = tempfile::TempDir::new().unwrap();
        let destination = temp.path().join("Library.dll");
        tokio::fs::write(&destination, b"hello ").await.unwrap();
        let fetcher = GitHubReleaseAssetFetcher::with_default_client(None).unwrap();

        let error = fetcher
            .download_asset(
                &reqwest::Url::parse(&format!("http://{address}/Library.dll")).unwrap(),
                &destination,
                &CancellationToken::new(),
            )
            .await
            .unwrap_err();

        server.join().unwrap();
        assert!(
            error
                .to_string()
                .contains("Content-Range start 5 for offset 6")
        );
        assert_eq!(tokio::fs::read(destination).await.unwrap(), b"hello ");
    }

    #[test]
    fn status_reports_missing_and_invalid() {
        let temp = tempfile::TempDir::new().unwrap();
        let pack = fixture_pack(&format!("{:x}", Sha256::digest(BASELIB)));
        let store = TruthSnapshotStore::new(temp.path(), &pack);
        assert_eq!(
            inspect_truth_snapshot(&pack, &store).state,
            TruthSnapshotReadiness::Missing
        );
        fs::create_dir_all(store.root()).unwrap();
        fs::write(store.root().join("current.json"), b"not-json").unwrap();
        assert_eq!(
            inspect_truth_snapshot(&pack, &store).state,
            TruthSnapshotReadiness::Invalid
        );
    }
}
