import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.contracts.approval import ApprovalDecision
from app.shared.contracts.workflow_events import WORKFLOW_EVENT_STAGES, WorkflowEvent
from app.shared.kernel.errors import DomainError
from app.shared.kernel.ids import ArtifactId, WorkflowRunId
from app.shared.kernel.result import Result


def test_workflow_event_contract_has_stage_and_payload():
    event = WorkflowEvent(stage="image_ready", payload={"image_id": "img-1"})

    assert event.stage == "image_ready"
    assert event.payload["image_id"] == "img-1"


def test_shared_kernel_exposes_minimal_common_types():
    result = Result.ok({"artifact": "mod"})
    error = DomainError(code="workflow.failed", message="failed")

    assert result.ok is True
    assert result.value == {"artifact": "mod"}
    assert error.code == "workflow.failed"
    assert ArtifactId("img-1") == "img-1"
    assert WorkflowRunId("run-1") == "run-1"


def test_workflow_event_stages_are_frozen():
    assert WORKFLOW_EVENT_STAGES == (
        "prompt_preview",
        "image_ready",
        "approval_pending",
        "agent_stream",
        "build_started",
        "build_finished",
        "done",
        "error",
    )


def test_approval_decision_defaults_to_manual_review():
    decision = ApprovalDecision(action="build_mod")

    assert decision.action == "build_mod"
    assert decision.approved is False
    assert decision.reason == ""
