//! Platform application 层：use case 服务。
//!
//! 当前 stage 实现：
//! - JobApplicationService 管理 Job 生命周期（submit/get/list/cancel）
//! - text_generate 一类 handler 内嵌（spawn tokio 任务调 LLM 流式生成）
//!
//! 后续 stage 引入 ExecutionOrchestratorService 后再把 handler 抽出。

mod job_application_service;

pub use job_application_service::{
    JobApplicationService, NoopProgressSink, ProgressEvent, ProgressSink,
};
