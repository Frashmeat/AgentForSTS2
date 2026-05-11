//! Platform 模块 —— 任务（Job）调度的 DDD 实现。
//!
//! 双轨设计（决议见 docs/03-方案/全栈重写/进行中/2026-05-11-Rust重写后续执行计划.md §2.3）：
//! - Web 轨：sqlx 走 Postgres，repository 实现见 ats-web crate（stage 3.1a 之后）
//! - Desktop 轨：文件存储，repository 实现见 `infra::file`，落到工程目录 `history/`
//!
//! 共享：contracts + domain trait + application services 全部在本 crate。
//!
//! 当前 stage 3.2 + 3.3b（desktop）+ 3.4 部分范围：
//! - Job/JobKind/JobStatus 等领域类型
//! - JobRepository async trait
//! - FileJobRepository 文件实现
//! - JobApplicationService submit/get/list/cancel
//! - text_generate kind 的 handler 落到 LLM
//!
//! 不在本阶段范围：
//! - ServerCredential / ExecutionRouting / Artifact 等 Web 专属仓库
//! - sqlx 实现（Q3 决议后再做）
//! - 其它 6 个 handler（code/asset/batch/build/package/single/log）

pub mod application;
pub mod contracts;
pub mod domain;
pub mod infra;

pub use application::{JobApplicationService, ProgressEvent, ProgressSink, NoopProgressSink};
pub use contracts::{SubmitTextGenerateRequest, SubmitJobAck};
pub use domain::{
    Job, JobError, JobId, JobKind, JobProgress, JobRepository, JobResult, JobStatus, JobSummary,
};
pub use infra::FileJobRepository;
