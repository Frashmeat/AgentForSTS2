//! Platform application 层：use case 服务。
//!
//! - JobApplicationService 管理 Job 生命周期（submit/get/list/cancel），spawn 后台
//!   tokio 任务调用 handlers 子模块。
//! - handlers/ 子目录按 JobKind 切分；每个 handler 自带单测。
//!
//! 后续 stage 引入 Handler trait 注册表后会再做一次切分，当前形状保留对 service
//! 直接依赖以减小改动面。

pub mod handlers;
mod job_application_service;

pub use handlers::{NoopProgressSink, ProgressEvent, ProgressSink};
pub use job_application_service::JobApplicationService;
