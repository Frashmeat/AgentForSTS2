//! ats-core — AgentTheSpire 业务逻辑核心
//!
//! 不依赖任何具体 IO 框架（axum、tauri）。两个壳（`ats-web`、`src-tauri`）
//! 通过 import 本 crate 复用同一份领域逻辑。

pub mod cancellation;
pub mod capabilities;
pub mod codegen;
pub mod config;
pub mod controlled_process;
pub mod errors;
pub mod failure;
pub mod fs_atomic;
pub mod game_pack;
pub mod health;
pub mod image_gen;
pub mod image_proc;
pub mod knowledge;
pub mod llm;
pub mod mod_analyzer;
pub mod plan_artifact;
pub mod planning;
pub mod platform;
pub mod project;
pub mod project_utils;
pub mod prompting;
pub mod toolchain;

/// 当前 core 版本号（从 Cargo metadata 注入）。
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
