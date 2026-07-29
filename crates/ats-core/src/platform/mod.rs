//! Platform 模块 —— 任务（Job）调度的 DDD 实现。
//!
//! 桌面/Web 边界见 `docs/01-总览/项目架构总览.md` 与 `docs/03-当前方案.md`：
//! - Web 轨：sqlx 走 Postgres，repository 实现见 ats-web crate（stage 3.1a 之后）
//! - Desktop 轨：文件存储，repository 实现见 `infra::file`，落到工程目录 `history/`
//!
//! 共享：contracts + domain trait + application services 全部在本 crate。
//!
//! 当前 stage 3.2 + 3.3b（desktop）+ 3.4 部分 + 3.5 第一轮 / 第二轮范围：
//! - Job/JobKind/JobStatus 等领域类型
//! - JobRepository async trait
//! - FileJobRepository 文件实现
//! - JobApplicationService submit/get/list/cancel
//! - handlers/ 子目录：text_generate / code_generate / build_project /
//!   log_analysis / package_project / batch_custom_code / single_asset_plan
//!
//! 不在本阶段范围：
//! - ServerCredential / ExecutionRouting / Artifact 等 Web 专属仓库
//! - sqlx 实现（Q3 决议后再做）
//! - asset_generate handler（依赖 image_gen 客户端）

pub mod application;
pub mod discovery;
pub mod contracts;
pub mod domain;
pub mod infra;

pub use application::{JobApplicationService, NoopProgressSink, ProgressEvent, ProgressSink};
pub use contracts::{
    SubmitAssetGenerateRequest, SubmitBatchCustomCodeRequest, SubmitBuildProjectRequest,
    SubmitCodeGenerateRequest, SubmitJobAck, SubmitKnowledgeRefreshRequest,
    SubmitLogAnalysisRequest, SubmitPackageProjectRequest, SubmitSingleAssetPlanRequest,
    SubmitTextGenerateRequest,
};
pub use domain::{
    Job, JobError, JobId, JobKind, JobProgress, JobRepository, JobResult, JobStatus, JobSummary,
};
pub use infra::{AuditedJobRepository, FileJobRepository};
