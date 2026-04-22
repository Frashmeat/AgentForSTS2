from __future__ import annotations


class Sts2GuidanceKnowledgeSource:
    def __init__(self, guidance_for_asset_type, planner_guidance) -> None:
        self._guidance_for_asset_type = guidance_for_asset_type
        self._planner_guidance = planner_guidance

    def load_context(self, context_type: str, asset_type: str | None = None) -> str:
        if context_type == "planner":
            return self._planner_guidance()
        if asset_type is not None:
            return self._guidance_for_asset_type(asset_type)
        return ""
