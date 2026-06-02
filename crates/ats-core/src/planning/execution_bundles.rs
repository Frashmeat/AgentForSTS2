//! 执行 bundle 推断。镜像 Python `application/execution_bundles.py`。
//!
//! 流程：
//!   1. 用 `find_groups` 算出按依赖关系连通的 item 分组
//!   2. 在每组内部按 `relationship_type` 二次细分（ordered_dependency / independent
//!      不重新合并，其它合并为同一 bundle）
//!   3. 对每个 bundle 评估 risk_codes（耦合度、规模、类型混杂、影响范围）
//!   4. 据此给 status（clear / needs_confirmation / split_recommended）+ 行动建议
//!
//! 用户在前端对每个 bundle 的决策（accept / split_requested 等）通过
//! `bundle_decisions` 回传，下一轮重算时按映射拆分或维持当前分组。

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::dependency_graph::find_groups;
use super::models::{AssetItemType, ModPlan, PlanItem};
use super::validation::ReviewStrictness;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BundleReviewStatus {
    Clear,
    NeedsConfirmation,
    SplitRecommended,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BundleDecision {
    #[default]
    Unresolved,
    Accepted,
    SplitRequested,
    NeedsItemRevision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyGroup {
    pub item_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskDetail {
    pub code: String,
    pub title: String,
    pub summary: String,
    pub recommendation: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedAction {
    pub action: String,
    pub label: String,
    pub description: String,
    pub emphasis: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionBundle {
    pub bundle_id: String,
    pub item_ids: Vec<String>,
    pub status: BundleReviewStatus,
    pub reason: String,
    pub risk_codes: Vec<String>,
    pub risk_details: Vec<RiskDetail>,
    pub recommended_actions: Vec<RecommendedAction>,
    pub blocking_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPlanPreview {
    pub strictness: ReviewStrictness,
    pub dependency_groups: Vec<DependencyGroup>,
    pub execution_bundles: Vec<ExecutionBundle>,
}

pub fn build_execution_plan(
    plan: &ModPlan,
    strictness: ReviewStrictness,
    bundle_decisions: &HashMap<String, BundleDecision>,
) -> ExecutionPlanPreview {
    let dependency_groups = find_groups(&plan.items);
    let dependency_group_models: Vec<DependencyGroup> = dependency_groups
        .iter()
        .map(|group| DependencyGroup {
            item_ids: group.iter().map(|i| i.id.clone()).collect(),
        })
        .collect();

    let mut bundles: Vec<ExecutionBundle> = Vec::new();
    for group in &dependency_groups {
        bundles.extend(build_group_bundles(group, strictness, bundle_decisions));
    }

    ExecutionPlanPreview {
        strictness,
        dependency_groups: dependency_group_models,
        execution_bundles: bundles,
    }
}

fn build_group_bundles(
    group: &[PlanItem],
    strictness: ReviewStrictness,
    bundle_decisions: &HashMap<String, BundleDecision>,
) -> Vec<ExecutionBundle> {
    let id_to_item: HashMap<&str, &PlanItem> = group.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut neighbors: HashMap<String, BTreeSet<String>> = group
        .iter()
        .map(|i| (i.id.clone(), BTreeSet::new()))
        .collect();

    for item in group {
        if matches!(
            item.relationship_type.as_str(),
            "ordered_dependency" | "independent"
        ) {
            continue;
        }
        for dep in &item.depends_on_item_ids {
            if id_to_item.contains_key(dep.as_str()) {
                neighbors
                    .entry(item.id.clone())
                    .or_default()
                    .insert(dep.clone());
                neighbors
                    .entry(dep.clone())
                    .or_default()
                    .insert(item.id.clone());
            }
        }
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut bundles: Vec<ExecutionBundle> = Vec::new();

    for item in group {
        if visited.contains(&item.id) {
            continue;
        }
        let component_ids = collect_component(&item.id, &neighbors, &mut visited);
        let component_items: Vec<&PlanItem> = component_ids
            .iter()
            .filter_map(|id| id_to_item.get(id.as_str()).copied())
            .collect();
        let bundle_id = bundle_id_for(&component_ids);
        let decision = bundle_decisions
            .get(&bundle_id)
            .copied()
            .unwrap_or_default();
        if matches!(decision, BundleDecision::SplitRequested) && component_items.len() > 1 {
            bundles.extend(build_split_bundles(&component_items));
            continue;
        }
        bundles.push(review_bundle(&component_items, strictness, bundle_id));
    }

    bundles
}

fn collect_component(
    start_id: &str,
    neighbors: &HashMap<String, BTreeSet<String>>,
    visited: &mut HashSet<String>,
) -> Vec<String> {
    let mut queue: Vec<String> = vec![start_id.to_string()];
    let mut component: Vec<String> = Vec::new();
    while let Some(current) = queue.pop() {
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());
        component.push(current.clone());
        if let Some(adj) = neighbors.get(&current) {
            // BTreeSet gives us deterministic ordering, matching Python's `sorted()`.
            for next in adj.iter().rev() {
                if !visited.contains(next) {
                    queue.push(next.clone());
                }
            }
        }
    }
    component
}

fn review_bundle(
    items: &[&PlanItem],
    strictness: ReviewStrictness,
    bundle_id: String,
) -> ExecutionBundle {
    let relationship_types: HashSet<&str> =
        items.iter().map(|i| i.relationship_type.as_str()).collect();
    let affected_targets: HashSet<&str> = items
        .iter()
        .flat_map(|i| i.affected_targets.iter().map(String::as_str))
        .collect();
    let item_types: HashSet<AssetItemType> = items.iter().map(|i| i.item_type).collect();

    let size_threshold = match strictness {
        ReviewStrictness::Efficient => 4,
        ReviewStrictness::Balanced => 3,
        ReviewStrictness::Strict => 2,
    };

    let mut risk_codes: Vec<String> = Vec::new();
    if relationship_types.contains("unknown") {
        risk_codes.push("unclear_coupling".into());
    }
    if items.len() > size_threshold {
        risk_codes.push("bundle_size_threshold".into());
    }
    if item_types.len() > 2 && !relationship_types.contains("same_feature") {
        risk_codes.push("mixed_item_types".into());
    }
    if affected_targets.len() > 3 {
        risk_codes.push("affected_targets_spread".into());
    }

    let status = if items.len() > 5 {
        BundleReviewStatus::SplitRecommended
    } else if !risk_codes.is_empty() {
        BundleReviewStatus::NeedsConfirmation
    } else {
        BundleReviewStatus::Clear
    };

    let reason = bundle_reason(&relationship_types);
    let blocking_reason = blocking_reason(status, &risk_codes);
    let recommended_actions = recommended_actions(status);
    let risk_details: Vec<RiskDetail> = risk_codes.iter().map(|c| risk_detail(c)).collect();

    ExecutionBundle {
        bundle_id,
        item_ids: items.iter().map(|i| i.id.clone()).collect(),
        status,
        reason,
        risk_codes,
        risk_details,
        recommended_actions,
        blocking_reason,
    }
}

fn build_split_bundles(items: &[&PlanItem]) -> Vec<ExecutionBundle> {
    items
        .iter()
        .map(|item| ExecutionBundle {
            bundle_id: bundle_id_for(std::slice::from_ref(&item.id)),
            item_ids: vec![item.id.clone()],
            status: BundleReviewStatus::Clear,
            reason: "已按用户要求拆分为独立执行单元".into(),
            risk_codes: Vec::new(),
            risk_details: Vec::new(),
            recommended_actions: Vec::new(),
            blocking_reason: "用户已要求拆分，该 item 当前按独立 bundle 处理。".into(),
        })
        .collect()
}

fn bundle_id_for(item_ids: &[String]) -> String {
    format!("bundle:{}", item_ids.join("::"))
}

fn bundle_reason(relationship_types: &HashSet<&str>) -> String {
    if relationship_types.contains("same_feature") {
        return "items 属于同一功能组，建议联合执行".into();
    }
    if relationship_types.contains("shared_mechanism") {
        return "items 共享机制、代码入口或资源上下文，建议联合确认是否合并".into();
    }
    if relationship_types.len() == 1 && relationship_types.contains("ordered_dependency") {
        return "items 只存在先后顺序，应保持独立执行".into();
    }
    if relationship_types.len() == 1 && relationship_types.contains("independent") {
        return "items 已标记为独立执行".into();
    }
    "items 的关系仍需确认".into()
}

fn risk_detail(code: &str) -> RiskDetail {
    match code {
        "unclear_coupling" => RiskDetail {
            code: code.into(),
            title: "Item 关系不明确".into(),
            summary: "系统无法确认这些 item 是否必须绑在一起执行。".into(),
            recommendation:
                "若你确认它们必须一起落地，可接受当前分组；否则优先补充关系说明或要求拆分。".into(),
            impact: "错误合并后会扩大一次执行失败的影响范围。".into(),
        },
        "bundle_size_threshold" => RiskDetail {
            code: code.into(),
            title: "Bundle 规模偏大".into(),
            summary: "当前 bundle 的 item 数已超过当前严格度下的建议阈值。".into(),
            recommendation: "优先要求拆分；只有在这些 item 明显属于同一功能包时再接受当前分组。"
                .into(),
            impact: "bundle 越大，失败后的定位和回滚成本越高。".into(),
        },
        "mixed_item_types" => RiskDetail {
            code: code.into(),
            title: "包含多种 item 类型".into(),
            summary: "同一 bundle 混入不同类型 item，执行节奏和验收口径更复杂。".into(),
            recommendation: "若只是弱相关，建议拆开；若围绕同一功能共同交付，可接受当前分组。"
                .into(),
            impact: "混合类型越多，执行与验收越容易漂移。".into(),
        },
        "affected_targets_spread" => RiskDetail {
            code: code.into(),
            title: "影响范围过散".into(),
            summary: "该 bundle 影响的目标点较多，说明分组边界可能过宽。".into(),
            recommendation: "优先返回补充范围说明，或要求先拆分后再重算。".into(),
            impact: "影响范围越散，一次失败波及多个模块的概率越高。".into(),
        },
        other => RiskDetail {
            code: other.into(),
            title: other.into(),
            summary: "系统识别到该 bundle 仍存在需要人工确认的风险。".into(),
            recommendation: "请结合分组理由判断是否接受当前分组，或返回补充说明后重算。".into(),
            impact: String::new(),
        },
    }
}

fn recommended_actions(status: BundleReviewStatus) -> Vec<RecommendedAction> {
    match status {
        BundleReviewStatus::Clear => Vec::new(),
        BundleReviewStatus::SplitRecommended => vec![
            RecommendedAction {
                action: "split_bundle".into(),
                label: "要求拆分".into(),
                description: "按更保守的口径重算，把该 bundle 拆成更小执行单元。".into(),
                emphasis: "warning".into(),
            },
            RecommendedAction {
                action: "accept_bundle".into(),
                label: "仍接受当前分组".into(),
                description: "你确认这些 item 必须一起执行，即使系统建议拆分。".into(),
                emphasis: "primary".into(),
            },
            RecommendedAction {
                action: "revise_items".into(),
                label: "返回补充说明".into(),
                description: "回到 Item 层补充关系说明、范围边界或验收说明后再重算。".into(),
                emphasis: "secondary".into(),
            },
        ],
        BundleReviewStatus::NeedsConfirmation => vec![
            RecommendedAction {
                action: "accept_bundle".into(),
                label: "接受当前分组".into(),
                description: "你确认这些 item 应该作为一个 bundle 联合执行。".into(),
                emphasis: "primary".into(),
            },
            RecommendedAction {
                action: "revise_items".into(),
                label: "返回补充说明".into(),
                description: "回到 Item 层补充关系说明、范围边界或验收说明后重算。".into(),
                emphasis: "secondary".into(),
            },
        ],
    }
}

fn blocking_reason(status: BundleReviewStatus, risk_codes: &[String]) -> String {
    match status {
        BundleReviewStatus::Clear => "当前 bundle 可直接执行。".into(),
        BundleReviewStatus::SplitRecommended => "系统建议先拆分该 bundle，再进入执行阶段。".into(),
        BundleReviewStatus::NeedsConfirmation
            if risk_codes.iter().any(|c| c == "unclear_coupling") =>
        {
            "系统认为该 bundle 可能可执行，但 Item 关系仍需你显式确认。".into()
        }
        BundleReviewStatus::NeedsConfirmation => {
            "系统仍需要你确认该 bundle 是否接受当前分组。".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::models::{AssetItemType, PlanItem};

    fn item(id: &str, item_type: AssetItemType) -> PlanItem {
        PlanItem {
            id: id.into(),
            item_type,
            name: id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_plan_yields_empty_preview() {
        let plan = ModPlan::default();
        let preview = build_execution_plan(&plan, ReviewStrictness::Balanced, &HashMap::new());
        assert!(preview.dependency_groups.is_empty());
        assert!(preview.execution_bundles.is_empty());
    }

    #[test]
    fn single_independent_item_clear_bundle() {
        // 默认 relationship_type="unknown" 会触发 unclear_coupling；需显式 independent
        // 才是真正的 "clear"。这条规则同时存在于 Python 端，是有意为之。
        let mut a = item("a", AssetItemType::Card);
        a.relationship_type = "independent".into();
        let plan = ModPlan {
            items: vec![a],
            ..Default::default()
        };
        let preview = build_execution_plan(&plan, ReviewStrictness::Balanced, &HashMap::new());
        assert_eq!(preview.execution_bundles.len(), 1);
        assert_eq!(
            preview.execution_bundles[0].status,
            BundleReviewStatus::Clear
        );
    }

    #[test]
    fn unknown_relationship_type_triggers_needs_confirmation() {
        // 默认 unknown → unclear_coupling → NeedsConfirmation。
        // 显式记录这条行为以防回归。
        let plan = ModPlan {
            items: vec![item("a", AssetItemType::Card)],
            ..Default::default()
        };
        let preview = build_execution_plan(&plan, ReviewStrictness::Balanced, &HashMap::new());
        assert_eq!(
            preview.execution_bundles[0].status,
            BundleReviewStatus::NeedsConfirmation,
        );
        assert!(
            preview.execution_bundles[0]
                .risk_codes
                .iter()
                .any(|c| c == "unclear_coupling")
        );
    }

    #[test]
    fn oversize_group_marked_split_recommended() {
        // 6 items in one connected group → status=split_recommended (>5 hard cap)
        let mut items = vec![item("a", AssetItemType::Card)];
        for i in 1..6 {
            let mut it = item(&format!("x{i}"), AssetItemType::Card);
            it.depends_on_item_ids = vec!["a".into()];
            items.push(it);
        }
        let plan = ModPlan {
            items,
            ..Default::default()
        };
        let preview = build_execution_plan(&plan, ReviewStrictness::Balanced, &HashMap::new());
        assert_eq!(preview.execution_bundles.len(), 1);
        assert_eq!(
            preview.execution_bundles[0].status,
            BundleReviewStatus::SplitRecommended,
        );
    }

    #[test]
    fn split_requested_decision_breaks_bundle_into_items() {
        let mut b = item("b", AssetItemType::Card);
        b.depends_on_item_ids = vec!["a".into()];
        let plan = ModPlan {
            items: vec![item("a", AssetItemType::Card), b],
            ..Default::default()
        };
        let mut decisions = HashMap::new();
        // 该 bundle 的 id 由两个 item 拼接得到。
        decisions.insert("bundle:a::b".into(), BundleDecision::SplitRequested);
        let preview = build_execution_plan(&plan, ReviewStrictness::Balanced, &decisions);
        // 拆分后应得到 2 个独立 bundle，全部 clear。
        assert_eq!(preview.execution_bundles.len(), 2);
        assert!(
            preview
                .execution_bundles
                .iter()
                .all(|b| b.status == BundleReviewStatus::Clear)
        );
    }

    #[test]
    fn ordered_dependency_keeps_items_split() {
        // ordered_dependency / independent 不参与二次合并 → 即使依赖相连，仍拆开为独立 bundle。
        let mut b = item("b", AssetItemType::Card);
        b.depends_on_item_ids = vec!["a".into()];
        b.relationship_type = "ordered_dependency".into();
        let plan = ModPlan {
            items: vec![item("a", AssetItemType::Card), b],
            ..Default::default()
        };
        let preview = build_execution_plan(&plan, ReviewStrictness::Balanced, &HashMap::new());
        assert_eq!(preview.execution_bundles.len(), 2);
    }
}
