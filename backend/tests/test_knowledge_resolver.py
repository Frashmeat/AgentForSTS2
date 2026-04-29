import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra.sts2_knowledge_resolver import Sts2KnowledgeResolver
from app.shared.contracts.knowledge import KnowledgeQuery


def _fact_keys(packet) -> set[str]:
    return {item.key for item in packet.facts}


def test_sts2_knowledge_resolver_returns_public_facts_for_asset_codegen():
    resolver = Sts2KnowledgeResolver()

    packet = resolver.resolve(
        KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="card")
    )

    assert packet.domain == "sts2"
    assert packet.scenario == "asset_codegen"
    assert "sts2.runtime.knowledge_paths" in _fact_keys(packet)


def test_sts2_knowledge_resolver_merges_type_specific_facts_for_card():
    resolver = Sts2KnowledgeResolver()

    packet = resolver.resolve(
        KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="card")
    )

    assert "sts2.card.base_class" in _fact_keys(packet)
    assert packet.guidance
    assert packet.lookup


def test_sts2_knowledge_resolver_adds_requirement_triggered_facts():
    resolver = Sts2KnowledgeResolver()

    packet = resolver.resolve(
        KnowledgeQuery(
            scenario="custom_code_codegen",
            domain="sts2",
            asset_type="custom_code",
            requirements="需要选牌 upgrade 并造成伤害",
        )
    )

    keys = _fact_keys(packet)
    assert "sts2.selection.card_selector_prefs" in keys
    assert "sts2.damage.damage_cmd" in keys


def test_sts2_knowledge_resolver_returns_lookup_and_warnings_fields():
    resolver = Sts2KnowledgeResolver()

    packet = resolver.resolve(
        KnowledgeQuery(
            scenario="asset_codegen",
            domain="sts2",
            asset_type="card",
            project_root=Path("MissingProjectRoot"),
        )
    )

    assert packet.lookup
    assert isinstance(packet.warnings, list)


def test_sts2_knowledge_resolver_group_codegen_deduplicates_asset_type_facts():
    resolver = Sts2KnowledgeResolver()

    packet = resolver.resolve(
        KnowledgeQuery(
            scenario="asset_group_codegen",
            domain="sts2",
            group_asset_types=["card", "card", "relic"],
            symbols=["Ignite", "Ignite", "BurnRelic"],
        )
    )

    keys = _fact_keys(packet)
    assert "sts2.card.base_class" in keys
    assert "sts2.relic.base_class" in keys
    assert list(keys).count("sts2.card.base_class") <= 1


def test_sts2_knowledge_resolver_prefers_active_knowledge_pack(monkeypatch, tmp_path: Path):
    from app.modules.knowledge.infra import knowledge_runtime

    active_root = tmp_path / "pack" / "content"
    resource_root = active_root / "resources" / "sts2"
    game_root = active_root / "game"
    baselib_root = active_root / "baselib"
    resource_root.mkdir(parents=True)
    game_root.mkdir(parents=True)
    baselib_root.mkdir(parents=True)
    (resource_root / "common.md").write_text("active common guidance\n", encoding="utf-8")
    (resource_root / "card.md").write_text("active card guidance\n", encoding="utf-8")
    (game_root / "Game.cs").write_text("// active game\n", encoding="utf-8")
    (baselib_root / "BaseLib.decompiled.cs").write_text("// active baselib\n", encoding="utf-8")

    monkeypatch.setattr(knowledge_runtime, "active_resource_knowledge_dir", lambda: resource_root)
    monkeypatch.setattr(knowledge_runtime, "active_game_knowledge_dir", lambda: game_root)
    monkeypatch.setattr(knowledge_runtime, "active_baselib_knowledge_dir", lambda: baselib_root)

    packet = Sts2KnowledgeResolver().resolve(
        KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="card")
    )

    assert any(item.body == "active card guidance" for item in packet.guidance)
    assert any(str(game_root) in item.body for item in packet.facts)
    assert any(item.path == str(game_root) for item in packet.lookup)
