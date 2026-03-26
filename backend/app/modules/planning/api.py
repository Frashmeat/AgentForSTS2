from __future__ import annotations

from agents import sts2_docs
from app.modules.knowledge.infra.sts2_docs_source import Sts2DocsKnowledgeSource
from app.modules.planning.application.services import PlanningService
from app.modules.planning.domain.models import AssetItemType, ModPlan, PlanItem

_knowledge_source = Sts2DocsKnowledgeSource(
    docs_for_type=sts2_docs.get_docs_for_type,
    planner_hints=sts2_docs.get_planner_api_hints,
)
_service = PlanningService(knowledge_source=_knowledge_source)


def build_planner_prompt(requirements: str) -> str:
    return _service.build_planner_prompt(requirements)


async def plan_mod(requirements: str) -> ModPlan:
    return await _service.plan_mod(requirements)


def parse_plan(raw_json: str) -> ModPlan:
    return _service.parse_plan(raw_json)


def plan_from_dict(data: dict) -> ModPlan:
    return _service.plan_from_dict(data)


def find_groups(items: list[PlanItem]) -> list[list[PlanItem]]:
    return _service.find_groups(items)


def topological_sort(items: list[PlanItem]) -> list[PlanItem]:
    return _service.topologically_sort_plan_items(items)


__all__ = [
    "AssetItemType",
    "ModPlan",
    "PlanItem",
    "build_planner_prompt",
    "find_groups",
    "parse_plan",
    "plan_from_dict",
    "plan_mod",
    "topological_sort",
]
