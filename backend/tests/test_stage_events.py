import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.contracts.workflow_events import WorkflowEvent
from llm.stage_events import build_stage_event


def test_build_stage_event_returns_none_for_blank_message():
    assert build_stage_event("text", "ai_running", "   ") is None


def test_build_stage_event_trims_message_and_adds_scope_stage():
    assert build_stage_event("agent", "agent_running", "  正在生成代码...  ") == {
        "scope": "agent",
        "stage": "agent_running",
        "message": "正在生成代码...",
    }


def test_build_stage_event_includes_item_id_when_provided():
    assert build_stage_event("image", "image_generating", "正在生成图像...", item_id="card_1") == {
        "scope": "image",
        "stage": "image_generating",
        "message": "正在生成图像...",
        "item_id": "card_1",
    }


def test_standard_workflow_event_keeps_stage_and_payload():
    event = WorkflowEvent(stage="done", payload={"success": True})
    assert event.stage == "done"
    assert event.payload["success"] is True
