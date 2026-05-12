//! Platform 基础设施：repository 的具体实现。
//!
//! 当前只含 `FileJobRepository`（Desktop 轨）。Web 轨的 sqlx 实现会进入
//! ats-web crate 而非这里——把 Postgres 依赖与 ats-core 解耦，方便 Desktop
//! 单独构建。

mod audited_repository;
mod file_job_repository;

pub use audited_repository::AuditedJobRepository;
pub use file_job_repository::FileJobRepository;
