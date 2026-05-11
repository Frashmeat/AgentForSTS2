//! Knowledge module — STS2 游戏代码/BaseLib 反编译产物 + 提示词模板的运行时状态。
//!
//! Stage 2.1 范围：
//! - 模板内嵌（8 个 sts2 模板 md 通过 include_str! 打进二进制）
//! - 知识库目录布局（runtime/knowledge/{game,baselib,resources,cache,packs}）
//! - 状态报告（哪些反编译产物存在 / 缺失，警告列表）
//!
//! 不在本阶段范围：
//! - ilspycmd 反编译调度（外部进程）
//! - BaseLib GitHub 下载
//! - 后台刷新任务（依赖 job 状态机）
//! - 知识包导出 ZIP

mod contracts;
mod models;
mod paths;
mod runtime;
mod sts2_code_facts_provider;
mod sts2_guidance;
mod sts2_guidance_provider;
mod sts2_knowledge_resolver;
mod sts2_lookup_provider;
mod templates;

pub use contracts::{
    KnowledgeFactItem, KnowledgeGuidanceItem, KnowledgeLookupItem, KnowledgePacket, KnowledgeQuery,
    KnowledgeScenario,
};
pub use models::{BaselibStatus, GameStatus, KnowledgeStatus, OverallState, SourceMode};
pub use paths::KnowledgePaths;
pub use runtime::{ensure_dirs, get_status};
pub use sts2_code_facts_provider::Sts2CodeFactsProvider;
pub use sts2_guidance::{guidance_for_asset_type, planner_guidance};
pub use sts2_guidance_provider::Sts2GuidanceProvider;
pub use sts2_knowledge_resolver::Sts2KnowledgeResolver;
pub use sts2_lookup_provider::Sts2LookupProvider;
pub use templates::{TEMPLATE_SLOTS, get_template};
