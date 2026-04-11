"""
STS2 Mod Development Knowledge Base
------------------------------------
Based on actual working examples and local documentation.
Used by Code Agent and Planner to avoid looking up basics from scratch.

Sources:
- E:/STS2mod/docs/ (local verified docs with real bugs & solutions)
- https://github.com/lamali292/sts2_example_mod
- https://github.com/Alchyr/BaseLib-StS2
- sts2_api_reference.md (decompiled from sts2.dll, ilspycmd v9.1.0)
"""
from __future__ import annotations

from pathlib import Path

from app.modules.knowledge.infra import knowledge_runtime
from app.modules.knowledge.infra.sts2_docs_source import Sts2DocsKnowledgeSource

API_REF_PATH = knowledge_runtime.GAME_DECOMPILED_DIR / Path(knowledge_runtime.GAME_KNOWLEDGE_SEED_FILE).name
BASELIB_SRC_PATH = knowledge_runtime.BASELIB_DECOMPILED_DIR / "BaseLib.decompiled.cs"
_RESOURCE_DIR = knowledge_runtime.RESOURCE_KNOWLEDGE_DIR


def _load_api_reference() -> str:
    """Load the decompiled API reference markdown. Returns empty string if missing."""
    knowledge_runtime.ensure_runtime_knowledge_seeded()
    try:
        return API_REF_PATH.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def _load_resource_text(file_name: str) -> str:
    knowledge_runtime.ensure_runtime_knowledge_seeded()
    return (_RESOURCE_DIR / file_name).read_text(encoding="utf-8").strip() + "\n"


def _get_docs_for_type_raw(asset_type: str) -> str:
    """
    Return the combined knowledge base for a specific asset type.
    Includes: common build/structure docs + type-specific API snippet.
    Does NOT include the full decompiled reference (too large for prompt injection).
    """
    common_docs = _load_resource_text("common.md")
    type_docs_map = {
        "card": _load_resource_text("card.md"),
        "card_fullscreen": _load_resource_text("card.md"),
        "relic": _load_resource_text("relic.md"),
        "power": _load_resource_text("power.md"),
        "potion": _load_resource_text("potion.md"),
        "character": _load_resource_text("character.md"),
        "custom_code": _load_resource_text("custom_code.md") + "\n" + _load_resource_text("potion.md") + "\n" + _load_resource_text("character.md"),
    }
    return common_docs + type_docs_map.get(asset_type, "")


def _get_planner_api_hints_raw() -> str:
    """Return the compact planner hints from the resource bundle."""
    return _load_resource_text("planner_hints.md")


_KNOWLEDGE_SOURCE = Sts2DocsKnowledgeSource(
    docs_for_type=_get_docs_for_type_raw,
    planner_hints=_get_planner_api_hints_raw,
)


def get_docs_for_type(asset_type: str) -> str:
    return _KNOWLEDGE_SOURCE.load_context("asset", asset_type=asset_type)


def get_planner_api_hints() -> str:
    return _KNOWLEDGE_SOURCE.load_context("planner")


# Aggregated docs export for tooling that needs the full combined reference.
# Prefer get_docs_for_type(asset_type) in new code.
STS2_MOD_DOCS = (
    _load_resource_text("common.md")
    + _load_resource_text("card.md")
    + _load_resource_text("relic.md")
    + _load_resource_text("power.md")
    + _load_resource_text("custom_code.md")
)
