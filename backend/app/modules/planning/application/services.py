from __future__ import annotations

import json

from config import get_config
from llm.text_runner import complete_text

from app.modules.planning.application.dependency_graph import find_groups, topological_sort
from app.modules.planning.domain.models import AssetItemType, ModPlan, PlanItem

_NEEDS_IMAGE_TYPES: set[AssetItemType] = {"card", "card_fullscreen", "relic", "power", "character"}


class PlanningService:
    def __init__(self, knowledge_source=None, text_completion=None) -> None:
        self.knowledge_source = knowledge_source
        self.text_completion = text_completion or complete_text

    def build_planner_prompt(self, requirements: str) -> str:
        api_hints = ""
        if self.knowledge_source is not None:
            api_hints = self.knowledge_source.load_context("planner")
        return f"""You are an expert STS2 (Slay the Spire 2) mod developer and designer.
The user wants to create a Slay the Spire 2 mod. Analyze their requirements and produce a detailed, structured mod plan.

{api_hints}

User requirements:
{requirements}

Output a JSON object with this exact structure:
{{
  "mod_name": "EnglishModName",
  "summary": "一句话中文描述这个mod的主题",
  "items": [
    {{
      "id": "type_name_snake",
      "type": "card" | "card_fullscreen" | "relic" | "power" | "character" | "custom_code",
      "name": "PascalCaseEnglishName",
      "name_zhs": "简体中文显示名称",
      "description": "中文描述：这个资产是什么，玩法效果是什么",
      "implementation_notes": "Technical C# implementation guidance: which base class to inherit, key methods to override, fields to set, interactions with other items in this mod. Be specific and technical.",
      "needs_image": true | false,
      "image_description": "中文视觉描述：画面主体、外观风格、颜色、氛围（不含游戏机制数值）",
      "depends_on": ["id_of_item_this_depends_on"]
    }}
  ]
}}

Rules:
- type "custom_code": mechanics, buffs, passives, events, anything without a dedicated visual asset. needs_image = false.
- type "power": buff/debuff icons shown during battle. needs_image = true (needs a small icon).
- type "character": full player character. needs_image = true.
- For items with no visual asset (custom_code): image_description = "".
- depends_on: list IDs of items whose C# code must exist first (e.g. a card that uses a custom power depends on that power).
- implementation_notes must be detailed enough that a developer can write the C# without looking anything up. Include: base class, constructor params, methods to override, logic description, references to other items by their C# class name.
- name_zhs: the in-game display name for Simplified Chinese players. For custom_code items with no display name, use "".
- Create ONLY what the user asked for. Do not add extra items unless they are clearly implied by the requirements.
- Output ONLY the JSON, no markdown, no explanation.
- All string values must be valid JSON strings: escape double quotes as \\", backslashes as \\\\, newlines as \\n. Do NOT include raw newlines or unescaped quotes inside string values.
- implementation_notes must be a single JSON string (no embedded code blocks with triple backticks — use plain text descriptions instead).
"""

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
