import inspect
import sys
import types
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))
sys.modules.setdefault(
    "llm.text_runner",
    types.SimpleNamespace(complete_text=lambda *args, **kwargs: None),
)

from app.modules.planning.application.services import PlanningService
from app.modules.planning.domain.models import PlanItem
from app.shared.contracts.knowledge import KnowledgePacket, KnowledgeQuery
from app.shared.prompting import PromptLoader

_PROMPT_LOADER_SUPPORTED = "prompt_loader" in inspect.signature(PlanningService).parameters


class FakeKnowledgeSource:
    def load_context(self, context_type: str, asset_type: str | None = None) -> str:
        if context_type == "planner":
            return "OnPlay PowerModel ShouldReceiveCombatHooks"
        return f"guidance:{asset_type or ''}"


class FakeKnowledgeResolver:
    def resolve(self, query: KnowledgeQuery) -> KnowledgePacket:
        return KnowledgePacket(
            domain=query.domain,
            scenario=query.scenario,
            summary="planner-summary",
        )


class FakePromptContextAssembler:
    def assemble(self, packet: KnowledgePacket) -> dict[str, str]:
        return {
            "facts": "FACTS",
            "guidance": "GUIDANCE",
            "lookup": "LOOKUP",
            "knowledge_warnings": "WARNINGS",
            "summary": packet.summary,
        }


def _build_service(
    knowledge_source=None,
    prompt_loader=None,
    knowledge_resolver=None,
    prompt_context_assembler=None,
) -> PlanningService:
    kwargs = {}
    if knowledge_source is not None:
        kwargs["knowledge_source"] = knowledge_source
    if _PROMPT_LOADER_SUPPORTED and prompt_loader is not None:
        kwargs["prompt_loader"] = prompt_loader
    if knowledge_resolver is not None:
        kwargs["knowledge_resolver"] = knowledge_resolver
    if prompt_context_assembler is not None:
        kwargs["prompt_context_assembler"] = prompt_context_assembler
    return PlanningService(**kwargs)


def test_planning_service_accepts_prompt_loader_dependency():
    assert _PROMPT_LOADER_SUPPORTED is True


def test_planning_service_topologically_sorts_plan_items():
    service = PlanningService(knowledge_source=FakeKnowledgeSource())
    items = [
        PlanItem(id="card_ignite", type="card", name="Ignite", depends_on=["power_burn"]),
        PlanItem(id="power_burn", type="power", name="BurnPower"),
    ]

    result = service.topologically_sort_plan_items(items)

    assert [item.id for item in result] == ["power_burn", "card_ignite"]


def test_planning_service_uses_knowledge_source_for_prompt():
    service = _build_service(knowledge_source=FakeKnowledgeSource())

    prompt = service.build_planner_prompt("make a burning card")

    assert "### Planner Guidance" in prompt
    assert "OnPlay" in prompt
    assert "make a burning card" in prompt


def test_planning_service_falls_back_to_legacy_planner_guidance_when_resolver_missing():
    service = _build_service(knowledge_source=FakeKnowledgeSource())

    prompt = service.build_planner_prompt("keep legacy planner guidance")

    assert "Structured planner fact calibration is unavailable in this legacy fallback path." in prompt
    assert "OnPlay PowerModel ShouldReceiveCombatHooks" in prompt
    assert "Legacy planner fallback is active." in prompt


def test_planning_service_prefers_resolver_when_available():
    class GuardKnowledgeSource:
        def load_context(self, context_type: str, asset_type: str | None = None) -> str:
            raise AssertionError("legacy planner guidance should not be used when resolver is available")

    service = _build_service(
        knowledge_source=GuardKnowledgeSource(),
        knowledge_resolver=FakeKnowledgeResolver(),
        prompt_context_assembler=FakePromptContextAssembler(),
    )

    prompt = service.build_planner_prompt("plan a calibrated burn package")

    assert "### Planner Guidance" in prompt
    assert "GUIDANCE" in prompt
    assert "### Code Facts Check" in prompt
    assert "FACTS" in prompt
    assert "### Further Lookup" in prompt
    assert "LOOKUP" in prompt


def test_planning_prompt_template_exists_for_real_loader():
    loader = PromptLoader()

    template = loader.load("runtime_agent.planning_planner_prompt")

    assert "{{ guidance }}" in template
    assert "{{ facts }}" in template
    assert "{{ lookup }}" in template
    assert "{{ knowledge_warnings }}" in template
    assert "{{ requirements }}" in template
    assert '"implementation_notes"' in template
    assert '"depends_on"' in template


