from __future__ import annotations

import asyncio
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.execution_orchestrator_service import ExecutionOrchestratorService
from app.modules.platform.application.services.execution_routing_service import ExecutionRoutingService
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.contracts.runner_contracts import StepExecutionBinding, StepExecutionResult
from app.modules.platform.domain.models.enums import AIExecutionStatus, JobItemStatus, JobStatus
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ExecutionProfileRecord,
    ServerCredentialRecord,
)
from app.modules.platform.infra.persistence.repositories.ai_execution_repository_sqlalchemy import (
    AIExecutionRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.execution_routing_repository_sqlalchemy import (
    ExecutionRoutingRepositorySqlAlchemy,
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
        self.calls: list[tuple[list[PlatformWorkflowStep], dict[str, object], dict[str, object]]] = []

    async def run(self, *, steps, base_request):
        self.calls.append((steps, dict(base_request.input_payload), base_request.execution_binding.model_dump()))
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
            execution_binding=StepExecutionBinding(
                agent_backend="codex",
                provider="openai",
                model="gpt-5.4",
                credential_ref="server-credential:3",
                credential="sk-live",
            ),
        )
    )

    assert results[0].status == "succeeded"
    assert runner.calls[0][0][0].step_id == "step-1"
    assert runner.calls[0][1]["prompt"] == "dark relic"
    assert runner.calls[0][2]["credential_ref"] == "server-credential:3"


def test_execution_orchestrator_service_can_resolve_server_execution_binding_for_runner(db_session):
    cipher = ServerCredentialCipher("test-secret-for-runner")
    profile = ExecutionProfileRecord(
        code="codex-gpt-5-4",
        display_name="Codex CLI / gpt-5.4",
        agent_backend="codex",
        model="gpt-5.4",
        description="默认推荐",
        enabled=True,
        recommended=True,
        sort_order=10,
    )
    db_session.add(profile)
    db_session.flush()
    credential = ServerCredentialRecord(
        execution_profile_id=profile.id,
        provider="openai",
        auth_type="api_key",
        credential_ciphertext=cipher.encrypt("sk-live-openai"),
        secret_ciphertext=None,
        base_url="https://api.openai.com/v1",
        label="primary",
        priority=1,
        enabled=True,
        health_status="healthy",
        last_checked_at=None,
        last_error_code="",
        last_error_message="",
    )
    db_session.add(credential)
    job_repository = JobRepositorySqlAlchemy(db_session)
    job = job_repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "single_generate",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [{"item_type": "card"}],
            }
        ),
    )
    job.status = JobStatus.RUNNING
    job.items[0].status = JobItemStatus.RUNNING
    db_session.flush()
    AIExecutionRepositorySqlAlchemy(db_session).create(
        AIExecutionRecord(
            job_id=job.id,
            job_item_id=job.items[0].id,
            user_id=1001,
            status=AIExecutionStatus.DISPATCHING,
            provider="openai",
            model="gpt-5.4",
            credential_ref=f"server-credential:{credential.id}",
            retry_attempt=0,
            switched_credential=False,
            request_idempotency_key="idem-runner",
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            step_type="workflow.dispatch",
            step_id="workflow.dispatch.item-1",
        )
    )
    db_session.flush()

    runner = _FakeRunner()
    service = ExecutionOrchestratorService(
        job_repository=job_repository,
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=None,
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        workflow_registry=_FakeRegistry(),
        workflow_runner=runner,
    )

    results = asyncio.run(
        service.run_registered_steps(
            user_id=1001,
            job_type="single_generate",
            item_type="card",
            job_id=job.id,
            job_item_id=job.items[0].id,
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            input_payload={"prompt": "dark relic"},
        )
    )

    assert results[0].status == "succeeded"
    assert runner.calls[0][2]["agent_backend"] == "codex"
    assert runner.calls[0][2]["provider"] == "openai"
    assert runner.calls[0][2]["model"] == "gpt-5.4"
    assert runner.calls[0][2]["credential_ref"] == f"server-credential:{credential.id}"
    assert runner.calls[0][2]["credential"] == "sk-live-openai"
    assert runner.calls[0][2]["base_url"] == "https://api.openai.com/v1"
