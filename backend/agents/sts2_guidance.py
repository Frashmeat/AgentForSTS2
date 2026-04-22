"""
STS2 Mod Development Guidance
-----------------------------
Based on actual working examples and local guidance resources.
Used by Code Agent and Planner to avoid looking up basics from scratch.

Sources:
- E:/STS2mod/docs/ (local verified docs with real bugs & solutions)
- https://github.com/lamali292/sts2_example_mod
- https://github.com/Alchyr/BaseLib-StS2
- sts2_api_reference.md (decompiled from sts2.dll, ilspycmd v9.1.0)
"""
from __future__ import annotations

from functools import lru_cache
from app.modules.knowledge.infra import knowledge_runtime
from app.modules.knowledge.infra.sts2_guidance_source import Sts2GuidanceKnowledgeSource

_RESOURCE_DIR = knowledge_runtime.RESOURCE_KNOWLEDGE_DIR


def get_game_api_reference_path():
    return knowledge_runtime.GAME_KNOWLEDGE_DIR / knowledge_runtime.GAME_KNOWLEDGE_SEED_FILE.name


def get_baselib_runtime_source_path():
    return knowledge_runtime.BASELIB_KNOWLEDGE_DIR / "BaseLib.decompiled.cs"


def _load_api_reference() -> str:
    """Load the decompiled API reference markdown. Returns empty string if missing."""
    knowledge_runtime.ensure_runtime_knowledge_seeded()
    try:
        return get_game_api_reference_path().read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def _load_resource_text(file_name: str) -> str:
    knowledge_runtime.ensure_runtime_knowledge_seeded()
    return (_RESOURCE_DIR / file_name).read_text(encoding="utf-8").strip() + "\n"


def _get_guidance_for_asset_type_raw(asset_type: str) -> str:
    """
    Return the combined guidance bundle for a specific asset type.
    Includes: common build/structure guidance + type-specific API snippets.
    Does NOT include the full decompiled reference (too large for prompt injection).
    """
    common_guidance = _load_resource_text("common.md")
    type_guidance_map = {
        "card": _load_resource_text("card.md"),
        "card_fullscreen": _load_resource_text("card.md"),
        "relic": _load_resource_text("relic.md"),
        "power": _load_resource_text("power.md"),
        "potion": _load_resource_text("potion.md"),
        "character": _load_resource_text("character.md"),
        "custom_code": _load_resource_text("custom_code.md") + "\n" + _load_resource_text("potion.md") + "\n" + _load_resource_text("character.md"),
    }
    return common_guidance + type_guidance_map.get(asset_type, "")


def _get_planner_guidance_raw() -> str:
    """Return the compact planner guidance from the resource bundle."""
    return _load_resource_text("planner_guidance.md")


_GUIDANCE_SOURCE = Sts2GuidanceKnowledgeSource(
    guidance_for_asset_type=_get_guidance_for_asset_type_raw,
    planner_guidance=_get_planner_guidance_raw,
)


def get_guidance_for_asset_type(asset_type: str) -> str:
    return _GUIDANCE_SOURCE.load_context("asset", asset_type=asset_type)


def get_planner_guidance() -> str:
    return _GUIDANCE_SOURCE.load_context("planner")


@lru_cache(maxsize=1)
def get_full_guidance_bundle() -> str:
    """Return the aggregated runtime guidance bundle lazily."""
    return (
        _load_resource_text("common.md")
        + _load_resource_text("card.md")
        + _load_resource_text("relic.md")
        + _load_resource_text("power.md")
        + _load_resource_text("custom_code.md")
    )
