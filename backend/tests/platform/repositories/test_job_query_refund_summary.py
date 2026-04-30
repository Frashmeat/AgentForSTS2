from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence.models import AIExecutionRecord, ChargeStatus, ExecutionChargeRecord
from app.modules.platform.infra.persistence.repositories.job_query_repository_sqlalchemy import (
    JobQueryRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy


def test_job_query_repository_exposes_refund_summary_fields(db_session):
    job_repository = JobRepositorySqlAlchemy(db_session)
    job = job_repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {"job_type": "single_generate", "workflow_version": "2026.04.03", "items": [{"item_type": "card"}]}
        ),
    )
    db_session.flush()

    first_execution = AIExecutionRecord(
        job_id=job.id,
        job_item_id=job.items[0].id,
        user_id=1001,
        status="succeeded",
        provider="openai",
        model="gpt-5.4",
        request_idempotency_key="idem-refund-1",
        workflow_version="2026.04.03",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-1",
    )
    second_execution = AIExecutionRecord(
        job_id=job.id,
        job_item_id=job.items[0].id,
        user_id=1001,
        status="failed_system",
        provider="openai",
        model="gpt-5.4",
        request_idempotency_key="idem-refund-2",
        workflow_version="2026.04.03",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-2",
    )
    db_session.add_all([first_execution, second_execution])
    db_session.flush()
    db_session.add_all(
        [
            ExecutionChargeRecord(
                ai_execution_id=first_execution.id,
                user_id=1001,
                charge_status=ChargeStatus.CAPTURED,
                charge_amount=1,
            ),
            ExecutionChargeRecord(
                ai_execution_id=second_execution.id,
                user_id=1001,
                charge_status=ChargeStatus.REFUNDED,
                charge_amount=1,
                refund_reason="system_error",
            ),
        ]
    )
    db_session.commit()

    repository = JobQueryRepositorySqlAlchemy(db_session)

    jobs = repository.list_jobs(1001)
    detail = repository.get_job_detail(1001, job.id)

    assert jobs[0].original_deducted == 2
    assert jobs[0].refunded_amount == 1
    assert jobs[0].net_consumed == 1
    assert jobs[0].refund_reason_summary == "system_error"
    assert detail is not None
    assert detail.original_deducted == 2
    assert detail.refunded_amount == 1
    assert detail.net_consumed == 1
    assert detail.refund_reason_summary == "system_error"
