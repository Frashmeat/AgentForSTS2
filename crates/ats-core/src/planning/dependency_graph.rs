//! 依赖图操作。镜像 Python `application/dependency_graph.py`。

use std::collections::{HashMap, HashSet};

use super::models::PlanItem;

/// 按依赖拓扑排序（依赖项在前）。
///
/// 算法：基于 DFS 的 post-order；重复 id 与缺失依赖被静默跳过，调用方应先
/// 调 `validate_plan` 处理这类语义错误。
#[must_use]
pub fn topological_sort(items: &[PlanItem]) -> Vec<PlanItem> {
    let id_map: HashMap<&str, &PlanItem> = items.iter().map(|it| (it.id.as_str(), it)).collect();
    let mut visited: HashSet<String> = HashSet::new();
    let mut result: Vec<PlanItem> = Vec::with_capacity(items.len());

    for root in items {
        visit(&root.id, &id_map, &mut visited, &mut result);
    }
    result
}

fn visit(
    root_id: &str,
    id_map: &HashMap<&str, &PlanItem>,
    visited: &mut HashSet<String>,
    result: &mut Vec<PlanItem>,
) {
    // 迭代式 post-order DFS：用显式栈替代递归，避免 LLM 产出的超深依赖链把调用栈打爆
    // （递归实现会 stack overflow 直接 abort 进程；这是不可 catch 的崩溃）。
    // 栈帧 (id, expanded)：expanded=false 表示首次访问、需展开依赖；
    // expanded=true 表示依赖已全部输出，可以输出该节点本身（post-order）。
    let mut stack: Vec<(String, bool)> = vec![(root_id.to_string(), false)];
    while let Some((id, expanded)) = stack.pop() {
        if expanded {
            if let Some(item) = id_map.get(id.as_str()).copied() {
                result.push(item.clone());
            }
            continue;
        }
        if visited.contains(&id) {
            continue;
        }
        let Some(item) = id_map.get(id.as_str()).copied() else {
            continue;
        };
        visited.insert(id.clone());
        // 先压「退出」帧（输出自己），再逆序压依赖 —— 栈是 LIFO，逆序保证
        // 依赖按原列表顺序被处理，与原递归实现的输出顺序一致。
        stack.push((id, true));
        for dep in item.depends_on_item_ids.iter().rev() {
            if !visited.contains(dep) {
                stack.push((dep.clone(), false));
            }
        }
    }
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
        let items = vec![item("c", &["b"]), item("a", &[]), item("b", &["a"])];
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
        let has_ab = group_ids
            .iter()
            .any(|g| g.contains(&"a".into()) && g.contains(&"b".into()));
        let has_xy = group_ids
            .iter()
            .any(|g| g.contains(&"x".into()) && g.contains(&"y".into()));
        assert!(has_ab && has_xy);
    }

    #[test]
    fn topological_sort_ignores_missing_dependency() {
        let items = vec![item("a", &["ghost"])];
        let sorted = topological_sort(&items);
        assert_eq!(sorted.len(), 1);
        assert_eq!(sorted[0].id, "a");
    }

    #[test]
    fn topological_sort_handles_deep_chain_without_stack_overflow() {
        // 一条 20000 长的依赖链：item_0 <- item_1 <- ... <- item_19999。
        // 递归实现在测试线程 2MB 栈上会爆栈 abort；迭代实现必须正常返回。
        let n: usize = 20_000;
        let items: Vec<PlanItem> = (0..n)
            .map(|i| {
                let id = format!("item_{i}");
                let deps = if i == 0 {
                    Vec::new()
                } else {
                    vec![format!("item_{}", i - 1)]
                };
                PlanItem {
                    id: id.clone(),
                    item_type: AssetItemType::Card,
                    name: id,
                    depends_on_item_ids: deps,
                    ..Default::default()
                }
            })
            .collect();
        let sorted = topological_sort(&items);
        assert_eq!(sorted.len(), n);
        // 依赖在前：item_0 必须最先，item_{n-1} 必须最后。
        assert_eq!(sorted[0].id, "item_0");
        assert_eq!(sorted[n - 1].id, format!("item_{}", n - 1));
    }

    #[test]
    fn topological_sort_terminates_on_cycle() {
        // a<->b 互相依赖：必须终止（不死循环 / 不爆栈），两个都产出。
        let items = vec![item("a", &["b"]), item("b", &["a"])];
        let sorted = topological_sort(&items);
        assert_eq!(sorted.len(), 2);
    }
}
