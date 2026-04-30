import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra.sts2_code_facts_provider import Sts2CodeFactsProvider
from app.shared.contracts.knowledge import KnowledgeQuery


def _keys(items) -> set[str]:
    return {item.key for item in items}


def test_sts2_code_facts_provider_returns_card_base_class_fact():
    provider = Sts2CodeFactsProvider()

    facts, warnings = provider.build_facts(KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="card"))

    assert "sts2.card.base_class" in _keys(facts)
    assert isinstance(warnings, list)


def test_sts2_code_facts_provider_returns_power_base_class_fact():
    provider = Sts2CodeFactsProvider()

    facts, _warnings = provider.build_facts(KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="power"))

    assert "sts2.power.base_class" in _keys(facts)


def test_sts2_code_facts_provider_returns_relic_base_class_fact():
    provider = Sts2CodeFactsProvider()

    facts, _warnings = provider.build_facts(KnowledgeQuery(scenario="asset_codegen", domain="sts2", asset_type="relic"))

    assert "sts2.relic.base_class" in _keys(facts)


def test_sts2_code_facts_provider_returns_custom_code_patch_fact():
    provider = Sts2CodeFactsProvider()

    facts, _warnings = provider.build_facts(
        KnowledgeQuery(scenario="custom_code_codegen", domain="sts2", asset_type="custom_code")
    )

    assert "sts2.custom_code.patching" in _keys(facts)


def test_sts2_code_facts_provider_adds_card_selector_fact_for_selection_keywords():
    provider = Sts2CodeFactsProvider()

    facts, _warnings = provider.build_facts(
        KnowledgeQuery(
            scenario="custom_code_codegen",
            domain="sts2",
            asset_type="custom_code",
            requirements="需要选牌并 upgrade 一张卡",
        )
    )

    assert "sts2.selection.card_selector_prefs" in _keys(facts)


def test_sts2_code_facts_provider_adds_damage_cmd_fact_for_damage_keywords():
    provider = Sts2CodeFactsProvider()

    facts, _warnings = provider.build_facts(
        KnowledgeQuery(
            scenario="asset_codegen",
            domain="sts2",
            asset_type="card",
            requirements="attack all enemies with aoe damage",
        )
    )

    assert "sts2.damage.damage_cmd" in _keys(facts)


def test_sts2_code_facts_provider_adds_power_cmd_fact_for_buff_keywords():
    provider = Sts2CodeFactsProvider()

    facts, _warnings = provider.build_facts(
        KnowledgeQuery(
            scenario="asset_codegen",
            domain="sts2",
            asset_type="power",
            requirements="施加力量 buff",
        )
    )

    assert "sts2.power.power_cmd" in _keys(facts)
