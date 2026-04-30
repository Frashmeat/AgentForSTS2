from ..kernel.events import WorkflowEvent

WORKFLOW_EVENT_STAGES = (
    "prompt_preview",
    "image_ready",
    "approval_pending",
    "agent_stream",
    "build_started",
    "build_finished",
    "done",
    "error",
)

__all__ = ["WORKFLOW_EVENT_STAGES", "WorkflowEvent"]
