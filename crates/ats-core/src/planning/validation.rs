//! 计划校验。镜像 Python `application/plan_validation.py`。
//!
//! 检查项：必填字段、id 重复、自依赖、悬空依赖、strictness 分级补全。
//! 不做依赖图环检测（拓扑排序对环鲁棒；环属于"语义无效"但当前 Python 也未单独检查）。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::models::{ModPlan, PlanItem};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStrictness {
    Efficient,
    #[default]
    Balanced,
    Strict,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PlanItemReviewStatus {
    Clear,
    NeedsUserInput,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanValidationIssue {
    pub code: String,
    pub message: String,
    /// 关联字段名（如 "id"、"depends_on_item_ids"），无则空串
    #[serde(default)]
    pub field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemValidation {
    pub item_id: String,
    pub status: PlanItemReviewStatus,
    pub issues: Vec<PlanValidationIssue>,
    pub missing_fields: Vec<String>,
    pub clarification_questions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanValidationResult {
    pub strictness: ReviewStrictness,
    pub items: Vec<PlanItemValidation>,
}

pub fn validate_plan(plan: &ModPlan, strictness: ReviewStrictness) -> PlanValidationResult {
    let duplicate_ids = find_duplicate_ids(&plan.items);
    let known_ids: HashSet<&str> = plan.items.iter().map(|it| it.id.as_str()).collect();

    let items = plan
        .items
        .iter()
        .map(|item| validate_item(item, strictness, &duplicate_ids, &known_ids))
        .collect();

    PlanValidationResult { strictness, items }
}

fn find_duplicate_ids(items: &[PlanItem]) -> HashSet<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut dups: HashSet<String> = HashSet::new();
    for it in items {
        if it.id.is_empty() {
            continue;
        }
        if !seen.insert(it.id.as_str()) {
            dups.insert(it.id.clone());
        }
    }
    dups
}

fn validate_item(
    item: &PlanItem,
    strictness: ReviewStrictness,
    duplicate_ids: &HashSet<String>,
    known_ids: &HashSet<&str>,
) -> PlanItemValidation {
    let mut issues = Vec::new();
    let mut missing_fields: Vec<String> = Vec::new();

    if item.id.trim().is_empty() {
        issues.push(issue("missing_id", "item 缺少 id", "id"));
    }
    if item.name.trim().is_empty() {
        issues.push(issue("missing_name", "item 缺少 name", "name"));
    }
    if item.item_type.as_str().trim().is_empty() {
        issues.push(issue("missing_type", "item 缺少 type", "type"));
    }
    if duplicate_ids.contains(&item.id) {
        issues.push(issue("duplicate_id", "item id 重复", "id"));
    }
    if item.depends_on_item_ids.iter().any(|dep| dep == &item.id) {
        issues.push(issue(
            "self_dependency",
            "item 不能依赖自己",
            "depends_on_item_ids",
        ));
    }
    for dep in &item.depends_on_item_ids {
        if !known_ids.contains(dep.as_str()) {
            issues.push(issue(
                "missing_dependency",
                &format!("依赖项不存在: {dep}"),
                "depends_on_item_ids",
            ));
        }
    }

    // strictness 分级缺字段检查
    if item.item_type.as_str() == "custom_code" {
        if item.goal.trim().is_empty() {
            missing_fields.push("goal".into());
        }
        if item.detailed_description.trim().is_empty() {
            missing_fields.push("detailed_description".into());
        }
    } else if matches!(strictness, ReviewStrictness::Strict) && item.goal.trim().is_empty() {
        missing_fields.push("goal".into());
    }
    if matches!(strictness, ReviewStrictness::Strict)
        && !item.depends_on_item_ids.is_empty()
        && item.relationship_reason.trim().is_empty()
    {
        missing_fields.push("relationship_reason".into());
    }

    if !issues.is_empty() {
        return PlanItemValidation {
            item_id: item.id.clone(),
            status: PlanItemReviewStatus::Invalid,
            issues,
            missing_fields: Vec::new(),
            clarification_questions: Vec::new(),
        };
    }

    if !missing_fields.is_empty() {
        let questions: Vec<String> = missing_fields.iter().map(|f| question_for(f)).collect();
        return PlanItemValidation {
            item_id: item.id.clone(),
            status: PlanItemReviewStatus::NeedsUserInput,
            issues: vec![PlanValidationIssue {
                code: "missing_detail".into(),
                message: "item 仍缺少进入执行所需的关键信息".into(),
                field: String::new(),
            }],
            missing_fields,
            clarification_questions: questions,
        };
    }

    PlanItemValidation {
        item_id: item.id.clone(),
        status: PlanItemReviewStatus::Clear,
        issues: Vec::new(),
        missing_fields: Vec::new(),
        clarification_questions: Vec::new(),
    }
}

fn issue(code: &str, message: &str, field: &str) -> PlanValidationIssue {
    PlanValidationIssue {
        code: code.into(),
        message: message.into(),
        field: field.into(),
    }
}

fn question_for(field: &str) -> String {
    match field {
        "goal" => "这个 item 在整个 Mod 中的目标是什么？".into(),
        "detailed_description" => "请补充这个 item 的详细行为说明。".into(),
        "relationship_reason" => "请说明它为什么依赖这些 item。".into(),
        other => format!("请补充字段：{other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::models::PlanItem;

    fn item(id: &str) -> PlanItem {
        PlanItem {
            id: id.into(),
            item_type: "card".into(),
            name: id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_id_marked_invalid() {
        let plan = ModPlan {
            items: vec![PlanItem {
                name: "x".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let r = validate_plan(&plan, ReviewStrictness::Balanced);
        assert_eq!(r.items[0].status, PlanItemReviewStatus::Invalid);
        assert!(r.items[0].issues.iter().any(|i| i.code == "missing_id"));
    }

    #[test]
    fn duplicate_ids_flagged() {
        let plan = ModPlan {
            items: vec![item("a"), item("a")],
            ..Default::default()
        };
        let r = validate_plan(&plan, ReviewStrictness::Balanced);
        assert!(
            r.items
                .iter()
                .all(|i| i.issues.iter().any(|x| x.code == "duplicate_id"))
        );
    }

    #[test]
    fn missing_dependency_flagged() {
        let mut a = item("a");
        a.depends_on_item_ids = vec!["ghost".into()];
        let plan = ModPlan {
            items: vec![a],
            ..Default::default()
        };
        let r = validate_plan(&plan, ReviewStrictness::Balanced);
        assert!(
            r.items[0]
                .issues
                .iter()
                .any(|i| i.code == "missing_dependency")
        );
    }

    #[test]
    fn self_dependency_flagged() {
        let mut a = item("a");
        a.depends_on_item_ids = vec!["a".into()];
        let plan = ModPlan {
            items: vec![a],
            ..Default::default()
        };
        let r = validate_plan(&plan, ReviewStrictness::Balanced);
        assert!(
            r.items[0]
                .issues
                .iter()
                .any(|i| i.code == "self_dependency")
        );
    }

    #[test]
    fn custom_code_missing_goal_needs_input() {
        let mut a = item("a");
        a.item_type = "custom_code".into();
        let plan = ModPlan {
            items: vec![a],
            ..Default::default()
        };
        let r = validate_plan(&plan, ReviewStrictness::Balanced);
        assert_eq!(r.items[0].status, PlanItemReviewStatus::NeedsUserInput);
        assert!(r.items[0].missing_fields.contains(&"goal".to_string()));
    }

    #[test]
    fn balanced_card_without_goal_is_clear() {
        let plan = ModPlan {
            items: vec![item("a")],
            ..Default::default()
        };
        let r = validate_plan(&plan, ReviewStrictness::Balanced);
        assert_eq!(r.items[0].status, PlanItemReviewStatus::Clear);
    }

    #[test]
    fn strict_mode_requires_goal() {
        let plan = ModPlan {
            items: vec![item("a")],
            ..Default::default()
        };
        let r = validate_plan(&plan, ReviewStrictness::Strict);
        assert_eq!(r.items[0].status, PlanItemReviewStatus::NeedsUserInput);
        assert!(r.items[0].missing_fields.contains(&"goal".to_string()));
    }
}
