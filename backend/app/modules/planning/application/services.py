from __future__ import annotations

import json

from app.modules.planning.application.dependency_graph import find_groups, topological_sort
from app.modules.planning.application.execution_bundles import ExecutionPlanPreview, build_execution_plan
from app.modules.planning.application.plan_validation import PlanValidationResult, ReviewStrictness, validate_plan
from app.modules.planning.domain.models import AssetItemType, ModPlan, PlanItem
from app.shared.contracts.knowledge import KnowledgeQuery
from app.shared.prompting import PromptContextAssembler, PromptLoader
from config import get_config
from llm.text_runner import complete_text

_NEEDS_IMAGE_TYPES: set[AssetItemType] = {"card", "card_fullscreen", "relic", "power", "character"}
_PLANNER_PROMPT_BUNDLE_KEY = "runtime_agent.planning_planner_prompt"


class PlanningService:
    def __init__(
        self,
        knowledge_source=None,
        text_completion=None,
        prompt_loader: PromptLoader | None = None,
        knowledge_resolver=None,
        prompt_context_assembler: PromptContextAssembler | None = None,
    ) -> None:
        self.knowledge_source = knowledge_source
        self.text_completion = text_completion or complete_text
        self.prompt_loader = prompt_loader or PromptLoader()
        self.knowledge_resolver = knowledge_resolver
        self.prompt_context_assembler = prompt_context_assembler or PromptContextAssembler()

    @staticmethod
    def _build_legacy_planner_knowledge(guidance_text: str) -> dict[str, str]:
        guidance = guidance_text.strip()
        facts = (
            "Structured planner fact calibration is unavailable in this legacy fallback path. "
            "Treat the planner guidance as summarized conventions and verify exact APIs against runtime knowledge before implementation."
        )
        lookup = (
            "Inspect `runtime/knowledge/game/` and `runtime/knowledge/baselib/BaseLib.decompiled.cs` "
            "when the plan depends on exact base classes, hooks, or command names."
        )
        warnings = (
            "### Warnings\n"
            "- Legacy planner fallback is active. Guidance remains primary, but exact code facts still need manual verification."
        )
        return {
            "guidance": guidance,
            "facts": facts,
            "lookup": lookup,
            "knowledge_warnings": warnings,
            "summary": "legacy planner fallback",
        }

    def _build_planner_query(self, requirements: str) -> KnowledgeQuery:
        return KnowledgeQuery(
            scenario="planner",
            domain="sts2",
            requirements=requirements,
        )

    def _resolve_planner_knowledge(self, requirements: str) -> dict[str, str]:
        if self.knowledge_resolver is None:
            return {
                "guidance": "",
                "facts": "",
                "lookup": "",
                "knowledge_warnings": "",
                "summary": "",
            }

        packet = self.knowledge_resolver.resolve(self._build_planner_query(requirements))
        context = self.prompt_context_assembler.assemble(packet)
        return {
            "guidance": context.get("guidance", ""),
            "facts": context.get("facts", ""),
            "lookup": context.get("lookup", ""),
            "knowledge_warnings": context.get("knowledge_warnings", ""),
            "summary": context.get("summary", ""),
        }

    def build_planner_prompt(self, requirements: str) -> str:
        knowledge = self._resolve_planner_knowledge(requirements)
        if not any(knowledge[key].strip() for key in ("guidance", "facts", "lookup")):
            if self.knowledge_source is not None:
                knowledge = self._build_legacy_planner_knowledge(self.knowledge_source.load_context("planner"))
        elif not knowledge["guidance"] and self.knowledge_source is not None:
            knowledge["guidance"] = self.knowledge_source.load_context("planner").strip()
        return self.prompt_loader.render(
            _PLANNER_PROMPT_BUNDLE_KEY,
            {
                "guidance": knowledge["guidance"],
                "facts": knowledge["facts"],
                "lookup": knowledge["lookup"],
                "knowledge_warnings": knowledge["knowledge_warnings"],
                "requirements": requirements,
            },
        )

    async def plan_mod(self, requirements: str) -> ModPlan:
        cfg = get_config()
        llm_cfg = cfg["llm"]
        prompt = self.build_planner_prompt(requirements)
        try:
            raw_json = await self.text_completion(prompt, llm_cfg)
            return self.parse_plan(raw_json)
        except Exception as exc:
            raise RuntimeError(f"规划失败: {type(exc).__name__}: {exc}") from exc

    def parse_plan(self, raw_json: str) -> ModPlan:
        try:
            data = json.loads(raw_json)
        except json.JSONDecodeError:
            from json_repair import repair_json

            data = json.loads(repair_json(raw_json))

        items = []
        for it in data.get("items", []):
            item_type: AssetItemType = it.get("type", "custom_code")
            needs_image = it.get("needs_image", item_type in _NEEDS_IMAGE_TYPES)
            items.append(
                PlanItem(
                    id=it["id"],
                    type=item_type,
                    name=it["name"],
                    name_zhs=it.get("name_zhs", ""),
                    description=it.get("description", ""),
                    goal=it.get("goal", ""),
                    detailed_description=it.get("detailed_description", ""),
                    implementation_notes=it.get("implementation_notes", ""),
                    needs_image=needs_image,
                    image_description=it.get("image_description", ""),
                    depends_on=it.get("depends_on", []),
                    scope_boundary=it.get("scope_boundary", ""),
                    dependency_reason=it.get("dependency_reason", ""),
                    acceptance_notes=it.get("acceptance_notes", ""),
                    affected_targets=it.get("affected_targets", []),
                    coupling_kind=it.get("coupling_kind", "unclear"),
                    clarification_status=it.get("clarification_status", ""),
                    clarification_questions=it.get("clarification_questions", []),
                )
            )
        return ModPlan(
            mod_name=data.get("mod_name", "MyMod"),
            summary=data.get("summary", ""),
            items=items,
        )

    def plan_from_dict(self, data: dict) -> ModPlan:
        items = [
            PlanItem(
                id=it["id"],
                type=it["type"],
                name=it["name"],
                name_zhs=it.get("name_zhs", ""),
                description=it.get("description", ""),
                goal=it.get("goal", ""),
                detailed_description=it.get("detailed_description", ""),
                implementation_notes=it.get("implementation_notes", ""),
                needs_image=it.get("needs_image", True),
                image_description=it.get("image_description", ""),
                depends_on=it.get("depends_on", []),
                scope_boundary=it.get("scope_boundary", ""),
                dependency_reason=it.get("dependency_reason", ""),
                acceptance_notes=it.get("acceptance_notes", ""),
                affected_targets=it.get("affected_targets", []),
                coupling_kind=it.get("coupling_kind", "unclear"),
                clarification_status=it.get("clarification_status", ""),
                clarification_questions=it.get("clarification_questions", []),
                provided_image_b64=it.get("provided_image_b64", ""),
            )
            for it in data.get("items", [])
        ]
        return ModPlan(
            mod_name=data.get("mod_name", "MyMod"),
            summary=data.get("summary", ""),
            items=items,
        )

    def topologically_sort_plan_items(self, items: list[PlanItem]) -> list[PlanItem]:
        return topological_sort(items)

    def find_groups(self, items: list[PlanItem]) -> list[list[PlanItem]]:
        return find_groups(items)

    def validate_plan(self, plan: ModPlan, strictness: ReviewStrictness = "balanced") -> PlanValidationResult:
        return validate_plan(plan, strictness)

    def build_execution_plan(
        self,
        plan: ModPlan,
        strictness: str = "balanced",
        bundle_decisions: dict[str, str] | None = None,
    ) -> ExecutionPlanPreview:
        return build_execution_plan(plan, strictness, bundle_decisions)
