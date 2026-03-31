from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.admin_query_service import AdminQueryService
from app.modules.platform.application.services.job_query_service import JobQueryService
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ExecutionChargeRecord,
    JobEventRecord,
    QuotaAccountRecord,
    QuotaBucketRecord,
)
from app.modules.platform.infra.persistence.repositories.admin_query_repositories_sqlalchemy import (
    AdminQueryRepositoriesSqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_query_repository_sqlalchemy import (
    JobQueryRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.modules.platform.infra.persistence.repositories.quota_query_repository_sqlalchemy import (
    QuotaQueryRepositorySqlAlchemy,
)


def test_query_services_wrap_user_and_admin_views(db_session):
    job_repository = JobRepositorySqlAlchemy(db_session)
    job = job_repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {"job_type": "single_generate", "workflow_version": "2026.03.31", "items": [{"item_type": "card"}]}
        ),
    )
    db_session.flush()
    execution = AIExecutionRecord(
        job_id=job.id,
        job_item_id=job.items[0].id,
        user_id=1001,
        status="succeeded",
        provider="openai",
        model="gpt-5.4",
        request_idempotency_key="idem-query",
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-query",
    )
    db_session.add(execution)
    db_session.flush()
    db_session.add(
        ExecutionChargeRecord(
            ai_execution_id=execution.id,
            user_id=1001,
            charge_status="refunded",
            charge_amount=1,
            refund_reason="system_error",
        )
    )
    db_session.add(
        JobEventRecord(
            job_id=job.id,
            job_item_id=job.items[0].id,
            user_id=1001,
            event_type="job.done",
            event_payload={"status": "succeeded"},
        )
    )
    quota_account = QuotaAccountRecord(user_id=1001, status="active")
    db_session.add(quota_account)
    db_session.flush()
    db_session.add(
        QuotaBucketRecord(
            quota_account_id=quota_account.id,
            bucket_type="daily",
            period_start=datetime.now(UTC) - timedelta(hours=1),
            period_end=datetime.now(UTC) + timedelta(hours=23),
            quota_limit=10,
            used_amount=2,
            refunded_amount=1,
        )
    )
    db_session.commit()

    job_query_service = JobQueryService(
        job_query_repository=JobQueryRepositorySqlAlchemy(db_session),
        quota_query_repository=QuotaQueryRepositorySqlAlchemy(db_session),
    )
    admin_query_service = AdminQueryService(
        admin_query_repositories=AdminQueryRepositoriesSqlAlchemy(db_session),
    )

    assert job_query_service.list_jobs(1001)[0].id == job.id
    assert job_query_service.get_job_detail(1001, job.id).id == job.id
    assert job_query_service.get_quota_view(1001, datetime.now(UTC)).daily_used == 2
    assert admin_query_service.list_executions(job.id)[0].id == execution.id
    assert admin_query_service.list_refunds(1001)[0].refund_reason == "system_error"
