from __future__ import annotations

from agents import sts2_guidance
from app.modules.knowledge.infra.sts2_code_facts_provider import Sts2CodeFactsProvider
from app.modules.knowledge.infra.sts2_guidance_provider import Sts2GuidanceProvider
from app.modules.knowledge.infra.sts2_knowledge_resolver import Sts2KnowledgeResolver
from app.modules.knowledge.infra.sts2_lookup_provider import Sts2LookupProvider
from app.modules.planning.application.execution_bundles import ExecutionPlanPreview
from app.modules.knowledge.infra.sts2_guidance_source import Sts2GuidanceKnowledgeSource
from app.modules.planning.application.services import PlanningService
from app.modules.planning.application.plan_validation import PlanValidationResult, ReviewStrictness
from app.modules.planning.domain.models import AssetItemType, ModPlan, PlanItem
from app.shared.prompting import PromptContextAssembler

_knowledge_source = Sts2GuidanceKnowledgeSource(
    guidance_for_asset_type=sts2_guidance.get_guidance_for_asset_type,
    planner_guidance=sts2_guidance.get_planner_guidance,
)
_knowledge_resolver = Sts2KnowledgeResolver(
    code_facts_provider=Sts2CodeFactsProvider(),
    guidance_provider=Sts2GuidanceProvider(),
    lookup_provider=Sts2LookupProvider(),
)
_service = PlanningService(
    knowledge_source=_knowledge_source,
    knowledge_resolver=_knowledge_resolver,
    prompt_context_assembler=PromptContextAssembler(),
)


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


def validate_plan(plan: ModPlan, strictness: ReviewStrictness = "balanced") -> PlanValidationResult:
    return _service.validate_plan(plan, strictness)


def build_execution_plan(
    plan: ModPlan,
    strictness: str = "balanced",
    bundle_decisions: dict[str, str] | None = None,
) -> ExecutionPlanPreview:
    return _service.build_execution_plan(plan, strictness, bundle_decisions)


__all__ = [
    "AssetItemType",
    "ExecutionPlanPreview",
    "ModPlan",
    "PlanItem",
    "PlanValidationResult",
    "ReviewStrictness",
    "build_planner_prompt",
    "find_groups",
    "parse_plan",
    "plan_from_dict",
    "plan_mod",
    "topological_sort",
    "build_execution_plan",
    "validate_plan",
]
