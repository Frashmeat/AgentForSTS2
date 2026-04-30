from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Literal

from app.modules.planning.application.dependency_graph import find_groups
from app.modules.planning.domain.models import ModPlan, PlanItem

BundleReviewStatus = Literal["clear", "needs_confirmation", "split_recommended"]
BundleDecision = Literal["unresolved", "accepted", "split_requested", "needs_item_revision"]


@dataclass
class DependencyGroup:
    item_ids: list[str]

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class ExecutionBundle:
    bundle_id: str
    item_ids: list[str]
    status: BundleReviewStatus
    reason: str
    risk_codes: list[str] = field(default_factory=list)
    risk_details: list[dict] = field(default_factory=list)
    recommended_actions: list[dict] = field(default_factory=list)
    blocking_reason: str = ""

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


def build_execution_plan(
    plan: ModPlan,
    strictness: str = "balanced",
    bundle_decisions: dict[str, BundleDecision] | None = None,
) -> ExecutionPlanPreview:
    dependency_groups = find_groups(plan.items)
    dependency_group_models = [DependencyGroup(item_ids=[item.id for item in group]) for group in dependency_groups]

    normalized_decisions = bundle_decisions or {}
    bundles: list[ExecutionBundle] = []
    for group in dependency_groups:
        bundles.extend(_build_group_bundles(group, strictness=strictness, bundle_decisions=normalized_decisions))

    return ExecutionPlanPreview(
        strictness=strictness,
        dependency_groups=dependency_group_models,
        execution_bundles=bundles,
    )


def _build_group_bundles(
    group: list[PlanItem],
    *,
    strictness: str,
    bundle_decisions: dict[str, BundleDecision],
) -> list[ExecutionBundle]:
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
        bundle_id = _bundle_id(component_ids)
        if bundle_decisions.get(bundle_id) == "split_requested" and len(component_items) > 1:
            bundles.extend(_build_split_bundles(component_items))
            continue
        bundles.append(_review_bundle(component_items, strictness=strictness, bundle_id=bundle_id))

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


def _review_bundle(items: list[PlanItem], *, strictness: str, bundle_id: str) -> ExecutionBundle:
    risk_codes: list[str] = []
    coupling_kinds = {item.coupling_kind for item in items}
    affected_targets = {target for item in items for target in item.affected_targets}
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
        bundle_id=bundle_id,
        item_ids=[item.id for item in items],
        status=status,
        reason=reason,
        risk_codes=risk_codes,
        risk_details=[_risk_detail(code) for code in risk_codes],
        recommended_actions=_recommended_actions(status),
        blocking_reason=_blocking_reason(status, risk_codes),
    )


def _build_split_bundles(items: list[PlanItem]) -> list[ExecutionBundle]:
    bundles: list[ExecutionBundle] = []
    for item in items:
        bundles.append(
            ExecutionBundle(
                bundle_id=_bundle_id([item.id]),
                item_ids=[item.id],
                status="clear",
                reason="已按用户要求拆分为独立执行单元",
                risk_codes=[],
                risk_details=[],
                recommended_actions=[],
                blocking_reason="用户已要求拆分，该 item 当前按独立 bundle 处理。",
            )
        )
    return bundles


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


def _bundle_id(item_ids: list[str]) -> str:
    return f"bundle:{'::'.join(item_ids)}"


def _risk_detail(code: str) -> dict:
    mapping = {
        "unclear_coupling": {
            "code": "unclear_coupling",
            "title": "耦合关系不明确",
            "summary": "系统无法确认这些 item 是否必须绑在一起执行。",
            "recommendation": "若你确认它们必须一起落地，可接受当前分组；否则优先补充依赖说明或要求拆分。",
            "impact": "错误合并后会扩大一次执行失败的影响范围。",
        },
        "bundle_size_threshold": {
            "code": "bundle_size_threshold",
            "title": "Bundle 规模偏大",
            "summary": "当前 bundle 的 item 数已超过当前严格度下的建议阈值。",
            "recommendation": "优先要求拆分；只有在这些 item 明显属于同一功能包时再接受当前分组。",
            "impact": "bundle 越大，失败后的定位和回滚成本越高。",
        },
        "mixed_item_types": {
            "code": "mixed_item_types",
            "title": "包含多种 item 类型",
            "summary": "同一 bundle 混入不同类型 item，执行节奏和验收口径更复杂。",
            "recommendation": "若只是弱相关，建议拆开；若围绕同一功能共同交付，可接受当前分组。",
            "impact": "混合类型越多，执行与验收越容易漂移。",
        },
        "affected_targets_spread": {
            "code": "affected_targets_spread",
            "title": "影响范围过散",
            "summary": "该 bundle 影响的目标点较多，说明分组边界可能过宽。",
            "recommendation": "优先返回补充范围说明，或要求先拆分后再重算。",
            "impact": "影响范围越散，一次失败波及多个模块的概率越高。",
        },
    }
    return mapping.get(
        code,
        {
            "code": code,
            "title": code,
            "summary": "系统识别到该 bundle 仍存在需要人工确认的风险。",
            "recommendation": "请结合分组理由判断是否接受当前分组，或返回补充说明后重算。",
        },
    )


def _recommended_actions(status: BundleReviewStatus) -> list[dict]:
    if status == "clear":
        return []
    if status == "split_recommended":
        return [
            {
                "action": "split_bundle",
                "label": "要求拆分",
                "description": "按更保守的口径重算，把该 bundle 拆成更小执行单元。",
                "emphasis": "warning",
            },
            {
                "action": "accept_bundle",
                "label": "仍接受当前分组",
                "description": "你确认这些 item 必须一起执行，即使系统建议拆分。",
                "emphasis": "primary",
            },
            {
                "action": "revise_items",
                "label": "返回补充说明",
                "description": "回到 Item 层补充依赖原因、范围边界或验收说明后再重算。",
                "emphasis": "secondary",
            },
        ]
    return [
        {
            "action": "accept_bundle",
            "label": "接受当前分组",
            "description": "你确认这些 item 应该作为一个 bundle 联合执行。",
            "emphasis": "primary",
        },
        {
            "action": "revise_items",
            "label": "返回补充说明",
            "description": "回到 Item 层补充依赖原因、范围边界或验收说明后重算。",
            "emphasis": "secondary",
        },
    ]


def _blocking_reason(status: BundleReviewStatus, risk_codes: list[str]) -> str:
    if status == "clear":
        return "当前 bundle 可直接执行。"
    if status == "split_recommended":
        return "系统建议先拆分该 bundle，再进入执行阶段。"
    if "unclear_coupling" in risk_codes:
        return "系统认为该 bundle 可能可执行，但耦合关系仍需你显式确认。"
    return "系统仍需要你确认该 bundle 是否接受当前分组。"
