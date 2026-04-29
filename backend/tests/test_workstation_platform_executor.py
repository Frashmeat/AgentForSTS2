from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.platform.application.workstation_platform_executor import (
    WorkstationPlatformExecutor,
    build_workstation_workflow_registry,
)
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.contracts.workstation_execution import WorkstationExecutionDispatchRequest


class FakeRunner:
    def __init__(self) -> None:
        self.steps = []
        self.base_request = None

    async def run(self, *, steps, base_request):
        self.steps = steps
        self.base_request = base_request
        return [
            StepExecutionResult(
                step_id=steps[-1].step_id,
                status="succeeded",
                output_payload={"text": "ok"},
            )
        ]


def _dispatch_request(job_type: str = "single_generate", item_type: str = "relic") -> WorkstationExecutionDispatchRequest:
    return WorkstationExecutionDispatchRequest.model_validate(
        {
            "execution_id": 2203,
            "job_id": 2002,
            "job_item_id": 2103,
            "job_type": job_type,
            "item_type": item_type,
            "workflow_version": "2026.03.31",
            "step_protocol_version": "v1",
            "result_schema_version": "v1",
            "input_payload": {"description": "生成内容"},
            "execution_binding": {
                "provider": "openai",
                "model": "gpt-5.4",
                "credential_ref": "server-credential:1",
                "credential": "sk-live",
            },
        }
    )


def test_workstation_platform_executor_runs_text_workflow_and_returns_poll_result():
    runner = FakeRunner()
    executor = WorkstationPlatformExecutor(
        registry=build_workstation_workflow_registry(),
        runner=runner,
    )

    result = executor.execute(_dispatch_request("single_generate", "relic")).model_dump()

    assert [step.step_type for step in runner.steps] == ["single.asset.plan"]
    assert result == {
        "workstation_execution_id": "ws-exec-2203",
        "status": "succeeded",
        "step_id": "single.relic.plan",
        "output_payload": {"text": "ok"},
        "error_summary": "",
        "error_payload": {},
    }


def test_workstation_platform_executor_runs_code_workflow_steps():
    runner = FakeRunner()
    executor = WorkstationPlatformExecutor(
        registry=build_workstation_workflow_registry(),
        runner=runner,
    )

    result = executor.execute(_dispatch_request("single_generate", "custom_code")).model_dump()

    assert [step.step_type for step in runner.steps] == [
        "batch.custom_code.plan",
        "code.generate",
        "build.project",
    ]
    assert result["status"] == "succeeded"
