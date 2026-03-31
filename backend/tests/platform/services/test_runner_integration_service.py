from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.execution_orchestrator_service import ExecutionOrchestratorService
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.infra.persistence.repositories.ai_execution_repository_sqlalchemy import (
    AIExecutionRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_event_repository_sqlalchemy import (
    JobEventRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.modules.platform.runner.workflow_registry import PlatformWorkflowStep


class _FakeRegistry:
    def resolve(self, job_type: str, item_type: str):
        return [PlatformWorkflowStep(step_type="image.generate", step_id="step-1", input_payload={"seed": 1})]


class _FakeRunner:
    def __init__(self) -> None:
        self.calls: list[tuple[list[PlatformWorkflowStep], dict[str, object]]] = []

    async def run(self, *, steps, base_request):
        self.calls.append((steps, dict(base_request.input_payload)))
        return [StepExecutionResult(step_id="step-1", status="succeeded", output_payload={"artifact_type": "image"})]


def test_execution_orchestrator_service_can_delegate_to_runner_without_http_layer(db_session):
    runner = _FakeRunner()
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=None,
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        workflow_registry=_FakeRegistry(),
        workflow_runner=runner,
    )

    results = asyncio.run(
        service.run_registered_steps(
            job_type="single_generate",
            item_type="card",
            job_id=1,
            job_item_id=2,
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            input_payload={"prompt": "dark relic"},
        )
    )

    assert results[0].status == "succeeded"
    assert runner.calls[0][0][0].step_id == "step-1"
    assert runner.calls[0][1]["prompt"] == "dark relic"
