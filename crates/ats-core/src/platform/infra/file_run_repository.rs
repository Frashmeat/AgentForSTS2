//! JSON-backed Run repository stored under a project's `history/` directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::platform::domain::{
    RunError, RunId, RunProgress, RunRecord, RunRepository, RunRepositoryResult, RunSummary,
    RunTransition,
};

const FILE_SUFFIX: &str = ".json";
const TMP_SUFFIX: &str = ".tmp";

#[derive(Debug, Clone)]
pub struct FileRunRepository {
    root: PathBuf,
    write_lock: Arc<tokio::sync::Mutex<()>>,
}

impl FileRunRepository {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            write_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn run_path(&self, run: &RunRecord) -> PathBuf {
        let stamp = run.created_at.format("%Y-%m-%dT%H-%M-%S%.3f");
        self.root
            .join(format!("{stamp}--{}{}", run.id, FILE_SUFFIX))
    }

    async fn locate(&self, id: &RunId) -> RunRepositoryResult<Option<PathBuf>> {
        let mut entries = match fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let suffix = format!("--{id}{FILE_SUFFIX}");
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_name().to_string_lossy().ends_with(&suffix) {
                return Ok(Some(entry.path()));
            }
        }
        Ok(None)
    }

    async fn read_record(path: &Path) -> RunRepositoryResult<RunRecord> {
        let text = fs::read_to_string(path).await?;
        let run: RunRecord = serde_json::from_str(&text)?;
        run.validate().map_err(|message| RunError::InvalidRecord {
            id: run.id.0.clone(),
            message,
        })?;
        Ok(run)
    }

    async fn write_record(path: &Path, run: &RunRecord) -> RunRepositoryResult<()> {
        run.validate().map_err(|message| RunError::InvalidRecord {
            id: run.id.0.clone(),
            message,
        })?;
        let bytes = serde_json::to_vec_pretty(run)?;
        let tmp = path.with_extension(format!(
            "{}{}",
            path.extension()
                .map(|extension| extension.to_string_lossy().into_owned())
                .unwrap_or_else(|| "json".into()),
            TMP_SUFFIX
        ));
        {
            let mut file = fs::File::create(&tmp).await?;
            file.write_all(&bytes).await?;
            file.flush().await?;
        }
        fs::rename(&tmp, path).await?;
        Ok(())
    }

    async fn load_for_write(&self, id: &RunId) -> RunRepositoryResult<(PathBuf, RunRecord)> {
        let path = self
            .locate(id)
            .await?
            .ok_or_else(|| RunError::NotFound(id.0.clone()))?;
        let run = Self::read_record(&path).await?;
        Ok((path, run))
    }
}

#[async_trait]
impl RunRepository for FileRunRepository {
    async fn create(&self, run: &RunRecord) -> RunRepositoryResult<()> {
        let _guard = self.write_lock.lock().await;
        fs::create_dir_all(&self.root).await?;
        if self.locate(&run.id).await?.is_some() {
            return Err(RunError::AlreadyExists(run.id.0.clone()));
        }
        Self::write_record(&self.run_path(run), run).await
    }

    async fn transition(
        &self,
        id: &RunId,
        transition: RunTransition,
    ) -> RunRepositoryResult<RunRecord> {
        let _guard = self.write_lock.lock().await;
        let (path, mut run) = self.load_for_write(id).await?;
        run.apply_transition(transition, Utc::now())
            .map_err(|message| RunError::InvalidTransition {
                id: id.0.clone(),
                message,
            })?;
        Self::write_record(&path, &run).await?;
        Ok(run)
    }

    async fn update_progress(
        &self,
        id: &RunId,
        progress: RunProgress,
    ) -> RunRepositoryResult<RunRecord> {
        let _guard = self.write_lock.lock().await;
        let (path, mut run) = self.load_for_write(id).await?;
        run.set_progress(progress)
            .map_err(|message| RunError::InvalidTransition {
                id: id.0.clone(),
                message,
            })?;
        Self::write_record(&path, &run).await?;
        Ok(run)
    }

