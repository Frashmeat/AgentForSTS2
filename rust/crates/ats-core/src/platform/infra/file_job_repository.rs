//! 基于工程文件夹 `history/` 的 JSON 文件 Job 仓库。
//!
//! 文件命名：`<created_at_iso8601-basic>--<job_id>.json`，按文件名排序就是按时间。
//! 写入原子：先写 `.tmp` 再 rename。
//! list() 走 `tokio::fs::read_dir`，逐文件读 Summary 字段（不解 payload/result）。

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::platform::domain::{
    Job, JobError, JobId, JobRepository, JobResult, JobSummary,
};

const FILE_SUFFIX: &str = ".json";
const TMP_SUFFIX: &str = ".tmp";

#[derive(Debug, Clone)]
pub struct FileJobRepository {
    root: PathBuf,
}

impl FileJobRepository {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn job_path(&self, id: &JobId, created_at: chrono::DateTime<chrono::Utc>) -> PathBuf {
        let stamp = created_at.format("%Y-%m-%dT%H-%M-%S%.3f").to_string();
        self.root.join(format!("{stamp}--{}{}", id.0, FILE_SUFFIX))
    }

    /// 在 root 下扫描所有 .json，根据 id 匹配返回路径（不读内容）。
    /// 用于 update / get / delete 时定位文件——文件名前缀含时间戳，无法只凭 id 推算路径。
    async fn locate(&self, id: &JobId) -> JobResult<Option<PathBuf>> {
        let mut entries = match fs::read_dir(&self.root).await {
            Ok(e) => e,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let suffix = format!("--{}{}", id.0, FILE_SUFFIX);
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.ends_with(&suffix) {
                return Ok(Some(entry.path()));
            }
        }
        Ok(None)
    }

    async fn ensure_root(&self) -> JobResult<()> {
        fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    async fn write_atomic(path: &Path, bytes: &[u8]) -> JobResult<()> {
        let tmp = path.with_extension(format!(
            "{}{}",
            path.extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_else(|| "json".into()),
            TMP_SUFFIX
        ));
        {
            let mut f = fs::File::create(&tmp).await?;
            f.write_all(bytes).await?;
            f.flush().await?;
        }
        fs::rename(&tmp, path).await?;
        Ok(())
    }
}

#[async_trait]
impl JobRepository for FileJobRepository {
    async fn create(&self, job: &Job) -> JobResult<()> {
        self.ensure_root().await?;
        if self.locate(&job.id).await?.is_some() {
            return Err(JobError::AlreadyExists(job.id.0.clone()));
        }
        let path = self.job_path(&job.id, job.created_at);
        let bytes = serde_json::to_vec_pretty(job)?;
        Self::write_atomic(&path, &bytes).await
    }

    async fn update(&self, job: &Job) -> JobResult<()> {
        let path = self
            .locate(&job.id)
            .await?
            .ok_or_else(|| JobError::NotFound(job.id.0.clone()))?;
        let bytes = serde_json::to_vec_pretty(job)?;
        Self::write_atomic(&path, &bytes).await
    }

    async fn get(&self, id: &JobId) -> JobResult<Job> {
        let path = self
            .locate(id)
            .await?
            .ok_or_else(|| JobError::NotFound(id.0.clone()))?;
        let text = fs::read_to_string(&path).await?;
        Ok(serde_json::from_str(&text)?)
    }

    async fn list(&self) -> JobResult<Vec<JobSummary>> {
        let mut entries = match fs::read_dir(&self.root).await {
            Ok(e) => e,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err.into()),
        };
        let mut paths: Vec<PathBuf> = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("json"))
            {
                paths.push(path);
            }
        }
        // 按文件名字母序倒序（== 时间倒序，因前缀是 ISO 时间）
        paths.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            let text = match fs::read_to_string(&path).await {
                Ok(t) => t,
                Err(_) => continue, // 跳过损坏的文件
            };
            // 解为完整 Job 再转 Summary——简单稳，目录通常 10-100 量级，足够用
            if let Ok(job) = serde_json::from_str::<Job>(&text) {
                out.push(JobSummary::from(&job));
            }
        }
        Ok(out)
    }

    async fn delete(&self, id: &JobId) -> JobResult<()> {
        let Some(path) = self.locate(id).await? else {
            return Err(JobError::NotFound(id.0.clone()));
        };
        fs::remove_file(path).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::domain::{Job, JobKind, JobStatus};

    async fn make_repo() -> (tempfile::TempDir, FileJobRepository) {
        let td = tempfile::TempDir::new().unwrap();
        let repo = FileJobRepository::new(td.path().join("history"));
        (td, repo)
    }

    fn sample_job() -> Job {
        Job::new(JobKind::TextGenerate, serde_json::json!({"prompt": "hi"}))
    }

    #[tokio::test]
    async fn create_get_round_trip() {
        let (_td, repo) = make_repo().await;
        let job = sample_job();
        repo.create(&job).await.unwrap();
        let got = repo.get(&job.id).await.unwrap();
        assert_eq!(got.id, job.id);
        assert_eq!(got.kind, JobKind::TextGenerate);
        assert_eq!(got.status, JobStatus::Pending);
    }

    #[tokio::test]
    async fn create_duplicate_errors() {
        let (_td, repo) = make_repo().await;
        let job = sample_job();
        repo.create(&job).await.unwrap();
        let err = repo.create(&job).await.unwrap_err();
        assert!(matches!(err, JobError::AlreadyExists(_)));
    }

    #[tokio::test]
    async fn update_persists_status_change() {
        let (_td, repo) = make_repo().await;
        let mut job = sample_job();
        repo.create(&job).await.unwrap();
        job.status = JobStatus::Running;
        job.started_at = Some(chrono::Utc::now());
        repo.update(&job).await.unwrap();
        let reloaded = repo.get(&job.id).await.unwrap();
        assert_eq!(reloaded.status, JobStatus::Running);
        assert!(reloaded.started_at.is_some());
    }

    #[tokio::test]
    async fn list_sorted_descending_by_time() {
        let (_td, repo) = make_repo().await;
        let mut j1 = sample_job();
        j1.created_at = chrono::Utc::now() - chrono::Duration::seconds(60);
        let j2 = sample_job();
        repo.create(&j1).await.unwrap();
        repo.create(&j2).await.unwrap();
        let list = repo.list().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, j2.id); // 新的在前
        assert_eq!(list[1].id, j1.id);
    }

    #[tokio::test]
    async fn delete_removes_file() {
        let (_td, repo) = make_repo().await;
        let job = sample_job();
        repo.create(&job).await.unwrap();
        repo.delete(&job.id).await.unwrap();
        let err = repo.get(&job.id).await.unwrap_err();
        assert!(matches!(err, JobError::NotFound(_)));
    }

    #[tokio::test]
    async fn list_empty_when_root_missing() {
        let td = tempfile::TempDir::new().unwrap();
        let repo = FileJobRepository::new(td.path().join("does-not-exist"));
        let list = repo.list().await.unwrap();
        assert!(list.is_empty());
    }
}
