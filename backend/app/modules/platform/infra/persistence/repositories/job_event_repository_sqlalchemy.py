from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.platform.domain.repositories import JobEventRepository
from app.modules.platform.infra.persistence.models import JobEventRecord


class JobEventRepositorySqlAlchemy(JobEventRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def append(
        self,
        *,
        job_id: int,
        user_id: int,
        event_type: str,
        payload: dict[str, object],
        job_item_id: int | None = None,
        ai_execution_id: int | None = None,
    ) -> JobEventRecord:
        event = JobEventRecord(
            job_id=job_id,
            job_item_id=job_item_id,
            ai_execution_id=ai_execution_id,
            user_id=user_id,
            event_type=event_type,
            event_payload=dict(payload),
        )
        self.session.add(event)
        self.session.flush()
        return event

    def list_by_job(self, job_id: int, after_id: int | None, limit: int) -> list[JobEventRecord]:
        query = self.session.query(JobEventRecord).filter(JobEventRecord.job_id == job_id)
        if after_id is not None:
            query = query.filter(JobEventRecord.id > after_id)
        return query.order_by(JobEventRecord.id.asc()).limit(limit).all()
