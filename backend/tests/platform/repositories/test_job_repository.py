from __future__ import annotations

import sys
from datetime import UTC, datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.domain.models.enums import JobStatus
from app.modules.platform.infra.persistence.models import JobItemRecord
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy


def test_job_repository_creates_job_aggregate_with_expanded_items(db_session):
    repository = JobRepositorySqlAlchemy(db_session)
    command = CreateJobCommand.model_validate(
        {
            "job_type": "batch_generate",
            "workflow_version": "2026.03.31",
            "input_summary": "批量生成卡牌",
            "items": [
                {"item_type": "card", "input_summary": "卡牌 A", "input_payload": {"name": "A"}},
                {"item_type": "card", "input_summary": "卡牌 B", "input_payload": {"name": "B"}},
            ],
        }
    )

    job = repository.create_job_with_items(user_id=1001, command=command)
    db_session.commit()

    reloaded = repository.find_by_id_for_user(job.id, 1001)
    items = (
        db_session.query(JobItemRecord)
        .filter(JobItemRecord.job_id == job.id)
        .order_by(JobItemRecord.item_index.asc())
        .all()
    )

    assert reloaded is not None
    assert reloaded.status == JobStatus.DRAFT
    assert reloaded.total_item_count == 2
    assert reloaded.pending_item_count == 2
    assert [item.item_index for item in items] == [0, 1]
    assert items[0].input_payload["name"] == "A"


def test_job_repository_honors_user_visibility_and_cancel_request(db_session):
    repository = JobRepositorySqlAlchemy(db_session)
    command = CreateJobCommand.model_validate(
        {
            "job_type": "single_generate",
            "workflow_version": "2026.03.31",
            "items": [{"item_type": "card"}],
        }
    )
    job = repository.create_job_with_items(user_id=1001, command=command)
    requested_at = datetime.now(UTC)

    assert repository.find_by_id_for_user(job.id, 2002) is None
    assert repository.mark_cancel_requested(job.id, 2002, requested_at) is False
    assert repository.mark_cancel_requested(job.id, 1001, requested_at) is True

    db_session.commit()
    reloaded = repository.find_by_id_for_user(job.id, 1001)

    assert reloaded is not None
    assert reloaded.status == JobStatus.CANCELLING
    assert reloaded.cancel_requested_at is not None
