//! 配置加载（stage 1 实现）。
//!
//! 设计取向：figment 多源合并（TOML + env + CLI override）。Stage 0 只
//! 占位，避免污染骨架的依赖图。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    // stage 1 填充：runtime_role、cors_origins、database_url、llm provider 等
}

impl Settings {
    /// 占位实现。Stage 1 替换为 figment 多源加载。
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }
}
