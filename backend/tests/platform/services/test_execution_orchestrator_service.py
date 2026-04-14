from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.execution_orchestrator_service import ExecutionOrchestratorService
from app.modules.platform.application.services.quota_billing_service import QuotaBillingService
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.infra.persistence.models import QuotaAccountRecord, QuotaAccountStatus, QuotaBucketRecord, QuotaBucketType
from app.modules.platform.infra.persistence.repositories.ai_execution_repository_sqlalchemy import (
    AIExecutionRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.execution_charge_repository_sqlalchemy import (
    ExecutionChargeRepositorySqlAlchemy,
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


def _seed_ready_job(db_session):
    job_repository = JobRepositorySqlAlchemy(db_session)
    job = job_repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {"job_type": "single_generate", "workflow_version": "2026.03.31", "items": [{"item_type": "card"}]}
        ),
    )
    job.status = JobStatus.QUEUED
    job.items[0].status = JobItemStatus.READY
    db_session.flush()
    return job


def test_execution_orchestrator_service_creates_execution_when_quota_is_available(db_session):
    job = _seed_ready_job(db_session)
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    now = datetime.now(UTC)
    account = quota_repository.create_account(QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE))
    quota_repository.create_bucket(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=now - timedelta(hours=1),
            period_end=now + timedelta(hours=23),
            quota_limit=10,
            used_amount=0,
            refunded_amount=0,
        )
    )
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=quota_repository,
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    execution = service.start_execution(
        user_id=1001,
        job_id=job.id,
        job_item_id=job.items[0].id,
        provider="openai",
        model="gpt-5.4",
        credential_ref="cred-a",
        retry_attempt=1,
        switched_credential=True,
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-1",
        request_idempotency_key="idem-run",
        now=now,
    )
    db_session.commit()

    assert execution is not None
    assert execution.job_id == job.id
    assert execution.credential_ref == "cred-a"
    assert execution.retry_attempt == 1
    assert execution.switched_credential is True


def test_execution_orchestrator_service_marks_quota_exhausted_when_reserve_fails(db_session):
    job = _seed_ready_job(db_session)
    service = ExecutionOrchestratorService(
        job_repository=JobRepositorySqlAlchemy(db_session),
        ai_execution_repository=AIExecutionRepositorySqlAlchemy(db_session),
        quota_billing_service=QuotaBillingService(
            execution_charge_repository=ExecutionChargeRepositorySqlAlchemy(db_session),
            quota_account_repository=QuotaAccountRepositorySqlAlchemy(db_session),
            usage_ledger_repository=UsageLedgerRepositorySqlAlchemy(db_session),
        ),
        job_event_repository=JobEventRepositorySqlAlchemy(db_session),
    )

    execution = service.start_execution(
        user_id=1001,
        job_id=job.id,
        job_item_id=job.items[0].id,
        provider="openai",
        model="gpt-5.4",
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-2",
        request_idempotency_key="idem-no-quota",
        now=datetime.now(UTC),
    )
    db_session.commit()

    assert execution is None
    assert job.status == JobStatus.QUOTA_EXHAUSTED
    assert job.items[0].status == JobItemStatus.QUOTA_SKIPPED
