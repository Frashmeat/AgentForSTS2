//! 共享 prompt 装配 + 模板加载。
//!
//! 镜像 Python `backend/app/shared/prompting/`。两块：
//! - `PromptLoader` 提供 `{{ var }}` 变量替换 + bundle 分段（`## section_key`）
//! - `PromptContextAssembler` 把 `KnowledgePacket` 渲染成 5 段字符串
//!   供下游 codegen / planner 直接塞进 prompt 模板。

mod assembler;
mod loader;

pub use assembler::PromptContextAssembler;
pub use loader::{PromptError, PromptLoader};
