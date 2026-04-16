from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.execution_orchestrator_service import ExecutionOrchestratorService
from app.modules.platform.application.services.execution_routing_service import ExecutionRoutingService
from app.modules.platform.application.services.job_application_service import JobApplicationService
from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand
from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.infra.persistence.models import (
    ExecutionProfileRecord,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBucketRecord,
    QuotaBucketType,
    ServerCredentialRecord,
)
from app.modules.platform.infra.persistence.repositories.ai_execution_repository_sqlalchemy import (
    AIExecutionRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.execution_charge_repository_sqlalchemy import (
    ExecutionChargeRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.execution_routing_repository_sqlalchemy import (
    ExecutionRoutingRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_event_repository_sqlalchemy import (
    JobEventRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.modules.platform.infra.persistence.repositories.quota_account_repository_sqlalchemy import (
    QuotaAccountRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.usage_ledger_repository_sqlalchemy import (
    UsageLedgerRepositorySqlAlchemy,
)
from app.modules.platform.runner.workflow_registry import PlatformWorkflowStep


def test_job_application_service_handles_create_start_and_cancel(db_session):
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )
    create_command = CreateJobCommand.model_validate(
        {
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "items": [
                {"item_type": "card", "input_payload": {"name": "One"}},
                {"item_type": "card", "input_payload": {"name": "Two"}},
            ],
        }
    )

    job = service.create_job(user_id=1001, command=create_command)
    assert job.status == JobStatus.DRAFT

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))
    assert started is not None
    assert started.status == JobStatus.QUEUED
    assert [item.status for item in started.items] == [JobItemStatus.READY, JobItemStatus.READY]

    cancelled = service.cancel_job(user_id=1001, command=CancelJobCommand(job_id=job.id, reason="user_stop"))
    db_session.commit()

    assert cancelled is True


def test_job_application_service_rejects_start_for_other_user(db_session):
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {"job_type": "single_generate", "workflow_version": "2026.03.31", "items": [{"item_type": "card"}]}
        ),
    )

    assert service.start_job(user_id=2002, command=StartJobCommand(job_id=job.id)) is None


class _LogRegistry:
    def resolve(self, job_type: str, item_type: str):
        if (job_type, item_type) != ("log_analysis", "log_analysis"):
            raise KeyError(f"workflow not found for {job_type}/{item_type}")
        return [PlatformWorkflowStep(step_type="log.analyze", step_id="log-analyze")]


class _SucceededRunner:
    async def run(self, *, steps, base_request):
        return [
            StepExecutionResult(
                step_id=steps[0].step_id,
                status="succeeded",
                output_payload={"text": "分析完成"},
            )
        ]


def test_job_application_service_can_complete_supported_log_analysis_job(db_session):
    cipher = ServerCredentialCipher("job-service-test-secret")
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
    db_session.add(
        ServerCredentialRecord(
            execution_profile_id=profile.id,
            provider="openai",
            auth_type="api_key",
            credential_ciphertext=cipher.encrypt("sk-live-openai"),
            secret_ciphertext=None,
            base_url="https://api.openai.com/v1",
            label="main",
            priority=1,
            enabled=True,
            health_status="healthy",
            last_checked_at=None,
            last_error_code="",
            last_error_message="",
        )
    )
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
    quota_repository.create_bucket(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=datetime.now(UTC) - timedelta(hours=1),
            period_end=datetime.now(UTC) + timedelta(hours=23),
            quota_limit=10,
            used_amount=0,
            refunded_amount=0,
        )
    )

    orchestrator = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_routing_service=ExecutionRoutingService(
            execution_routing_repository=ExecutionRoutingRepositorySqlAlchemy(db_session)
        ),
        server_credential_cipher=cipher,
        workflow_registry=_LogRegistry(),
        workflow_runner=_SucceededRunner(),
    )
    service = JobApplicationService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
        execution_orchestrator_service=orchestrator,
    )
    job = service.create_job(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {
                "job_type": "log_analysis",
                "workflow_version": "2026.03.31",
                "selected_execution_profile_id": profile.id,
                "selected_agent_backend": "codex",
                "selected_model": "gpt-5.4",
                "items": [
                    {
                        "item_type": "log_analysis",
                        "input_summary": "分析最近一次日志",
                        "input_payload": {"context": "黑屏了"},
                    }
                ],
            }
        ),
    )

    started = service.start_job(user_id=1001, command=StartJobCommand(job_id=job.id))

    assert started is not None
    assert started.status == JobStatus.SUCCEEDED
    assert started.items[0].status == JobItemStatus.SUCCEEDED
    assert started.items[0].result_summary == "分析完成"
