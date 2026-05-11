//! JobRepository trait —— Web sqlx 与 Desktop file 双实现各自满足同一契约。

use async_trait::async_trait;

use super::errors::JobResult;
use super::models::{Job, JobId, JobSummary};

#[async_trait]
pub trait JobRepository: Send + Sync {
    /// 持久化一个新建任务；id 已存在时返回 `JobError::AlreadyExists`。
    async fn create(&self, job: &Job) -> JobResult<()>;

    /// 覆盖式更新整个 job；id 不存在时返回 `JobError::NotFound`。
    async fn update(&self, job: &Job) -> JobResult<()>;

    /// 按 id 读取完整 Job。
    async fn get(&self, id: &JobId) -> JobResult<Job>;

    /// 列出全部 Summary（不含 payload/result），按 created_at 倒序。
    async fn list(&self) -> JobResult<Vec<JobSummary>>;

    /// 删除单个任务（含其历史文件）。
    async fn delete(&self, id: &JobId) -> JobResult<()>;
}
