//! Planning module — Mod 规划领域模型 + 纯函数算法。
//!
//! Stage 2.3 范围：
//! - 领域类型（`PlanItem`、`ModPlan`、`AssetItemType`、`ReviewStrictness`）
//! - 拓扑排序 + 依赖连通分量
//! - 计划校验（id/type/name/dep 完整性、strictness 分级）
//! - JSON 反序列化（parse_plan）
//!
//! 不在本阶段：
//! - LLM-driven 规划（build_planner_prompt / plan_mod）——等 LLM 模块就位
//! - execution_bundles（拆分到 stage 2.3.2，依赖本模块的 find_groups）
//! - prompt 装配（依赖 knowledge::sts2 一族 provider，后续阶段）

mod dependency_graph;
mod models;
mod parse;
mod validation;

pub use dependency_graph::{find_groups, topological_sort};
pub use models::{AssetItemType, ModPlan, PlanItem};
pub use parse::{ParseError, parse_plan};
pub use validation::{
    PlanItemReviewStatus, PlanItemValidation, PlanValidationIssue, PlanValidationResult,
    ReviewStrictness, validate_plan,
};
