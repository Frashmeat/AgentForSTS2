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

    /// 原子「读-改-写」：实现方在内部串行化下读取当前 job，交给 `apply` 决定改不改。
    /// `apply` 原地修改 job 并返回 `true` 表示落盘、`false` 表示放弃（不写）。
    /// 返回最终的 job（可能被改、也可能原样）。
    ///
    /// 这是状态机 CAS 原语：让「仅当未处于终态时才落终态」成为原子操作，避免
    /// cancel 与 handler 收尾各自 get→update 时互相覆盖（取消被悄悄复活成 Completed）。
    async fn modify(
        &self,
        id: &JobId,
        apply: Box<dyn for<'a> FnOnce(&'a mut Job) -> bool + Send>,
    ) -> JobResult<Job>;

    /// 按 id 读取完整 Job。
    async fn get(&self, id: &JobId) -> JobResult<Job>;

    /// 列出全部 Summary（不含 payload/result），按 created_at 倒序。
    async fn list(&self) -> JobResult<Vec<JobSummary>>;

    /// 删除单个任务（含其历史文件）。
    async fn delete(&self, id: &JobId) -> JobResult<()>;
}
