from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import (
    StepExecutionBinding,
    StepExecutionRequest,
    StepExecutionResult,
)
from app.modules.platform.runner.step_dispatcher import StepDispatcher
from app.modules.platform.runner.workflow_registry import PlatformWorkflowStep
from app.modules.platform.runner.workflow_runner import WorkflowRunner


def test_workflow_runner_executes_steps_in_order_and_publishes_events():
    events: list[tuple[str, str]] = []
    seen_inputs: list[dict[str, object]] = []
    seen_bindings: list[dict[str, object]] = []

    async def execute(request: StepExecutionRequest) -> StepExecutionResult:
        seen_inputs.append(dict(request.input_payload))
        seen_bindings.append(request.execution_binding.model_dump())
        return StepExecutionResult(
            step_id=request.step_id,
            status="succeeded",
            output_payload={"last_step": request.step_id},
        )

    runner = WorkflowRunner(
        dispatcher=StepDispatcher(execute_handler=execute),
        event_publisher=lambda event_type, step_id: events.append((event_type, step_id)),
    )

    results = asyncio.run(
        runner.run(
            steps=[
                PlatformWorkflowStep(step_type="image.generate", step_id="step-1", input_payload={"prompt": "relic"}),
                PlatformWorkflowStep(step_type="code.generate", step_id="step-2"),
            ],
            base_request=StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="placeholder",
                step_id="placeholder",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                execution_binding=StepExecutionBinding(
                    agent_backend="codex",
                    provider="openai",
                    model="gpt-5.4",
                    credential_ref="server-credential:7",
                    credential="sk-live",
                ),
            ),
        )
    )

    assert [result.step_id for result in results] == ["step-1", "step-2"]
    assert seen_inputs[0]["prompt"] == "relic"
    assert seen_inputs[1]["last_step"] == "step-1"
    assert seen_bindings[0]["credential_ref"] == "server-credential:7"
    assert seen_bindings[1]["provider"] == "openai"
    assert events == [
        ("step.started", "step-1"),
        ("step.finished", "step-1"),
        ("step.started", "step-2"),
        ("step.finished", "step-2"),
    ]


def test_workflow_runner_stops_after_failed_step():
    async def execute(request: StepExecutionRequest) -> StepExecutionResult:
        if request.step_id == "step-1":
            return StepExecutionResult(step_id=request.step_id, status="failed_system", error_summary="boom")
        return StepExecutionResult(step_id=request.step_id, status="succeeded")

    runner = WorkflowRunner(dispatcher=StepDispatcher(execute_handler=execute))

    results = asyncio.run(
        runner.run(
            steps=[
                PlatformWorkflowStep(step_type="image.generate", step_id="step-1"),
                PlatformWorkflowStep(step_type="code.generate", step_id="step-2"),
            ],
            base_request=StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="placeholder",
                step_id="placeholder",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
            ),
        )
    )

    assert [result.step_id for result in results] == ["step-1"]
    assert results[0].status == "failed_system"
