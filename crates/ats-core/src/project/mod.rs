//! 工程文件夹模式——桌面端持久化的核心抽象。
//!
//! 决议依据：`docs/03-方案/全栈重写/进行中/2026-05-11-Rust+Tauri全栈重写计划.md`
//! §6.2 Q1，桌面端不用 DB，每个 mod 项目是一个自包含目录，可整体携带 / git / zip。
//!
//! 本模块范围（stage 3.1b 最小可用）：
//! - `AppDataPaths` 解析 OS app data dir 下的 config / credentials / recents / knowledge / logs
//! - `ProjectFolder::create` / `open` 管理工程目录生命周期，含 `.ats/lock` 排他锁
//! - `ProjectMeta` 序列化为 `project.json`
//! - `RecentProjects` 维护"最近打开"LRU 列表
//!
//! 不在本阶段范围（后续 stage 接）：
//! - plan.json / items/* / artifacts/* 业务持久化 → 跟随 platform 模块迁移
//! - credentials.json + OS keyring → stage 5 装配收口前
//! - 跨进程 fs2 文件锁 → 现在用进程内 Mutex + lock 文件存在性检查兜底

mod error;
mod folder;
mod paths;
mod recents;
mod template;

pub use error::{ProjectError, ProjectResult};
pub use folder::{PROJECT_SCHEMA_VERSION, ProjectFolder, ProjectMeta};
pub use paths::AppDataPaths;
pub use recents::{RecentEntry, RecentProjects};
pub use template::{derive_csharp_name, scaffold_from_template};