def test_planning_service_build_planner_prompt_uses_prompt_loader():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(
            self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None
        ) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-planner-prompt"

    loader = FakePromptLoader()
    service = _build_service(knowledge_source=FakeKnowledgeSource(), prompt_loader=loader)

    prompt = service.build_planner_prompt("make a burning card")

    assert prompt == "rendered-planner-prompt"
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "runtime_agent.planning_planner_prompt"
    assert set(variables) == {"guidance", "facts", "lookup", "knowledge_warnings", "requirements"}
    assert "Structured planner fact calibration is unavailable" in variables["facts"]
    assert variables["guidance"] == "OnPlay PowerModel ShouldReceiveCombatHooks"
    assert "runtime/knowledge/game/" in variables["lookup"]
    assert "Legacy planner fallback is active." in variables["knowledge_warnings"]
    assert variables["requirements"] == "make a burning card"
    assert fallback_template == ""


def test_planning_service_build_planner_prompt_uses_resolver_context_when_available():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(
            self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None
        ) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-planner-prompt"

    loader = FakePromptLoader()
    service = _build_service(
        knowledge_source=FakeKnowledgeSource(),
        prompt_loader=loader,
        knowledge_resolver=FakeKnowledgeResolver(),
        prompt_context_assembler=FakePromptContextAssembler(),
    )

    prompt = service.build_planner_prompt("make a calibrated burn card")

    assert prompt == "rendered-planner-prompt"
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "runtime_agent.planning_planner_prompt"
    assert set(variables) == {"guidance", "facts", "lookup", "knowledge_warnings", "requirements"}
    assert variables["guidance"] == "GUIDANCE"
    assert variables["facts"] == "FACTS"
    assert variables["lookup"] == "LOOKUP"
    assert variables["knowledge_warnings"] == "WARNINGS"
    assert variables["requirements"] == "make a calibrated burn card"
    assert fallback_template == ""


def test_planning_service_build_planner_prompt_uses_shared_bundle_key_without_local_fallback(monkeypatch):
    from app.modules.planning.application import services as planning_services
    from app.shared.prompting import PromptNotFoundError

    service = planning_services.PlanningService(
        knowledge_source=FakeKnowledgeSource(),
        prompt_loader=PromptLoader(root=Path(__file__).parent / "missing-planning-prompts"),
    )

    with pytest.raises(PromptNotFoundError, match="runtime_agent.planning_planner_prompt"):
        service.build_planner_prompt("make a burning card")


def test_planning_service_parse_plan_infers_needs_image_from_item_type():
    service = PlanningService()

    plan = service.parse_plan(
        """
        {
          "mod_name": "InferImageMod",
          "summary": "test",
          "items": [
            {
              "id": "power_burn",
              "type": "power",
              "name": "BurnPower"
            },
            {
              "id": "helper_logic",
              "type": "custom_code",
              "name": "HelperLogic"
            }
          ]
        }
        """
    )

    assert [item.needs_image for item in plan.items] == [True, False]


def test_planning_service_find_groups_clusters_connected_items():
    service = PlanningService()
    items = [
        PlanItem(id="power_burn", type="power", name="BurnPower"),
        PlanItem(id="card_ignite", type="card", name="Ignite", depends_on=["power_burn"]),
        PlanItem(id="relic_ember", type="relic", name="EmberRelic"),
    ]

    groups = service.find_groups(items)

    assert [[item.id for item in group] for group in groups] == [
        ["power_burn", "card_ignite"],
        ["relic_ember"],
    ]


def test_planning_service_plan_from_dict_preserves_provided_image_payload():
    service = PlanningService()

    plan = service.plan_from_dict(
        {
            "mod_name": "ImagePayloadMod",
            "summary": "",
            "items": [
                {
                    "id": "card_ignite",
                    "type": "card",
                    "name": "Ignite",
                    "provided_image_b64": "ZmFrZS1pbWFnZQ==",
                }
            ],
        }
    )

    assert plan.items[0].provided_image_b64 == "ZmFrZS1pbWFnZQ=="


def test_planning_service_parse_plan_defaults_new_review_fields():
    service = PlanningService()

    plan = service.parse_plan(
        """
        {
          "mod_name": "ReviewDefaultsMod",
          "summary": "",
          "items": [
            {
              "id": "card_ignite",
              "type": "card",
              "name": "Ignite",
              "description": "Deal fire damage."
            }
          ]
        }
        """
    )

    item = plan.items[0]
    assert item.goal == ""
    assert item.detailed_description == ""
    assert item.scope_boundary == ""
    assert item.dependency_reason == ""
    assert item.acceptance_notes == ""
    assert item.affected_targets == []
    assert item.coupling_kind == "unclear"
    assert item.clarification_status == ""
    assert item.clarification_questions == []


