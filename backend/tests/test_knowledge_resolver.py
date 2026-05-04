import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra.sts2_knowledge_resolver import Sts2KnowledgeResolver
from app.shared.contracts.knowledge import KnowledgeQuery


def _prepare_runtime_knowledge(monkeypatch, tmp_path: Path):
    from app.modules.knowledge.infra import knowledge_runtime

    runtime_root = tmp_path / "runtime" / "knowledge"
    resource_root = runtime_root / "resources" / "sts2"
    game_root = runtime_root / "game"
    baselib_root = runtime_root / "baselib"
    resource_root.mkdir(parents=True)
    game_root.mkdir(parents=True)
    baselib_root.mkdir(parents=True)
    for name in ("common.md", "card.md", "relic.md", "power.md", "custom_code.md"):
        (resource_root / name).write_text(f"{name} guidance\n", encoding="utf-8")
    (game_root / "Game.cs").write_text("// runtime game\n", encoding="utf-8")
    (baselib_root / "BaseLib.decompiled.cs").write_text("// runtime baselib\n", encoding="utf-8")
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_root)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_root)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_root)
    return resource_root, game_root, baselib_root


def _fact_keys(packet) -> set[str]:
    return {item.key for item in packet.facts}


def test_sts2_knowledge_resolver_returns_public_facts_for_asset_codegen(monkeypatch, tmp_path: Path):
    _prepare_runtime_knowledge(monkeypatch, tmp_path)
    resolver = Sts2KnowledgeResolver()

    packet = resolver.resolve(KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="card"))

    assert packet.domain == "sts2"
    assert packet.scenario == "asset_codegen"
    assert "sts2.runtime.knowledge_paths" in _fact_keys(packet)


def test_sts2_knowledge_resolver_merges_type_specific_facts_for_card(monkeypatch, tmp_path: Path):
    _prepare_runtime_knowledge(monkeypatch, tmp_path)
    resolver = Sts2KnowledgeResolver()

    packet = resolver.resolve(KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="card"))

    assert "sts2.card.base_class" in _fact_keys(packet)
    assert packet.guidance
    assert packet.lookup


def test_sts2_knowledge_resolver_adds_requirement_triggered_facts(monkeypatch, tmp_path: Path):
    _prepare_runtime_knowledge(monkeypatch, tmp_path)
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


def test_sts2_knowledge_resolver_returns_lookup_and_warnings_fields(monkeypatch, tmp_path: Path):
    _prepare_runtime_knowledge(monkeypatch, tmp_path)
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


def test_sts2_knowledge_resolver_group_codegen_deduplicates_asset_type_facts(monkeypatch, tmp_path: Path):
    _prepare_runtime_knowledge(monkeypatch, tmp_path)
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


def test_sts2_knowledge_resolver_uses_runtime_knowledge_dirs(monkeypatch, tmp_path: Path):
    resource_root, game_root, _baselib_root = _prepare_runtime_knowledge(monkeypatch, tmp_path)
    (resource_root / "card.md").write_text("runtime card guidance\n", encoding="utf-8")

    packet = Sts2KnowledgeResolver().resolve(KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="card"))

    assert any(item.body == "runtime card guidance" for item in packet.guidance)
    assert any(str(game_root) in item.body for item in packet.facts)
    assert any(item.path == str(game_root) for item in packet.lookup)


def test_sts2_knowledge_resolver_treats_api_reference_only_game_as_missing(monkeypatch, tmp_path: Path):
    from app.modules.knowledge.infra import knowledge_runtime

    game_root = tmp_path / "runtime" / "knowledge" / "game"
    resource_root = tmp_path / "runtime" / "knowledge" / "resources" / "sts2"
    baselib_root = tmp_path / "runtime" / "knowledge" / "baselib"
    game_root.mkdir(parents=True)
    resource_root.mkdir(parents=True)
    baselib_root.mkdir(parents=True)
    (game_root / "sts2_api_reference.md").write_text("reference only\n", encoding="utf-8")
    (resource_root / "card.md").write_text("card guidance\n", encoding="utf-8")
    (baselib_root / "BaseLib.decompiled.cs").write_text("// baselib\n", encoding="utf-8")

    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_root)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_root)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_root)

    packet = Sts2KnowledgeResolver().resolve(KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="card"))

    assert any(item.title == "STS2 ilspy fallback" for item in packet.lookup)
    assert not any(item.title == "STS2 runtime knowledge directory" for item in packet.lookup)
    runtime_fact = next(item for item in packet.facts if item.key == "sts2.runtime.knowledge_paths")
    assert "Runtime-decompiled STS2 game sources are missing" in runtime_fact.body
    assert "source of truth" not in runtime_fact.body
