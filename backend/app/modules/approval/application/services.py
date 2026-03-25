from __future__ import annotations

from app.modules.approval.application.ports import ActionExecutor, ApprovalStore
from app.modules.approval.domain.models import ActionRequest


class ApprovalService:
    def __init__(self, store: ApprovalStore, executor: ActionExecutor | None = None):
        self.store = store
        self.executor = executor

    def _build_request(
        self,
        raw_action: dict,
        *,
        source_backend: str,
        source_workflow: str,
    ) -> ActionRequest:
        from approval.policies import infer_risk_level, should_require_approval

        if "kind" not in raw_action:
            raise ValueError("Action plan item is missing required field: kind")
        if "title" not in raw_action:
            raise ValueError("Action plan item is missing required field: title")

        kind = raw_action["kind"]
        risk_level = infer_risk_level(kind)
        return ActionRequest(
            kind=kind,
            title=raw_action["title"],
            reason=raw_action.get("reason", ""),
            payload=raw_action.get("payload", {}),
            risk_level=risk_level,
            requires_approval=should_require_approval(risk_level),
            source_backend=source_backend,
            source_workflow=source_workflow,
        )

    def create_requests_from_plan(
        self,
        plan: dict,
        *,
        source_backend: str,
        source_workflow: str,
    ) -> list[ActionRequest]:
        pending_requests = [
            self._build_request(
                raw_action,
                source_backend=source_backend,
                source_workflow=source_workflow,
            )
            for raw_action in plan.get("actions", [])
        ]

        created: list[ActionRequest] = []
        for action in pending_requests:
            created.append(self.store.create_request(action))
        return created

    async def execute_request(self, action_id: str) -> ActionRequest:
        if self.executor is None:
            raise RuntimeError("Approval executor is not configured")

        action = self.store.get_request(action_id)
        if action.requires_approval and action.status != "approved":
            raise ValueError("Approval request must be approved before execution")

        try:
            self.store.mark_running(action_id)
            result = await self.executor.execute_action(action)
            return self.store.mark_succeeded(
                action_id,
                {"output": result.output, **result.metadata},
            )
        except Exception as exc:
            return self.store.mark_failed(action_id, str(exc))
