//! ats-core — AgentTheSpire 业务逻辑核心
//!
//! 不依赖任何具体 IO 框架（axum、tauri）。两个壳（`ats-web`、`src-tauri`）
//! 通过 import 本 crate 复用同一份领域逻辑。

pub mod codegen;
pub mod config;
pub mod errors;
pub mod health;
pub mod knowledge;
pub mod llm;
pub mod planning;
pub mod project;
pub mod prompting;

/// 当前 core 版本号（从 Cargo metadata 注入）。
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
