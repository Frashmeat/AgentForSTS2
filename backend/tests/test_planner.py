"""Tests for planner.py — plan parsing, topological sort, prompt construction."""
import sys
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))
sys.modules.setdefault(
    "llm.text_runner",
    types.SimpleNamespace(complete_text=lambda *args, **kwargs: None),
)

from app.modules.planning.api import parse_plan, topological_sort, build_planner_prompt, PlanItem


# ── _parse_plan ───────────────────────────────────────────────────────────────

MINIMAL_PLAN_JSON = """{
  "mod_name": "TestMod",
  "summary": "A test mod",
  "items": [
    {
      "id": "card_strike",
      "type": "card",
      "name": "StrikeCard",
      "description": "Deal 6 damage.",
      "implementation_notes": "Inherit CustomCardModel, override OnPlay to deal 6 damage.",
      "needs_image": true,
      "image_description": "A glowing sword",
      "depends_on": []
    }
  ]
}"""


def test_parse_plan_basic():
    plan = parse_plan(MINIMAL_PLAN_JSON)
    assert plan.mod_name == "TestMod"
    assert plan.summary == "A test mod"
    assert len(plan.items) == 1
    item = plan.items[0]
    assert item.id == "card_strike"
    assert item.type == "card"
    assert item.name == "StrikeCard"
    assert item.needs_image is True
    assert item.depends_on == []
    assert item.goal == ""
    assert item.coupling_kind == "unclear"
    assert item.clarification_questions == []


def test_parse_plan_preserves_extended_review_fields():
    json_str = """{
      "mod_name": "ExtendedReviewMod",
      "summary": "",
      "items": [
        {
          "id": "helper_logic",
          "type": "custom_code",
          "name": "HelperLogic",
          "description": "Shared helper",
          "implementation_notes": "Keep helpers isolated",
          "needs_image": false,
          "image_description": "",
          "depends_on": [],
          "goal": "Provide common battle helpers.",
          "detailed_description": "Create helper methods shared by multiple generated assets.",
          "scope_boundary": "Do not modify unrelated dungeon flow.",
          "dependency_reason": "",
          "acceptance_notes": "Helpers are reusable by future assets.",
          "affected_targets": ["HelperLogic", "BattleState"],
          "coupling_kind": "shared_logic",
          "clarification_status": "clear",
          "clarification_questions": []
        }
      ]
    }"""
    plan = parse_plan(json_str)
    item = plan.items[0]
    assert item.goal == "Provide common battle helpers."
    assert item.detailed_description == "Create helper methods shared by multiple generated assets."
    assert item.scope_boundary == "Do not modify unrelated dungeon flow."
    assert item.acceptance_notes == "Helpers are reusable by future assets."
    assert item.affected_targets == ["HelperLogic", "BattleState"]
    assert item.coupling_kind == "shared_logic"
    assert item.clarification_status == "clear"


def test_parse_plan_with_dependencies():
    json_str = """{
      "mod_name": "DepMod",
      "summary": "",
      "items": [
        {"id": "power_burn", "type": "power", "name": "BurnPower",
         "description": "", "implementation_notes": "", "needs_image": true,
         "image_description": "", "depends_on": []},
        {"id": "card_ignite", "type": "card", "name": "IgniteCard",
         "description": "", "implementation_notes": "", "needs_image": true,
         "image_description": "", "depends_on": ["power_burn"]}
      ]
    }"""
    plan = parse_plan(json_str)
    assert len(plan.items) == 2
    card_item = next(it for it in plan.items if it.id == "card_ignite")
    assert "power_burn" in card_item.depends_on


def test_parse_plan_custom_code_no_image():
    json_str = """{
      "mod_name": "Mech",
      "summary": "",
      "items": [
        {"id": "mech_passive", "type": "custom_code", "name": "SoulMechanic",
         "description": "Passive soul counter", "implementation_notes": "Harmony patch",
         "needs_image": false, "image_description": "", "depends_on": []}
      ]
    }"""
    plan = parse_plan(json_str)
    item = plan.items[0]
    assert item.needs_image is False
    assert item.type == "custom_code"


def test_parse_plan_defaults_gracefully():
    """Missing optional fields should not crash."""
    json_str = """{
      "mod_name": "Minimal",
      "items": [
        {"id": "x", "type": "relic", "name": "MyRelic",
         "description": "desc", "implementation_notes": "notes",
         "needs_image": true, "image_description": "", "depends_on": []}
      ]
    }"""
    plan = parse_plan(json_str)
    assert plan.summary == ""
    assert plan.mod_name == "Minimal"


# ── topological_sort ──────────────────────────────────────────────────────────

def _make_item(id_, depends_on=None):
    return PlanItem(
        id=id_, type="card", name=id_, description="", implementation_notes="",
        needs_image=False, image_description="", depends_on=depends_on or [],
    )


def test_topo_sort_independent_items_all_present():
    items = [_make_item("a"), _make_item("b"), _make_item("c")]
    result = topological_sort(items)
    assert {it.id for it in result} == {"a", "b", "c"}


def test_topo_sort_respects_dependency():
    # b depends on a → a must come before b
    items = [_make_item("b", depends_on=["a"]), _make_item("a")]
    result = topological_sort(items)
    ids = [it.id for it in result]
    assert ids.index("a") < ids.index("b")


def test_topo_sort_chain():
    # c → b → a
    items = [
        _make_item("c", depends_on=["b"]),
        _make_item("b", depends_on=["a"]),
        _make_item("a"),
    ]
    result = topological_sort(items)
    ids = [it.id for it in result]
    assert ids.index("a") < ids.index("b") < ids.index("c")


def test_topo_sort_missing_dep_ignored():
    """Dependency on a non-existent item should not crash."""
    items = [_make_item("b", depends_on=["ghost_id"]), _make_item("a")]
    result = topological_sort(items)
    assert len(result) == 2


# ── _build_planner_prompt ─────────────────────────────────────────────────────

def test_planner_prompt_contains_guidance():
    prompt = build_planner_prompt("make a relic that triggers on attack")
    assert "### Planner Guidance" in prompt
    assert "OnPlay" in prompt
    assert "PowerModel" in prompt
    assert "ShouldReceiveCombatHooks" in prompt


def test_planner_prompt_contains_user_requirements():
    req = "unique_requirement_xyz_12345"
    prompt = build_planner_prompt(req)
    assert req in prompt


def test_planner_prompt_contains_json_schema():
    prompt = build_planner_prompt("test")
    assert "### Code Facts Check" in prompt
    assert "### Further Lookup" in prompt
    assert "implementation_notes" in prompt
    assert "depends_on" in prompt
    assert "needs_image" in prompt
