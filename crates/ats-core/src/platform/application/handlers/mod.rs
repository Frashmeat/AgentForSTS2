//! RunRecord handler 子模块集合。
//!
//! 每个 handler 一个文件，对应 RunKind 的一个 variant。共享类型（ProgressEvent /
//! ProgressSink / 状态迁移 helper）放 common。
//!
//! 设计取向：handler 内部都是自由函数 `run_xxx(...)`，由 RunApplicationService
//! 的 `submit_xxx(...)` spawn。后续 stage 引入 Handler trait 注册表后会再做一次
//! 切分，当前形状保留对 service 直接依赖以减小改动面。

pub(crate) mod asset_bundle;
pub(crate) mod asset_compile;
pub mod asset_generate;
pub mod batch_custom_code;
pub mod build_project;
pub mod code_generate;
pub mod common;
pub mod log_analysis;
pub mod package_project;
pub mod single_asset_plan;
pub mod text_generate;
pub mod truth_snapshot_refresh;

pub use common::{
    NoopProgressSink, ProgressEvent, ProgressSink, emit_cancelled_mid_stream, finalize_with_error,
    is_cancelled, transition_to_running,
};
