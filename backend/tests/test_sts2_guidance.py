"""Tests for sts2_guidance.py — runtime guidance accessors."""
import sys
from pathlib import Path

# 让 pytest 能找到 backend 模块
sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra import knowledge_runtime
from agents.sts2_guidance import (
    get_guidance_for_asset_type,
    get_planner_guidance,
    get_game_api_reference_path,
    get_full_guidance_bundle,
)

RESOURCE_DIR = knowledge_runtime.RESOURCE_KNOWLEDGE_DIR


def _ensure_runtime_knowledge_available() -> None:
    knowledge_runtime.ensure_runtime_knowledge_seeded()


def test_api_ref_file_exists():
    """The runtime knowledge API reference markdown must be present."""
    _ensure_runtime_knowledge_available()
    api_ref_path = get_game_api_reference_path()
    assert api_ref_path.exists(), f"Missing file: {api_ref_path}"


def test_api_ref_file_not_empty():
    _ensure_runtime_knowledge_available()
    content = get_game_api_reference_path().read_text(encoding="utf-8")
    assert len(content) > 1000, "API reference file looks too small"


def test_api_ref_path_resolves_from_runtime_knowledge_dir(monkeypatch, tmp_path: Path):
    runtime_dir = tmp_path / "runtime" / "knowledge" / "game"
    runtime_dir.mkdir(parents=True)
    seed_file = tmp_path / "seed" / "sts2_api_reference.md"
    seed_file.parent.mkdir(parents=True, exist_ok=True)
    seed_file.write_text("reference", encoding="utf-8")

    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_SEED_FILE", seed_file, raising=False)

    assert get_game_api_reference_path() == runtime_dir / "sts2_api_reference.md"


def test_sts2_resource_files_exist():
    _ensure_runtime_knowledge_available()
    expected_files = [
        "common.md",
        "card.md",
        "relic.md",
        "power.md",
        "potion.md",
        "character.md",
        "custom_code.md",
        "planner_guidance.md",
    ]

    for file_name in expected_files:
        assert (RESOURCE_DIR / file_name).exists(), f"Missing resource file: {file_name}"


# ── get_guidance_for_asset_type ───────────────────────────────────────────────

def test_card_guidance_contains_onplay():
    guidance = get_guidance_for_asset_type("card")
    assert "OnPlay" in guidance, "card guidance must mention OnPlay method"


def test_card_guidance_contains_pool_attribute():
    guidance = get_guidance_for_asset_type("card")
    assert "[Pool(" in guidance, "card guidance must mention [Pool()] attribute"


def test_card_guidance_contains_cardtype_enum():
    guidance = get_guidance_for_asset_type("card")
    assert "CardType" in guidance


def test_relic_guidance_contains_shouldreceivecombathooks():
    guidance = get_guidance_for_asset_type("relic")
    assert "ShouldReceiveCombatHooks" in guidance


def test_relic_guidance_contains_relicrority():
    guidance = get_guidance_for_asset_type("relic")
    assert "RelicRarity" in guidance


def test_relic_guidance_contains_flash():
    guidance = get_guidance_for_asset_type("relic")
    assert "Flash()" in guidance


def test_power_guidance_contains_powermodel():
    guidance = get_guidance_for_asset_type("power")
    assert "PowerModel" in guidance


def test_power_guidance_contains_powertype():
    guidance = get_guidance_for_asset_type("power")
    assert "PowerType" in guidance


def test_power_guidance_contains_powerstacktype():
    guidance = get_guidance_for_asset_type("power")
    assert "PowerStackType" in guidance


def test_card_guidance_does_not_contain_power_model_class():
    """Card guidance should not contain PowerModel class definition (wrong type contamination)."""
    guidance = get_guidance_for_asset_type("card")
    # PowerModel the C# class should only be in power guidance, not card guidance
    assert "class MyBuff : PowerModel" not in guidance


def test_all_types_contain_common_gotchas():
    """All types must include common gotchas (dotnet publish, etc.)."""
    for t in ["card", "relic", "power", "custom_code"]:
        guidance = get_guidance_for_asset_type(t)
        assert "dotnet publish" in guidance, f"type={t} guidance missing dotnet publish note"


def test_unknown_type_returns_common_guidance():
    """Unknown asset type should still return the common guidance (not crash)."""
    guidance = get_guidance_for_asset_type("unknown_future_type")
    assert "dotnet publish" in guidance
    assert len(guidance) > 100


def test_card_guidance_includes_resource_backed_common_and_type_sections():
    guidance = get_guidance_for_asset_type("card")
    common_text = (RESOURCE_DIR / "common.md").read_text(encoding="utf-8").strip()
    card_text = (RESOURCE_DIR / "card.md").read_text(encoding="utf-8").strip()

    assert common_text in guidance
    assert card_text in guidance


# ── get_planner_guidance ──────────────────────────────────────────────────────

def test_planner_guidance_contains_onplay():
    guidance = get_planner_guidance()
    assert "OnPlay" in guidance


def test_planner_guidance_contains_shouldreceivecombathooks():
    guidance = get_planner_guidance()
    assert "ShouldReceiveCombatHooks" in guidance


def test_planner_guidance_contains_real_base_classes():
    guidance = get_planner_guidance()
    assert "CustomCardModel" in guidance
    assert "RelicModel" in guidance
    assert "PowerModel" in guidance


def test_planner_guidance_contains_hook_examples():
    guidance = get_planner_guidance()
    assert "AfterDamageGiven" in guidance
    assert "AfterCardPlayed" in guidance
    assert "AfterPlayerTurnStart" in guidance


def test_planner_guidance_compact_size():
    """Planner guidance should be concise — not the full guidance dump."""
    guidance = get_planner_guidance()
    full_guidance = get_full_guidance_bundle()
    assert len(guidance) < len(full_guidance) / 2, "planner guidance should be much smaller than full guidance"


def test_planner_guidance_matches_resource_file():
    guidance = get_planner_guidance()
    resource_text = (RESOURCE_DIR / "planner_guidance.md").read_text(encoding="utf-8").strip()

    assert guidance.strip() == resource_text


# ── combined full guidance bundle ────────────────────────────────────────────

def test_full_guidance_bundle_covers_all_types():
    """The aggregated guidance bundle should cover all major asset types."""
    full_guidance = get_full_guidance_bundle()
    assert "OnPlay" in full_guidance
    assert "RelicModel" in full_guidance
    assert "PowerModel" in full_guidance
    assert "ShouldReceiveCombatHooks" in full_guidance
