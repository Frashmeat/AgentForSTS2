from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ExecutionChargeRecord,
    JobStatus,
    LedgerType,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBucketRecord,
    QuotaBucketType,
    UsageLedgerRecord,
)
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


def _seed_job_item(db_session):
    job_repository = JobRepositorySqlAlchemy(db_session)
    command = CreateJobCommand.model_validate(
        {
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "items": [{"item_type": "card", "input_payload": {"name": "Relic"}}],
        }
    )
    job = job_repository.create_job_with_items(user_id=1001, command=command)
    db_session.flush()
    item = job.items[0]
    job.status = JobStatus.RUNNING
    db_session.flush()
    return job, item


def test_execution_repository_supports_scoped_idempotency_and_latest_lookup(db_session):
    _, item = _seed_job_item(db_session)
    repository = AIExecutionRepositorySqlAlchemy(db_session)

    first = repository.create(
        AIExecutionRecord(
            job_id=item.job_id,
            job_item_id=item.id,
            user_id=1001,
            status="created",
            provider="openai",
            model="gpt-5.4",
            request_idempotency_key="idem-1",
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            step_type="image.generate",
            step_id="step-1",
        )
    )
    second = repository.create(
        AIExecutionRecord(
            job_id=item.job_id,
            job_item_id=item.id,
            user_id=1001,
            status="dispatching",
            provider="openai",
            model="gpt-5.4",
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            step_type="image.generate",
            step_id="step-2",
        )
    )
    db_session.commit()

    assert repository.find_by_scoped_idempotency(1001, item.id, "idem-1").id == first.id
    assert repository.find_latest_by_job_item(item.id).id == second.id


def test_charge_quota_ledger_and_event_repositories_persist_auditable_chain(db_session):
    job, item = _seed_job_item(db_session)
    execution_repository = AIExecutionRepositorySqlAlchemy(db_session)
    charge_repository = ExecutionChargeRepositorySqlAlchemy(db_session)
    quota_repository = QuotaAccountRepositorySqlAlchemy(db_session)
    ledger_repository = UsageLedgerRepositorySqlAlchemy(db_session)
    event_repository = JobEventRepositorySqlAlchemy(db_session)
    now = datetime.now(UTC)

    execution = execution_repository.create(
        AIExecutionRecord(
            job_id=job.id,
            job_item_id=item.id,
            user_id=1001,
            status="running",
            provider="openai",
            model="gpt-5.4",
            request_idempotency_key="idem-billing",
            workflow_version="2026.03.31",
            step_protocol_version="v1",
            result_schema_version="v1",
            step_type="code.generate",
            step_id="step-billing",
        )
    )
    account = quota_repository.create_account(
        QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE)
    )
    daily_bucket = quota_repository.create_bucket(
        QuotaBucketRecord(
            quota_account_id=account.id,
            bucket_type=QuotaBucketType.DAILY,
            period_start=now - timedelta(hours=1),
            period_end=now + timedelta(hours=23),
            quota_limit=10,
            used_amount=1,
            refunded_amount=0,
        )
    )
    charge = charge_repository.create_reserved(
        ExecutionChargeRecord(
            ai_execution_id=execution.id,
            user_id=1001,
            charge_status="reserved",
            charge_amount=1,
        )
    )
    reserve = ledger_repository.append_reserve(
        UsageLedgerRecord(
            user_id=1001,
            quota_account_id=account.id,
            quota_bucket_id=daily_bucket.id,
            ai_execution_id=execution.id,
            ledger_type=LedgerType.RESERVE,
            amount=1,
            balance_after=8,
            reason_code="execution_start",
        )
    )
    ledger_repository.append_capture(
        UsageLedgerRecord(
            user_id=1001,
            quota_account_id=account.id,
            quota_bucket_id=daily_bucket.id,
            ai_execution_id=execution.id,
            ledger_type=LedgerType.CAPTURE,
            amount=1,
            balance_after=8,
            reason_code="execution_finish",
        )
    )
    event_repository.append(
        job_id=job.id,
        user_id=1001,
        event_type="job.started",
        payload={"status": "running"},
        job_item_id=item.id,
        ai_execution_id=execution.id,
    )
    event_repository.append(
        job_id=job.id,
        user_id=1001,
        event_type="job.finished",
        payload={"status": "succeeded"},
        job_item_id=item.id,
    )
    db_session.commit()

    ledgers = ledger_repository.list_by_execution_id(execution.id)
    events = event_repository.list_by_job(job.id, after_id=None, limit=10)

    assert charge_repository.find_by_execution_id_for_update(execution.id).id == charge.id
    assert quota_repository.find_account_by_user_id(1001).id == account.id
    assert quota_repository.find_active_bucket_for_update(account.id, now).id == daily_bucket.id
    assert reserve.amount == 1
    assert [entry.ledger_type.value for entry in ledgers] == ["reserve", "capture"]
    assert [event.event_type for event in events] == ["job.started", "job.finished"]
