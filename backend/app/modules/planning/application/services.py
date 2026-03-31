from __future__ import annotations

import json

from config import get_config
from llm.text_runner import complete_text

from app.modules.planning.application.dependency_graph import find_groups, topological_sort
from app.modules.planning.domain.models import AssetItemType, ModPlan, PlanItem
from app.shared.prompting import PromptLoader

_NEEDS_IMAGE_TYPES: set[AssetItemType] = {"card", "card_fullscreen", "relic", "power", "character"}
_PLANNER_PROMPT_BUNDLE_KEY = "runtime_agent.planning_planner_prompt"


class PlanningService:
    def __init__(self, knowledge_source=None, text_completion=None, prompt_loader: PromptLoader | None = None) -> None:
        self.knowledge_source = knowledge_source
        self.text_completion = text_completion or complete_text
        self.prompt_loader = prompt_loader or PromptLoader()

    def build_planner_prompt(self, requirements: str) -> str:
        api_hints = ""
        if self.knowledge_source is not None:
            api_hints = self.knowledge_source.load_context("planner")
        return self.prompt_loader.render(
            _PLANNER_PROMPT_BUNDLE_KEY,
            {
                "api_hints": api_hints,
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
                    implementation_notes=it.get("implementation_notes", ""),
                    needs_image=needs_image,
                    image_description=it.get("image_description", ""),
                    depends_on=it.get("depends_on", []),
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
                implementation_notes=it.get("implementation_notes", ""),
                needs_image=it.get("needs_image", True),
                image_description=it.get("image_description", ""),
                depends_on=it.get("depends_on", []),
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
