from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.job_application_service import JobApplicationService
from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand
from app.modules.platform.domain.models.enums import JobItemStatus, JobStatus
from app.modules.platform.infra.persistence.repositories.job_event_repository_sqlalchemy import (
    JobEventRepositorySqlAlchemy,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy


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
