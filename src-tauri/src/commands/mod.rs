//! Tauri IPC command 集合。
//!
//! 每个子模块对标一个或一组 `backend/routers/*.py`，调用同一份 `ats-core` 服务。
//! Fallible commands reject with the shared structured `ActionableFailure` shape.

pub mod capabilities;
pub mod codegen;
pub mod failure;
pub mod health;
pub mod image_proc_state;
pub mod knowledge;
pub mod llm;
pub mod mod_analyzer;
pub mod plan_artifact;
pub mod planning;
pub mod platform;
pub mod project;
pub mod settings;