def test_planning_service_plan_from_dict_preserves_review_fields():
    service = PlanningService()

    plan = service.plan_from_dict(
        {
            "mod_name": "ReviewFieldsMod",
            "summary": "",
            "items": [
                {
                    "id": "helper_logic",
                    "type": "custom_code",
                    "name": "HelperLogic",
                    "goal": "Provide shared combat helper behavior.",
                    "detailed_description": "Add a reusable helper for combat callbacks and state sync.",
                    "scope_boundary": "Only add helper abstractions, do not patch unrelated rewards flow.",
                    "dependency_reason": "Needs card items to call into the helper after it exists.",
                    "acceptance_notes": "Helper API is available to card/relic integrations.",
                    "affected_targets": ["CombatHooks", "HelperLogic"],
                    "coupling_kind": "shared_logic",
                    "clarification_status": "needs_user_input",
                    "clarification_questions": ["Should the helper own combat state persistence?"],
                }
            ],
        }
    )

    item = plan.items[0]
    assert item.goal == "Provide shared combat helper behavior."
    assert item.detailed_description == "Add a reusable helper for combat callbacks and state sync."
    assert item.scope_boundary == "Only add helper abstractions, do not patch unrelated rewards flow."
    assert item.dependency_reason == "Needs card items to call into the helper after it exists."
    assert item.acceptance_notes == "Helper API is available to card/relic integrations."
    assert item.affected_targets == ["CombatHooks", "HelperLogic"]
    assert item.coupling_kind == "shared_logic"
    assert item.clarification_status == "needs_user_input"
    assert item.clarification_questions == ["Should the helper own combat state persistence?"]


def test_planning_service_validate_plan_marks_duplicate_ids_invalid():
    service = PlanningService()
    plan = service.plan_from_dict(
        {
            "mod_name": "DuplicateIdMod",
            "summary": "",
            "items": [
                {"id": "dup", "type": "card", "name": "FirstCard"},
                {"id": "dup", "type": "relic", "name": "SecondRelic"},
            ],
        }
    )

    result = service.validate_plan(plan, strictness="balanced")

    statuses = {item.item_id: item.status for item in result.items}
    assert statuses == {"dup": "invalid"}
    issue_codes = [issue.code for issue in result.items[0].issues]
    assert "duplicate_id" in issue_codes


def test_planning_service_validate_plan_marks_vague_custom_code_for_clarification():
    service = PlanningService()
    plan = service.plan_from_dict(
        {
            "mod_name": "ClarifyMod",
            "summary": "",
            "items": [
                {
                    "id": "helper_logic",
                    "type": "custom_code",
                    "name": "HelperLogic",
                    "description": "调整战斗逻辑",
                }
            ],
        }
    )

    result = service.validate_plan(plan, strictness="balanced")

    assert result.items[0].status == "needs_user_input"
    assert result.items[0].missing_fields == ["goal", "detailed_description"]
    assert result.items[0].clarification_questions


def test_planning_service_validate_plan_strictness_changes_item_status():
    service = PlanningService()
    plan = service.plan_from_dict(
        {
            "mod_name": "StrictnessMod",
            "summary": "",
            "items": [
                {
                    "id": "card_ignite",
                    "type": "card",
                    "name": "Ignite",
                    "description": "Deal 6 damage.",
                }
            ],
        }
    )

    efficient = service.validate_plan(plan, strictness="efficient")
    strict = service.validate_plan(plan, strictness="strict")

    assert efficient.items[0].status == "clear"
    assert strict.items[0].status == "needs_user_input"
    assert "goal" in strict.items[0].missing_fields


def test_planning_service_build_execution_plan_keeps_order_only_items_separate():
    service = PlanningService()
    plan = service.plan_from_dict(
        {
            "mod_name": "OrderOnlyMod",
            "summary": "",
            "items": [
                {
                    "id": "power_burn",
                    "type": "power",
                    "name": "BurnPower",
                    "coupling_kind": "order_only",
                },
                {
                    "id": "card_ignite",
                    "type": "card",
                    "name": "Ignite",
                    "depends_on": ["power_burn"],
                    "coupling_kind": "order_only",
                },
            ],
        }
    )

    result = service.build_execution_plan(plan, strictness="balanced")

    assert [group.item_ids for group in result.dependency_groups] == [["power_burn", "card_ignite"]]
    assert [bundle.item_ids for bundle in result.execution_bundles] == [["power_burn"], ["card_ignite"]]


