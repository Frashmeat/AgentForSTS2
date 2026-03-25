from __future__ import annotations


class Sts2DocsKnowledgeSource:
    def __init__(self, docs_for_type, planner_hints) -> None:
        self._docs_for_type = docs_for_type
        self._planner_hints = planner_hints

    def load_context(self, context_type: str, asset_type: str | None = None) -> str:
        if context_type == "planner":
            return self._planner_hints()
        if asset_type is not None:
            return self._docs_for_type(asset_type)
        return ""
