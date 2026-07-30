//! Knowledge module — STS2 游戏代码/BaseLib 反编译产物 + 提示词模板的运行时状态。
//!
//! 已覆盖：
//! - 模板内嵌（8 个 sts2 模板 md 通过 include_str! 打进二进制）
//! - 知识库目录布局（runtime/knowledge/{game,baselib,resources,cache,packs}）
//! - 状态报告（哪些反编译产物存在 / 缺失，警告列表）
//! - **Stage 2.2.1**：ilspycmd 子进程发现 + 反编译调度 + manifest 持久化
//! - STS2 代码事实检索，以及 legacy / Truth Snapshot 事实选择等价性
//!
//! 不在本阶段范围：
//! - Prompt / Evidence 生产链切换到 `VerifiedGameContext`

mod baselib;
mod contracts;
mod decompile;
mod manifest;
mod models;
pub mod pack;
mod paths;
pub mod runtime;
mod sts2_code_facts_provider;
mod sts2_guidance;
mod sts2_guidance_provider;
mod sts2_knowledge_resolver;
mod sts2_lookup_provider;
mod templates;
#[cfg(test)]
pub(crate) mod test_support;

pub use baselib::{BaselibError, BaselibSource, FetchedBaselib, GitHubBaselibSource};
pub use contracts::{
    KnowledgeFactItem, KnowledgeGuidanceItem, KnowledgeLookupItem, KnowledgePacket, KnowledgeQuery,
    KnowledgeScenario,
};
pub use decompile::{
    DecompileError, DecompileStats, default_dotnet_tools_dirs, discover_ilspycmd,
    discover_ilspycmd_in, run_decompile_file, run_decompile_project,
};
pub use manifest::{
    DecompileRecord, KnowledgeManifest, build_record, build_record_with_tag, read_manifest,
    write_manifest,
};
pub use models::{BaselibStatus, GameStatus, KnowledgeStatus, OverallState, SourceMode};
pub use paths::KnowledgePaths;
pub use runtime::{detect_source_mode, ensure_dirs, get_status};
pub use sts2_code_facts_provider::{SnapshotCodeFactsError, Sts2CodeFactsProvider};
pub use sts2_guidance::{guidance_for_asset_type, planner_guidance};
pub use sts2_guidance_provider::Sts2GuidanceProvider;
pub use sts2_knowledge_resolver::Sts2KnowledgeResolver;
pub use sts2_lookup_provider::Sts2LookupProvider;
pub use templates::{TEMPLATE_SLOTS, get_template};
