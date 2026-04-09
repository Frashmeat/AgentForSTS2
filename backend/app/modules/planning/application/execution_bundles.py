from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Literal

from app.modules.planning.application.dependency_graph import find_groups
from app.modules.planning.domain.models import ModPlan, PlanItem

BundleReviewStatus = Literal["clear", "needs_confirmation", "split_recommended"]


@dataclass
class DependencyGroup:
    item_ids: list[str]

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class ExecutionBundle:
    item_ids: list[str]
    status: BundleReviewStatus
    reason: str
    risk_codes: list[str] = field(default_factory=list)

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class ExecutionPlanPreview:
    strictness: str
    dependency_groups: list[DependencyGroup]
    execution_bundles: list[ExecutionBundle]

    def to_dict(self) -> dict:
        return {
            "strictness": self.strictness,
            "dependency_groups": [group.to_dict() for group in self.dependency_groups],
            "execution_bundles": [bundle.to_dict() for bundle in self.execution_bundles],
        }


def build_execution_plan(plan: ModPlan, strictness: str = "balanced") -> ExecutionPlanPreview:
    dependency_groups = find_groups(plan.items)
    dependency_group_models = [DependencyGroup(item_ids=[item.id for item in group]) for group in dependency_groups]

    bundles: list[ExecutionBundle] = []
    for group in dependency_groups:
        bundles.extend(_build_group_bundles(group, strictness=strictness))

    return ExecutionPlanPreview(
        strictness=strictness,
        dependency_groups=dependency_group_models,
        execution_bundles=bundles,
    )


def _build_group_bundles(group: list[PlanItem], *, strictness: str) -> list[ExecutionBundle]:
    id_to_item = {item.id: item for item in group}
    neighbors: dict[str, set[str]] = {item.id: set() for item in group}

    for item in group:
        if item.coupling_kind == "order_only":
            continue
        for dep in item.depends_on:
            if dep in neighbors:
                neighbors[item.id].add(dep)
                neighbors[dep].add(item.id)

    visited: set[str] = set()
    bundles: list[ExecutionBundle] = []

    for item in group:
        if item.id in visited:
            continue
        component_ids = _collect_component(item.id, neighbors, visited)
        component_items = [id_to_item[item_id] for item_id in component_ids]
        bundles.append(_review_bundle(component_items, strictness=strictness))

    return bundles


def _collect_component(start_id: str, neighbors: dict[str, set[str]], visited: set[str]) -> list[str]:
    queue = [start_id]
    component: list[str] = []
    while queue:
        current = queue.pop()
        if current in visited:
            continue
        visited.add(current)
        component.append(current)
        queue.extend(sorted(neighbors[current] - visited))
    return component


def _review_bundle(items: list[PlanItem], *, strictness: str) -> ExecutionBundle:
    risk_codes: list[str] = []
    coupling_kinds = {item.coupling_kind for item in items}
    affected_targets = {
        target
        for item in items
        for target in item.affected_targets
    }
    item_types = {item.type for item in items}
    size_threshold = {"efficient": 4, "balanced": 3, "strict": 2}.get(strictness, 3)

    if "unclear" in coupling_kinds:
        risk_codes.append("unclear_coupling")
    if len(items) > size_threshold:
        risk_codes.append("bundle_size_threshold")
    if len(item_types) > 2 and "feature_bundle" not in coupling_kinds:
        risk_codes.append("mixed_item_types")
    if len(affected_targets) > 3:
        risk_codes.append("affected_targets_spread")

    status: BundleReviewStatus = "clear"
    if len(items) > 5:
        status = "split_recommended"
    elif risk_codes:
        status = "needs_confirmation"

    reason = _bundle_reason(coupling_kinds)
    return ExecutionBundle(
        item_ids=[item.id for item in items],
        status=status,
        reason=reason,
        risk_codes=risk_codes,
    )


def _bundle_reason(coupling_kinds: set[str]) -> str:
    if "feature_bundle" in coupling_kinds:
        return "items 属于同一功能包，建议联合执行"
    if "shared_logic" in coupling_kinds:
        return "items 共享核心逻辑改动点，建议联合执行"
    if "shared_registration" in coupling_kinds:
        return "items 共享注册入口，建议联合执行"
    if "shared_resource" in coupling_kinds:
        return "items 共享资源上下文，建议联合确认是否合并"
    if "order_only" in coupling_kinds and len(coupling_kinds) == 1:
        return "items 只存在顺序依赖，应保持独立执行"
    return "items 的执行耦合关系仍需确认"