    async fn get(&self, id: &RunId) -> RunRepositoryResult<RunRecord> {
        let path = self
            .locate(id)
            .await?
            .ok_or_else(|| RunError::NotFound(id.0.clone()))?;
        Self::read_record(&path).await
    }

    async fn list(&self) -> RunRepositoryResult<Vec<RunSummary>> {
        let mut entries = match fs::read_dir(&self.root).await {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut paths = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
            {
                paths.push(path);
            }
        }
        paths.sort_by(|left, right| right.file_name().cmp(&left.file_name()));

        let mut summaries = Vec::with_capacity(paths.len());
        for path in paths {
            let run = Self::read_record(&path).await?;
            summaries.push(RunSummary::from(&run));
        }
        Ok(summaries)
    }

    async fn delete(&self, id: &RunId) -> RunRepositoryResult<()> {
        let _guard = self.write_lock.lock().await;
        let path = self
            .locate(id)
            .await?
            .ok_or_else(|| RunError::NotFound(id.0.clone()))?;
        fs::remove_file(path).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::domain::{CancellationReason, RunKind, RunStatus, RunTimelineEventKind};

    fn sample_run() -> RunRecord {
        RunRecord::new(RunKind::TextGenerate, serde_json::json!({"prompt": "hi"}))
    }

    #[tokio::test]
    async fn transition_and_progress_round_trip() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo = FileRunRepository::new(temp.path().join("history"));
        let run = sample_run();
        repo.create(&run).await.unwrap();
        repo.transition(&run.id, RunTransition::Start)
            .await
            .unwrap();
        repo.update_progress(
            &run.id,
            RunProgress {
                stage: "stream".into(),
                percent: Some(0.5),
                message: None,
            },
        )
        .await
        .unwrap();

        let reloaded = repo.get(&run.id).await.unwrap();
        assert_eq!(reloaded.status, RunStatus::Running);
        assert_eq!(reloaded.timeline.len(), 2);
        assert_eq!(reloaded.progress.unwrap().stage, "stream");
    }

    #[tokio::test]
    async fn terminal_transition_is_compare_and_swap() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo = FileRunRepository::new(temp.path().join("history"));
        let run = sample_run();
        repo.create(&run).await.unwrap();
        repo.transition(
            &run.id,
            RunTransition::Cancel {
                reason: CancellationReason::User,
            },
        )
        .await
        .unwrap();

        assert!(
            repo.transition(&run.id, RunTransition::Start)
                .await
                .is_err()
        );
        let reloaded = repo.get(&run.id).await.unwrap();
        assert_eq!(reloaded.status, RunStatus::Cancelled);
        assert_eq!(
            reloaded
                .timeline
                .iter()
                .filter(|event| event.kind.is_terminal())
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn invalid_record_is_rejected_on_read() {
        let temp = tempfile::TempDir::new().unwrap();
        let root = temp.path().join("history");
        let repo = FileRunRepository::new(root.clone());
        let run = sample_run();
        repo.create(&run).await.unwrap();
        let path = repo.locate(&run.id).await.unwrap().unwrap();
        let mut value = serde_json::to_value(&run).unwrap();
        value["timeline"] = serde_json::json!([{
            "kind": "succeeded",
            "at": Utc::now()
        }]);
        fs::write(path, serde_json::to_vec_pretty(&value).unwrap())
            .await
            .unwrap();

        assert!(matches!(
            repo.get(&run.id).await,
            Err(RunError::InvalidRecord { .. })
        ));
    }

    #[tokio::test]
    async fn list_is_newest_first() {
        let temp = tempfile::TempDir::new().unwrap();
        let repo = FileRunRepository::new(temp.path().join("history"));
        let mut older = sample_run();
        older.created_at -= chrono::Duration::minutes(1);
        older.timeline[0].at = older.created_at;
        let newer = sample_run();
        repo.create(&older).await.unwrap();
        repo.create(&newer).await.unwrap();

        let list = repo.list().await.unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, newer.id);
        assert_eq!(list[1].id, older.id);
        assert_eq!(older.timeline[0].kind, RunTimelineEventKind::Created);
    }
}
