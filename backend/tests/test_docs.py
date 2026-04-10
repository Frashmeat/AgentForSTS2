"""Tests for sts2_docs.py — API knowledge base functions."""
import sys
from pathlib import Path

# 让 pytest 能找到 backend 模块
sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra import knowledge_runtime
from agents.sts2_docs import (
    get_docs_for_type,
    get_planner_api_hints,
    API_REF_PATH,
    STS2_MOD_DOCS,
)

RESOURCE_DIR = knowledge_runtime.RESOURCE_KNOWLEDGE_DIR


def test_api_ref_file_exists():
    """The runtime knowledge API reference markdown must be present."""
    assert API_REF_PATH.exists(), f"Missing file: {API_REF_PATH}"


def test_api_ref_file_not_empty():
    content = API_REF_PATH.read_text(encoding="utf-8")
    assert len(content) > 1000, "API reference file looks too small"


def test_sts2_resource_files_exist():
    expected_files = [
        "common.md",
        "card.md",
        "relic.md",
        "power.md",
        "potion.md",
        "character.md",
        "custom_code.md",
        "planner_hints.md",
    ]

    for file_name in expected_files:
        assert (RESOURCE_DIR / file_name).exists(), f"Missing resource file: {file_name}"


# ── get_docs_for_type ─────────────────────────────────────────────────────────

def test_card_docs_contain_onplay():
    docs = get_docs_for_type("card")
    assert "OnPlay" in docs, "card docs must mention OnPlay method"


def test_card_docs_contain_pool_attribute():
    docs = get_docs_for_type("card")
    assert "[Pool(" in docs, "card docs must mention [Pool()] attribute"


def test_card_docs_contain_cardtype_enum():
    docs = get_docs_for_type("card")
    assert "CardType" in docs


def test_relic_docs_contain_shouldreceivecombathooks():
    docs = get_docs_for_type("relic")
    assert "ShouldReceiveCombatHooks" in docs


def test_relic_docs_contain_relicrority():
    docs = get_docs_for_type("relic")
    assert "RelicRarity" in docs


def test_relic_docs_contain_flash():
    docs = get_docs_for_type("relic")
    assert "Flash()" in docs


def test_power_docs_contain_powermodel():
    docs = get_docs_for_type("power")
    assert "PowerModel" in docs


def test_power_docs_contain_powertype():
    docs = get_docs_for_type("power")
    assert "PowerType" in docs


def test_power_docs_contain_powerstacktype():
    docs = get_docs_for_type("power")
    assert "PowerStackType" in docs


def test_card_docs_not_contain_power_model_class():
    """Card docs should not contain PowerModel class definition (wrong type contamination)."""
    docs = get_docs_for_type("card")
    # PowerModel the C# class should only be in power docs, not card docs
    assert "class MyBuff : PowerModel" not in docs


def test_all_types_contain_common_gotchas():
    """All types must include common gotchas (dotnet publish, etc.)."""
    for t in ["card", "relic", "power", "custom_code"]:
        docs = get_docs_for_type(t)
        assert "dotnet publish" in docs, f"type={t} docs missing dotnet publish note"


def test_unknown_type_returns_common_docs():
    """Unknown asset type should still return the common docs (not crash)."""
    docs = get_docs_for_type("unknown_future_type")
    assert "dotnet publish" in docs
    assert len(docs) > 100


def test_card_docs_include_resource_backed_common_and_type_sections():
    docs = get_docs_for_type("card")
    common_text = (RESOURCE_DIR / "common.md").read_text(encoding="utf-8").strip()
    card_text = (RESOURCE_DIR / "card.md").read_text(encoding="utf-8").strip()

    assert common_text in docs
    assert card_text in docs


# ── get_planner_api_hints ─────────────────────────────────────────────────────

def test_planner_hints_contain_onplay():
    hints = get_planner_api_hints()
    assert "OnPlay" in hints


def test_planner_hints_contain_shouldreceivecombathooks():
    hints = get_planner_api_hints()
    assert "ShouldReceiveCombatHooks" in hints


def test_planner_hints_contain_real_base_classes():
    hints = get_planner_api_hints()
    assert "CustomCardModel" in hints
    assert "RelicModel" in hints
    assert "PowerModel" in hints


def test_planner_hints_contain_hook_examples():
    hints = get_planner_api_hints()
    assert "AfterDamageGiven" in hints
    assert "AfterCardPlayed" in hints
    assert "AfterPlayerTurnStart" in hints


def test_planner_hints_compact_size():
    """Hints should be concise — not the full docs dump."""
    hints = get_planner_api_hints()
    full_docs = STS2_MOD_DOCS
    assert len(hints) < len(full_docs) / 2, "planner hints should be much smaller than full docs"


def test_planner_hints_match_resource_file():
    hints = get_planner_api_hints()
    resource_text = (RESOURCE_DIR / "planner_hints.md").read_text(encoding="utf-8").strip()

    assert hints.strip() == resource_text


# ── legacy STS2_MOD_DOCS ──────────────────────────────────────────────────────

def test_legacy_docs_combine_all_types():
    """STS2_MOD_DOCS should cover all major types for backwards compatibility."""
    assert "OnPlay" in STS2_MOD_DOCS
    assert "RelicModel" in STS2_MOD_DOCS
    assert "PowerModel" in STS2_MOD_DOCS
    assert "ShouldReceiveCombatHooks" in STS2_MOD_DOCS
