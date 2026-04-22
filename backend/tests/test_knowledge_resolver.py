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
