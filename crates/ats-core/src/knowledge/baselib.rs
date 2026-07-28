//! BaseLib GitHub Releases 客户端 —— 下载 latest release 中的 BaseLib.dll。
//!
//! 默认源仓库：`Alchyr/BaseLib-StS2`（与 Python 后端一致）。
//!
//! 设计取向：
//! - `BaselibSource` trait 抽离实现，方便单测注入 mock
//! - `GitHubBaselibSource` 是 reqwest 实现，单元测试不命中（只在集成场景跑）
//! - 资产挑选规则：精确 "BaseLib.dll"（忽略大小写）> 任意 *.dll 且名字含 baselib

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct FetchedBaselib {
    /// 写入的本地 .dll 路径
    pub dll_path: PathBuf,
    pub release_tag: String,
    pub asset_name: String,
    pub source_url: String,
    pub bytes: u64,
}

#[derive(Debug, Error)]
pub enum BaselibError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("API responded {status}: {message}")]
    Api { status: u16, message: String },
    #[error("response JSON parse failed: {0}")]
    Parse(String),
    #[error("no suitable BaseLib.dll asset in latest release")]
    NoAsset { available: Vec<String> },
    #[error("write file {0}: {1}")]
    Write(PathBuf, String),
}

#[async_trait]
pub trait BaselibSource: Send + Sync {
    /// 获取最新 BaseLib.dll，保存到 `dest_dir/<asset_name>`，返回元信息。
    async fn fetch_baselib_dll(&self, dest_dir: &Path) -> Result<FetchedBaselib, BaselibError>;
}

/// GitHub Releases 实现。
///
/// API endpoint：`https://api.github.com/repos/<owner>/<repo>/releases/latest`
/// 找到匹配的 .dll asset 后调用其 `browser_download_url` 拉文件。
pub struct GitHubBaselibSource {
    client: reqwest::Client,
    owner: String,
    repo: String,
    /// Optional GitHub token for authenticated API requests (avoids 60 req/h rate limit).
    token: Option<String>,
    #[cfg(feature = "e2e")]
    latest_release_url_override: Option<String>,
}

impl GitHubBaselibSource {
    /// 默认指向 `Alchyr/BaseLib-StS2`，与 Python 后端一致。
    #[must_use]
    pub fn default_alchyr(client: reqwest::Client) -> Self {
        Self {
            client,
            owner: "Alchyr".into(),
            repo: "BaseLib-StS2".into(),
            token: None,
            #[cfg(feature = "e2e")]
            latest_release_url_override: None,
        }
    }

    #[must_use]
    pub fn new(client: reqwest::Client, owner: String, repo: String) -> Self {
        Self {
            client,
            owner,
            repo,
            token: None,
            #[cfg(feature = "e2e")]
            latest_release_url_override: None,
        }
    }

    #[must_use]
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token.filter(|t| !t.is_empty());
        self
    }

    /// 仅供 `e2e` feature 将 GitHub Releases 请求定向到本地确定性 stub。
    #[cfg(feature = "e2e")]
    #[must_use]
    pub fn with_latest_release_url_override(mut self, url: String) -> Self {
        self.latest_release_url_override = Some(url);
        self
    }

    /// 不需要让上层 crate 引入 reqwest：用内置默认 client（60s 超时）。
    /// 失败时返回 `BaselibError::Http`。
    pub fn default_alchyr_with_default_client(token: Option<String>) -> Result<Self, BaselibError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| BaselibError::Http(e.to_string()))?;
        Ok(Self::default_alchyr(client).with_token(token))
    }

    fn latest_release_url(&self) -> String {
        #[cfg(feature = "e2e")]
        if let Some(url) = &self.latest_release_url_override {
            return url.clone();
        }
        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.owner, self.repo
        )
    }
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    name: String,
    browser_download_url: String,
}

#[async_trait]
impl BaselibSource for GitHubBaselibSource {
    async fn fetch_baselib_dll(&self, dest_dir: &Path) -> Result<FetchedBaselib, BaselibError> {
        let mut req = self
            .client
            .get(self.latest_release_url())
            .header("User-Agent", "agentthespire-rust")
            .header("Accept", "application/vnd.github+json");
        if let Some(ref t) = self.token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        let release_resp = req
            .send()
            .await
            .map_err(|e| BaselibError::Http(e.to_string()))?;
        let status = release_resp.status();
        if !status.is_success() {
            let message = release_resp
                .text()
                .await
                .unwrap_or_else(|_| "<no body>".into());
            return Err(BaselibError::Api {
                status: status.as_u16(),
                message: api_error_message(status.as_u16(), &message),
            });
        }
        let release: GitHubRelease = release_resp
            .json()
            .await
            .map_err(|e| BaselibError::Parse(e.to_string()))?;

        let asset = select_baselib_asset(&release.assets).ok_or_else(|| BaselibError::NoAsset {
            available: release.assets.iter().map(|a| a.name.clone()).collect(),
        })?;

        std::fs::create_dir_all(dest_dir)
            .map_err(|e| BaselibError::Write(dest_dir.to_path_buf(), format!("create dir: {e}")))?;
        let dest = dest_dir.join(&asset.name);

        let dl_resp = self
            .client
            .get(&asset.browser_download_url)
            .header("User-Agent", "agentthespire-rust")
            .send()
            .await
            .map_err(|e| BaselibError::Http(e.to_string()))?;
        let dl_status = dl_resp.status();
        if !dl_status.is_success() {
            let message = dl_resp.text().await.unwrap_or_else(|_| "<no body>".into());
            return Err(BaselibError::Api {
                status: dl_status.as_u16(),
                message: api_error_message(dl_status.as_u16(), &message),
            });
        }
        let bytes = dl_resp
            .bytes()
            .await
            .map_err(|e| BaselibError::Http(e.to_string()))?;
        let byte_len = bytes.len() as u64;
        std::fs::write(&dest, &bytes)
            .map_err(|e| BaselibError::Write(dest.clone(), e.to_string()))?;

        Ok(FetchedBaselib {
            dll_path: dest,
            release_tag: release.tag_name,
            asset_name: asset.name.clone(),
            source_url: asset.browser_download_url.clone(),
            bytes: byte_len,
        })
    }
}

