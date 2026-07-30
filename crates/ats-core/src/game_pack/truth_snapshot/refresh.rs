//! Pack-driven acquisition and indexing for verified truth snapshots.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

use super::hash::digest_file;
use super::{TruthSnapshotError, TruthSnapshotIndex, TruthSnapshotSource, TruthSnapshotStore};
use crate::game_pack::{LoadedGamePack, TruthSourceKind};
use crate::knowledge::{
    default_dotnet_tools_dirs, discover_ilspycmd, run_decompile_file, run_decompile_project,
};

#[derive(Debug, Error)]
pub enum TruthSnapshotRefreshError {
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
    Snapshot(#[from] TruthSnapshotError),
    #[error("{action} `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub type TruthSnapshotRefreshResult<T> = Result<T, TruthSnapshotRefreshError>;

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
    ) -> Result<(), String>;
}

pub trait TruthSourceIndexer: Send + Sync {
    fn tool_versions(&self) -> Result<BTreeMap<String, String>, String>;

    fn index(&self, indexer: &str, source: &Path, output_dir: &Path) -> Result<(), String>;
}

pub struct GitHubReleaseAssetFetcher {
    client: reqwest::Client,
    token: Option<String>,
    #[cfg(feature = "e2e")]
    release_url_override: Option<String>,
}

impl GitHubReleaseAssetFetcher {
    pub fn with_default_client(token: Option<String>) -> Result<Self, String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
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
    ) -> Result<(), String> {
        let release_url = self.release_url(repository, pinned_release)?;
        let response = self
            .api_request(release_url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("GitHub release API returned {}", response.status()));
        }
        let release: GitHubRelease = response
            .json()
            .await
            .map_err(|error| format!("parse GitHub release response: {error}"))?;
        if release.tag_name != pinned_release {
            return Err(format!(
                "GitHub release tag mismatch: expected `{pinned_release}`, got `{}`",
                release.tag_name
            ));
        }
        let release_asset = release
            .assets
            .iter()
            .find(|candidate| candidate.name == asset)
            .ok_or_else(|| format!("release `{pinned_release}` has no exact asset `{asset}`"))?;
        let download_url = reqwest::Url::parse(&release_asset.browser_download_url)
            .map_err(|error| format!("invalid asset download URL: {error}"))?;
        // Never forward the GitHub API bearer token to a URL selected by the
        // release response.
        let response = self
            .download_request(download_url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "GitHub asset download returned {}",
                response.status()
            ));
        }
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| format!("create download directory: {error}"))?;
        }
        let mut file = tokio::fs::File::create(destination)
            .await
            .map_err(|error| format!("create download destination: {error}"))?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| error.to_string())?;
            file.write_all(&chunk)
                .await
                .map_err(|error| format!("write downloaded asset: {error}"))?;
        }
        file.flush()
            .await
            .map_err(|error| format!("flush downloaded asset: {error}"))?;
        file.sync_all()
            .await
            .map_err(|error| format!("sync downloaded asset: {error}"))?;
        Ok(())
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

