//! Tauri IPC command 集合。
//!
//! 每个子模块对标一个或一组 `backend/routers/*.py`，调用同一份 `ats-core` 服务。
//! 错误统一转 `Result<T, String>`（Tauri 前端只能拿到字符串错误）。

pub mod codegen;
pub mod health;
pub mod knowledge;
pub mod llm;
pub mod planning;
pub mod project;