/// 资产挑选规则（与 Python 后端一致）：
/// 1. 名字精确等于 "BaseLib.dll"（忽略大小写）优先
/// 2. 否则任意 .dll 文件且文件名含 "baselib"（忽略大小写）
/// 3. 都没有返回 None
fn select_baselib_asset(assets: &[GitHubAsset]) -> Option<&GitHubAsset> {
    let exact = assets
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case("BaseLib.dll"));
    if exact.is_some() {
        return exact;
    }
    assets.iter().find(|a| {
        let lower = a.name.to_ascii_lowercase();
        lower.ends_with(".dll") && lower.contains("baselib")
    })
}

fn api_error_message(status: u16, response_body: &str) -> String {
    let body = response_body.to_ascii_lowercase();
    match status {
        401 => "GitHub token is invalid or expired; update or clear runtime.workstation.github_token and retry.".into(),
        403 if body.contains("rate limit") || body.contains("rate_limit") => {
            "GitHub API rate limit exceeded; configure a valid GitHub token in runtime.workstation.github_token or wait for the limit to reset.".into()
        }
        429 => "GitHub API rate limit exceeded; configure a valid GitHub token in runtime.workstation.github_token or wait for the limit to reset.".into(),
        403 => "GitHub request was forbidden; check GitHub token permissions in runtime.workstation.github_token and retry.".into(),
        _ => "GitHub request failed; retry later and check the GitHub service status or network connection.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_asset(name: &str) -> GitHubAsset {
        GitHubAsset {
            name: name.into(),
            browser_download_url: format!("https://example.invalid/{name}"),
        }
    }

    #[test]
    fn select_prefers_exact_baselib_dll() {
        let assets = vec![
            fake_asset("README.md"),
            fake_asset("BaseLib-extras.dll"),
            fake_asset("BaseLib.dll"),
        ];
        let picked = select_baselib_asset(&assets).unwrap();
        assert_eq!(picked.name, "BaseLib.dll");
    }

    #[test]
    fn select_falls_back_to_any_baselib_dll() {
        let assets = vec![
            fake_asset("README.md"),
            fake_asset("BaseLibStS2-v0.5.dll"),
            fake_asset("notes.txt"),
        ];
        let picked = select_baselib_asset(&assets).unwrap();
        assert_eq!(picked.name, "BaseLibStS2-v0.5.dll");
    }

    #[test]
    fn select_case_insensitive() {
        let assets = vec![fake_asset("baselib.DLL")];
        let picked = select_baselib_asset(&assets).unwrap();
        assert_eq!(picked.name, "baselib.DLL");
    }

    #[test]
    fn select_returns_none_when_no_dll() {
        let assets = vec![fake_asset("README.md"), fake_asset("source.zip")];
        assert!(select_baselib_asset(&assets).is_none());
    }

    #[test]
    fn select_skips_non_baselib_dlls() {
        let assets = vec![fake_asset("Other.dll"), fake_asset("Helper.dll")];
        assert!(select_baselib_asset(&assets).is_none());
    }

    #[test]
    fn latest_release_url_uses_default_repo() {
        let s = GitHubBaselibSource::default_alchyr(reqwest::Client::new());
        assert_eq!(
            s.latest_release_url(),
            "https://api.github.com/repos/Alchyr/BaseLib-StS2/releases/latest"
        );
    }

    #[test]
    fn api_error_explains_invalid_or_expired_token() {
        let message = api_error_message(
            401,
            r#"{\"message\":\"Bad credentials\",\"token\":\"secret\"}"#,
        );
        assert!(message.contains("invalid or expired"));
        assert!(message.contains("runtime.workstation.github_token"));
        assert!(!message.contains("secret"));
        assert!(!message.contains("Bad credentials"));
    }

    #[test]
    fn api_error_explains_rate_limit_and_forbidden_permissions() {
        let rate_limit = api_error_message(
            403,
            r#"{\"message\":\"API rate limit exceeded for 203.0.113.1\"}"#,
        );
        assert!(rate_limit.contains("rate limit"));
        assert!(rate_limit.contains("valid GitHub token"));
        assert!(!rate_limit.contains("203.0.113.1"));

        let forbidden = api_error_message(403, r#"{\"message\":\"Resource not accessible\"}"#);
        assert!(forbidden.contains("permissions"));
        assert!(forbidden.contains("GitHub token"));

        let throttled = api_error_message(429, "arbitrary upstream body");
        assert!(throttled.contains("rate limit"));
        assert!(!throttled.contains("upstream body"));
    }
}