def test_planning_service_build_execution_plan_merges_feature_bundle_items():
    service = PlanningService()
    plan = service.plan_from_dict(
        {
            "mod_name": "FeatureBundleMod",
            "summary": "",
            "items": [
                {
                    "id": "hero_core",
                    "type": "character",
                    "name": "HeroCore",
                    "coupling_kind": "feature_bundle",
                    "affected_targets": ["HeroCore"],
                },
                {
                    "id": "hero_relic",
                    "type": "relic",
                    "name": "HeroRelic",
                    "depends_on": ["hero_core"],
                    "coupling_kind": "feature_bundle",
                    "affected_targets": ["HeroCore"],
                },
            ],
        }
    )

    result = service.build_execution_plan(plan, strictness="balanced")

    assert [bundle.item_ids for bundle in result.execution_bundles] == [["hero_core", "hero_relic"]]
    assert result.execution_bundles[0].status == "clear"


def test_planning_service_build_execution_plan_marks_unclear_bundle_for_confirmation():
    service = PlanningService()
    plan = service.plan_from_dict(
        {
            "mod_name": "UnclearBundleMod",
            "summary": "",
            "items": [
                {
                    "id": "shared_helper",
                    "type": "custom_code",
                    "name": "SharedHelper",
                    "coupling_kind": "unclear",
                    "affected_targets": ["SharedLogic"],
                },
                {
                    "id": "shared_card",
                    "type": "card",
                    "name": "SharedCard",
                    "depends_on": ["shared_helper"],
                    "coupling_kind": "shared_logic",
                    "affected_targets": ["SharedLogic"],
                },
            ],
        }
    )

    result = service.build_execution_plan(plan, strictness="balanced")

    assert result.execution_bundles[0].status == "needs_confirmation"
    assert result.execution_bundles[0].bundle_id == "bundle:shared_helper::shared_card"
    assert "unclear_coupling" in result.execution_bundles[0].risk_codes
    assert result.execution_bundles[0].risk_details
    assert result.execution_bundles[0].recommended_actions


def test_planning_service_build_execution_plan_strict_mode_marks_large_bundle_for_confirmation():
    service = PlanningService()
    plan = service.plan_from_dict(
        {
            "mod_name": "StrictBundleMod",
            "summary": "",
            "items": [
                {
                    "id": "feature_a",
                    "type": "custom_code",
                    "name": "FeatureA",
                    "coupling_kind": "feature_bundle",
                    "affected_targets": ["SharedFeature"],
                },
                {
                    "id": "feature_b",
                    "type": "card",
                    "name": "FeatureB",
                    "depends_on": ["feature_a"],
                    "coupling_kind": "feature_bundle",
                    "affected_targets": ["SharedFeature"],
                },
                {
                    "id": "feature_c",
                    "type": "relic",
                    "name": "FeatureC",
                    "depends_on": ["feature_b"],
                    "coupling_kind": "feature_bundle",
                    "affected_targets": ["SharedFeature"],
                },
            ],
        }
    )

    efficient = service.build_execution_plan(plan, strictness="efficient")
    strict = service.build_execution_plan(plan, strictness="strict")

    assert efficient.execution_bundles[0].status == "clear"
    assert strict.execution_bundles[0].status == "needs_confirmation"
    assert "bundle_size_threshold" in strict.execution_bundles[0].risk_codes


def test_planning_service_build_execution_plan_applies_split_requested_decision():
    service = PlanningService()
    plan = service.plan_from_dict(
        {
            "mod_name": "SplitBundleMod",
            "summary": "",
            "items": [
                {
                    "id": "feature_a",
                    "type": "custom_code",
                    "name": "FeatureA",
                    "coupling_kind": "shared_logic",
                    "affected_targets": ["SharedFeature"],
                },
                {
                    "id": "feature_b",
                    "type": "card",
                    "name": "FeatureB",
                    "depends_on": ["feature_a"],
                    "coupling_kind": "shared_logic",
                    "affected_targets": ["SharedFeature"],
                },
            ],
        }
    )

    result = service.build_execution_plan(
        plan,
        strictness="balanced",
        bundle_decisions={"bundle:feature_a::feature_b": "split_requested"},
    )

    assert [bundle.item_ids for bundle in result.execution_bundles] == [["feature_a"], ["feature_b"]]
    assert all(bundle.status == "clear" for bundle in result.execution_bundles)
