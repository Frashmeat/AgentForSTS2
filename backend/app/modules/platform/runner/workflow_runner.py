from __future__ import annotations

from collections.abc import Callable

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest, StepExecutionResult

from .step_dispatcher import StepDispatcher
from .workflow_registry import PlatformWorkflowStep


EventPublisher = Callable[[str, str], None]


class WorkflowRunner:
    def __init__(self, dispatcher: StepDispatcher, event_publisher: EventPublisher | None = None) -> None:
        self.dispatcher = dispatcher
        self.event_publisher = event_publisher

    async def run(
        self,
        *,
        steps: list[PlatformWorkflowStep],
        base_request: StepExecutionRequest,
        event_publisher: EventPublisher | None = None,
    ) -> list[StepExecutionResult]:
        results: list[StepExecutionResult] = []
        payload: dict[str, object] = dict(base_request.input_payload)
        publisher = event_publisher or self.event_publisher

        for step in steps:
            merged_payload = dict(payload)
            merged_payload.update(step.input_payload)
            request = StepExecutionRequest(
                workflow_version=base_request.workflow_version,
                step_protocol_version=base_request.step_protocol_version,
                step_type=step.step_type,
                step_id=step.step_id,
                job_id=base_request.job_id,
                job_item_id=base_request.job_item_id,
                result_schema_version=base_request.result_schema_version,
                input_payload=merged_payload,
                execution_binding=base_request.execution_binding,
            )
            if publisher is not None:
                publisher("step.started", step.step_id)
            result = await self.dispatcher.dispatch(request)
            results.append(result)
            if publisher is not None:
                publisher("step.finished", step.step_id)
            if result.status != "succeeded":
                break
            payload.update(result.output_payload)

        return results
