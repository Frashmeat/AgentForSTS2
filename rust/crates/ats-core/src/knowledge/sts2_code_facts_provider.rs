//! STS2 代码事实抽取（stub）。
//!
//! Python 端 `Sts2CodeFactsProvider` 走 ilspycmd 反编译后的 .cs 源，按 query 抽取
//! 类型/符号/相关代码片段。完整实现强依赖：
//! - 知识库动态更新链路（stage 2.2，需 ilspycmd 子进程编排）
//! - 反编译产物存在于 `runtime/knowledge/game/`
//!
//! 当前阶段只暴露占位接口，确保 codegen / resolver 拼装链路完整。当反编译产物
//! 缺失时返回空 facts + 一条 warning；产物就绪后再充实算法。

use crate::knowledge::contracts::{KnowledgeFactItem, KnowledgeQuery};
use crate::knowledge::models::SourceMode;
use crate::knowledge::paths::KnowledgePaths;

#[derive(Debug, Default, Clone)]
pub struct Sts2CodeFactsProvider;

impl Sts2CodeFactsProvider {
    /// 返回 (facts, warnings)。
    pub fn build_facts(
        &self,
        _query: &KnowledgeQuery,
        _paths: &KnowledgePaths,
        game_mode: SourceMode,
    ) -> (Vec<KnowledgeFactItem>, Vec<String>) {
        match game_mode {
            SourceMode::RuntimeDecompiled => {
                // TODO(stage 2.2)：走 .cs 源抽取，参考 Python `Sts2CodeFactsProvider.build_facts`。
                (
                    Vec::new(),
                    vec![
                        "代码事实抽取尚未在 Rust 端实现（stage 2.2 待办）；当前 prompt 中将缺少类型/符号事实。".into(),
                    ],
                )
            }
            SourceMode::Missing => (
                Vec::new(),
                vec![
                    "游戏反编译源缺失，无法抽取代码事实；先在知识库面板执行更新。".into(),
                ],
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn missing_source_emits_warning() {
        let paths = KnowledgePaths::from_runtime_dir(Path::new("/tmp/runtime"));
        let (facts, warnings) = Sts2CodeFactsProvider.build_facts(
            &KnowledgeQuery::default(),
            &paths,
            SourceMode::Missing,
        );
        assert!(facts.is_empty());
        assert_eq!(warnings.len(), 1);
    }
}
