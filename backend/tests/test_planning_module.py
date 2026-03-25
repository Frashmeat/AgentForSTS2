import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.planning.application.services import PlanningService
from app.modules.planning.domain.models import PlanItem


class FakeKnowledgeSource:
    def load_context(self, context_type: str, asset_type: str | None = None) -> str:
        if context_type == "planner":
            return "OnPlay PowerModel ShouldReceiveCombatHooks"
        return f"docs:{asset_type or ''}"


def test_planning_service_topologically_sorts_plan_items():
    service = PlanningService(knowledge_source=FakeKnowledgeSource())
    items = [
        PlanItem(id="card_ignite", type="card", name="Ignite", depends_on=["power_burn"]),
        PlanItem(id="power_burn", type="power", name="BurnPower"),
    ]

    result = service.topologically_sort_plan_items(items)

    assert [item.id for item in result] == ["power_burn", "card_ignite"]


def test_planning_service_uses_knowledge_source_for_prompt():
    service = PlanningService(knowledge_source=FakeKnowledgeSource())

    prompt = service.build_planner_prompt("make a burning card")

    assert "OnPlay" in prompt
    assert "make a burning card" in prompt