impl TruthSourceIndexer for IlspycmdTruthIndexer {
    fn tool_versions(&self) -> Result<BTreeMap<String, String>, String> {
        let output = Command::new(&self.executable)
            .arg("--version")
            .output()
            .map_err(|error| format!("run ilspycmd --version: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "ilspycmd --version exited with code {}",
                output.status.code().unwrap_or(-1)
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let version = stdout
            .lines()
            .chain(stderr.lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
            .ok_or_else(|| "ilspycmd --version returned no version text".to_string())?;
        Ok(BTreeMap::from([("ilspycmd".into(), version.into())]))
    }

    fn index(&self, indexer: &str, source: &Path, output_dir: &Path) -> Result<(), String> {
        match indexer {
            "dotnet_project" => run_decompile_project(&self.executable, source, output_dir)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            "dotnet_file" => {
                run_decompile_file(&self.executable, source, &output_dir.join("decompiled.cs"))
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
            other => Err(format!("unsupported truth indexer `{other}`")),
        }
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
        validate_truth_source_inputs(pack, local_inputs)?;
        let _refresh_lock = acquire_refresh_lock(store, pack)?;

        let indexer = Arc::clone(&self.indexer);
        let tool_versions = tokio::task::spawn_blocking(move || indexer.tool_versions())
            .await
            .map_err(|error| TruthSnapshotRefreshError::Worker(error.to_string()))?
            .map_err(TruthSnapshotRefreshError::Tool)?;

        let mut warnings = Vec::new();
        let current = match store.open_current(pack) {
            Ok(current) => current,
            Err(error) => {
                warnings.push(format!(
                    "existing current snapshot is invalid and will not be reused: {error}"
                ));
                None
            }
        };
        if !force
            && let Some(current) = &current
            && current_matches_inputs(current, pack, local_inputs, &tool_versions)?
        {
            return Ok(outcome(current, true, warnings));
        }

        let mut draft = store.begin(pack)?;
        let mut staged_sources = BTreeMap::new();
        for declaration in &pack.truth_sources {
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
                            .fetch(repository, pinned_release, asset, &destination)
                            .await
                            .map_err(|message| TruthSnapshotRefreshError::Fetch {
                                source_id: declaration.id.clone(),
                                message,
                            })?;
                        destination
                    }
                }
            };
            let staged = draft.stage_source(&declaration.id, &source_path)?;
            staged_sources.insert(declaration.id.clone(), staged);
        }

        let fetch_root = draft.staging_root().join(".fetch");
        if fetch_root.exists() {
            fs::remove_dir_all(&fetch_root).map_err(|source| TruthSnapshotRefreshError::Io {
                action: "remove staged truth source download",
                path: fetch_root,
                source,
            })?;
        }

        for declaration in &pack.truth_sources {
            let source = staged_sources[&declaration.id].clone();
            let output = draft.index_output_dir(&declaration.id)?;
            let source_id = declaration.id.clone();
            let indexer_name = declaration.indexer.clone();
            let indexer = Arc::clone(&self.indexer);
            tokio::task::spawn_blocking(move || indexer.index(&indexer_name, &source, &output))
                .await
                .map_err(|error| TruthSnapshotRefreshError::Worker(error.to_string()))?
                .map_err(|message| TruthSnapshotRefreshError::Index {
                    source_id,
                    indexer: declaration.indexer.clone(),
                    message,
                })?;
        }

        let verified = tokio::task::spawn_blocking(move || draft.finalize(tool_versions))
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
) -> TruthSnapshotRefreshResult<bool> {
    if &current.manifest().tool_versions != tool_versions {
        return Ok(false);
    }
    for declaration in &pack.truth_sources {
        if let TruthSourceKind::LocalFile { input_key } = &declaration.kind {
            let digest = digest_file(&local_inputs[input_key])?;
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
    use std::sync::atomic::{AtomicUsize, Ordering};

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::game_pack::{GamePackLoadPolicy, GamePackLoader};

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
        ) -> Result<(), String> {
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

    impl TruthSourceIndexer for MockIndexer {
        fn tool_versions(&self) -> Result<BTreeMap<String, String>, String> {
            Ok(BTreeMap::from([("ilspycmd".into(), self.version.clone())]))
        }

        fn index(&self, indexer: &str, source: &Path, output_dir: &Path) -> Result<(), String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail_on.as_deref() == Some(indexer) {
                return Err("injected index failure".into());
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

    fn inputs(root: &Path, bytes: &[u8]) -> BTreeMap<String, PathBuf> {
        let game = root.join("game.dll");
        fs::write(&game, bytes).unwrap();
        BTreeMap::from([("game_assembly".into(), game)])
    }

    fn refresher(fetcher: Arc<MockFetcher>, indexer: Arc<MockIndexer>) -> TruthSnapshotRefresher {
        TruthSnapshotRefresher::new(fetcher, indexer)
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
