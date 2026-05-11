//! 依赖图操作。镜像 Python `application/dependency_graph.py`。

use std::collections::{HashMap, HashSet};

use super::models::PlanItem;

/// 按依赖拓扑排序（依赖项在前）。
///
/// 算法：基于 DFS 的 post-order；重复 id 与缺失依赖被静默跳过，调用方应先
/// 调 `validate_plan` 处理这类语义错误。
#[must_use]
pub fn topological_sort(items: &[PlanItem]) -> Vec<PlanItem> {
    let id_map: HashMap<&str, &PlanItem> =
        items.iter().map(|it| (it.id.as_str(), it)).collect();
    let mut visited: HashSet<String> = HashSet::new();
    let mut result: Vec<PlanItem> = Vec::with_capacity(items.len());

    for root in items {
        visit(&root.id, &id_map, &mut visited, &mut result);
    }
    result
}

fn visit(
    item_id: &str,
    id_map: &HashMap<&str, &PlanItem>,
    visited: &mut HashSet<String>,
    result: &mut Vec<PlanItem>,
) {
    if visited.contains(item_id) {
        return;
    }
    let Some(item) = id_map.get(item_id).copied() else {
        return;
    };
    visited.insert(item_id.to_string());
    for dep in &item.depends_on_item_ids {
        visit(dep, id_map, visited, result);
    }
    result.push(item.clone());
}

/// 把通过依赖关系（无向）相连的 items 聚成一组。每组内部按拓扑顺序排列。
#[must_use]
pub fn find_groups(items: &[PlanItem]) -> Vec<Vec<PlanItem>> {
    let id_to_item: HashMap<&str, &PlanItem> =
        items.iter().map(|it| (it.id.as_str(), it)).collect();
    let mut neighbors: HashMap<String, HashSet<String>> = items
        .iter()
        .map(|it| (it.id.clone(), HashSet::new()))
        .collect();
    for it in items {
        for dep in &it.depends_on_item_ids {
            if id_to_item.contains_key(dep.as_str()) {
                neighbors
                    .entry(it.id.clone())
                    .or_default()
                    .insert(dep.clone());
                neighbors
                    .entry(dep.clone())
                    .or_default()
                    .insert(it.id.clone());
            }
        }
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut groups: Vec<Vec<PlanItem>> = Vec::new();
    for it in items {
        if visited.contains(&it.id) {
            continue;
        }
        let mut group_ids: Vec<String> = Vec::new();
        let mut stack: Vec<String> = vec![it.id.clone()];
        while let Some(curr) = stack.pop() {
            if visited.contains(&curr) {
                continue;
            }
            visited.insert(curr.clone());
            group_ids.push(curr.clone());
            if let Some(adj) = neighbors.get(&curr) {
                for next in adj {
                    if !visited.contains(next) {
                        stack.push(next.clone());
                    }
                }
            }
        }
        let group_items: Vec<PlanItem> = group_ids
            .iter()
            .filter_map(|id| id_to_item.get(id.as_str()).map(|it| (*it).clone()))
            .collect();
        groups.push(topological_sort(&group_items));
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::planning::models::{AssetItemType, PlanItem};

    fn item(id: &str, deps: &[&str]) -> PlanItem {
        PlanItem {
            id: id.into(),
            item_type: AssetItemType::Card,
            name: id.into(),
            depends_on_item_ids: deps.iter().map(|s| (*s).to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn topological_sort_orders_dependencies_first() {
        let items = vec![
            item("c", &["b"]),
            item("a", &[]),
            item("b", &["a"]),
        ];
        let sorted: Vec<String> = topological_sort(&items).into_iter().map(|i| i.id).collect();
        // a 必须在 b 前，b 必须在 c 前
        let pos = |id: &str| sorted.iter().position(|i| i == id).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("b") < pos("c"));
    }

    #[test]
    fn find_groups_splits_disconnected_components() {
        let items = vec![
            item("a", &[]),
            item("b", &["a"]),
            item("x", &[]),
            item("y", &["x"]),
        ];
        let groups = find_groups(&items);
        assert_eq!(groups.len(), 2);
        // 两组分别覆盖 a/b 与 x/y
        let group_ids: Vec<Vec<String>> = groups
            .iter()
            .map(|g| g.iter().map(|i| i.id.clone()).collect())
            .collect();
        let has_ab = group_ids.iter().any(|g| g.contains(&"a".into()) && g.contains(&"b".into()));
        let has_xy = group_ids.iter().any(|g| g.contains(&"x".into()) && g.contains(&"y".into()));
        assert!(has_ab && has_xy);
    }

    #[test]
    fn topological_sort_ignores_missing_dependency() {
        let items = vec![item("a", &["ghost"])];
        let sorted = topological_sort(&items);
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].id, "a");
    }
}
