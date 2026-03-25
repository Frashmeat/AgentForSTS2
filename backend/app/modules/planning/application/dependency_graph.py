from __future__ import annotations

from app.modules.planning.domain.models import PlanItem


def topological_sort(items: list[PlanItem]) -> list[PlanItem]:
    id_map = {item.id: item for item in items}
    visited: set[str] = set()
    result: list[PlanItem] = []

    def visit(item_id: str):
        if item_id in visited or item_id not in id_map:
            return
        visited.add(item_id)
        for dep in id_map[item_id].depends_on:
            visit(dep)
        result.append(id_map[item_id])

    for item in items:
        visit(item.id)
    return result


def find_groups(items: list[PlanItem]) -> list[list[PlanItem]]:
    id_to_item = {it.id: it for it in items}
    neighbors: dict[str, set[str]] = {it.id: set() for it in items}
    for it in items:
        for dep in it.depends_on:
            if dep in id_to_item:
                neighbors[it.id].add(dep)
                neighbors[dep].add(it.id)

    visited: set[str] = set()
    groups: list[list[PlanItem]] = []
    for it in items:
        if it.id in visited:
            continue
        group_ids: list[str] = []
        queue = [it.id]
        while queue:
            curr = queue.pop()
            if curr in visited:
                continue
            visited.add(curr)
            group_ids.append(curr)
            queue.extend(neighbors[curr] - visited)
        groups.append(topological_sort([id_to_item[i] for i in group_ids if i in id_to_item]))
    return groups
