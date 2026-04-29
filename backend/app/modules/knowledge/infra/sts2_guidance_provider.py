from __future__ import annotations

from pathlib import Path

from app.modules.knowledge.infra import knowledge_runtime
from app.shared.contracts.knowledge import KnowledgeGuidanceItem, KnowledgeQuery


class Sts2GuidanceProvider:
    def build_guidance(self, query: KnowledgeQuery) -> list[KnowledgeGuidanceItem]:
        knowledge_runtime.ensure_runtime_knowledge_seeded()
        resource_root = knowledge_runtime.active_resource_knowledge_dir()
        if query.scenario == "planner":
            planner_path = resource_root / "planner_guidance.md"
            return [self._item("sts2.guidance.planner", "Planner hints", planner_path, ["planner"])]

        files = [("sts2.guidance.common", "Common guidance", resource_root / "common.md", ["card", "power", "relic", "custom_code", "character"])]
        for asset_type in self._iter_asset_types(query):
            for key, title, path, asset_types in self._files_for_asset_type(asset_type, resource_root):
                files.append((key, title, path, asset_types))

        guidance: list[KnowledgeGuidanceItem] = []
        seen: set[str] = set()
        for key, title, path, asset_types in files:
            if key in seen or not path.exists():
                continue
            seen.add(key)
            guidance.append(self._item(key, title, path, asset_types))
        return guidance

    @staticmethod
    def _item(key: str, title: str, path: Path, asset_types: list[str]) -> KnowledgeGuidanceItem:
        text = path.read_text(encoding="utf-8", errors="replace").strip()
        return KnowledgeGuidanceItem(
            key=key,
            title=title,
            body=text,
            source_path=str(path),
            asset_types=asset_types,
        )

    @staticmethod
    def _iter_asset_types(query: KnowledgeQuery) -> list[str]:
        ordered: list[str] = []
        if query.asset_type:
            ordered.append(query.asset_type)
        for asset_type in query.group_asset_types:
            if asset_type not in ordered:
                ordered.append(asset_type)
        if not ordered and query.scenario == "custom_code_codegen":
            ordered.append("custom_code")
        return ordered

    @staticmethod
    def _files_for_asset_type(asset_type: str, resource_root: Path) -> list[tuple[str, str, Path, list[str]]]:
        mapping = {
            "card": [("sts2.guidance.card", "Card guidance", resource_root / "card.md", ["card", "card_fullscreen"])],
            "card_fullscreen": [("sts2.guidance.card", "Card guidance", resource_root / "card.md", ["card", "card_fullscreen"])],
            "power": [("sts2.guidance.power", "Power guidance", resource_root / "power.md", ["power"])],
            "relic": [("sts2.guidance.relic", "Relic guidance", resource_root / "relic.md", ["relic"])],
            "character": [("sts2.guidance.character", "Character guidance", resource_root / "character.md", ["character"])],
            "custom_code": [
                ("sts2.guidance.custom_code", "Custom code guidance", resource_root / "custom_code.md", ["custom_code"]),
                ("sts2.guidance.potion", "Potion guidance", resource_root / "potion.md", ["custom_code"]),
                ("sts2.guidance.character", "Character guidance", resource_root / "character.md", ["custom_code", "character"]),
            ],
        }
        return mapping.get(asset_type, [])
