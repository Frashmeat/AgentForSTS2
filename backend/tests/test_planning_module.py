import sys
import types
import inspect
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
sys.modules.setdefault(
    "llm.text_runner",
    types.SimpleNamespace(complete_text=lambda *args, **kwargs: None),
)

from app.modules.planning.application.services import PlanningService
from app.modules.planning.domain.models import PlanItem
from app.shared.prompting import PromptLoader

_PROMPT_LOADER_SUPPORTED = "prompt_loader" in inspect.signature(PlanningService).parameters


class FakeKnowledgeSource:
    def load_context(self, context_type: str, asset_type: str | None = None) -> str:
        if context_type == "planner":
            return "OnPlay PowerModel ShouldReceiveCombatHooks"
        return f"docs:{asset_type or ''}"


def _build_service(knowledge_source=None, prompt_loader=None) -> PlanningService:
    kwargs = {}
    if knowledge_source is not None:
        kwargs["knowledge_source"] = knowledge_source
    if _PROMPT_LOADER_SUPPORTED and prompt_loader is not None:
        kwargs["prompt_loader"] = prompt_loader
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

    assert "OnPlay" in prompt
    assert "make a burning card" in prompt


def test_planning_prompt_template_exists_for_real_loader():
    loader = PromptLoader(root=Path("backend/app/modules/planning/resources/prompts"))

    template = loader.load("planner_prompt.txt")

    assert "{{ api_hints }}" in template
    assert "{{ requirements }}" in template
    assert '"implementation_notes"' in template
    assert '"depends_on"' in template


def test_planning_service_build_planner_prompt_uses_prompt_loader():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-planner-prompt"

    loader = FakePromptLoader()
    service = _build_service(knowledge_source=FakeKnowledgeSource(), prompt_loader=loader)

    prompt = service.build_planner_prompt("make a burning card")

    assert prompt == "rendered-planner-prompt"
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "planner_prompt.txt"
    assert variables["api_hints"] == "OnPlay PowerModel ShouldReceiveCombatHooks"
    assert variables["requirements"] == "make a burning card"
    assert '"implementation_notes"' in fallback_template
    assert '"depends_on"' in fallback_template


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
