from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.platform.domain.repositories import AIExecutionRepository
from app.modules.platform.infra.persistence.models import AIExecutionRecord


class AIExecutionRepositorySqlAlchemy(AIExecutionRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def find_by_scoped_idempotency(
        self,
        user_id: int,
        job_item_id: int,
        request_idempotency_key: str,
    ) -> AIExecutionRecord | None:
        return (
            self.session.query(AIExecutionRecord)
            .filter(
                AIExecutionRecord.user_id == user_id,
                AIExecutionRecord.job_item_id == job_item_id,
                AIExecutionRecord.request_idempotency_key == request_idempotency_key,
            )
            .one_or_none()
        )

    def create(self, execution: AIExecutionRecord) -> AIExecutionRecord:
        self.session.add(execution)
        self.session.flush()
        return execution

    def find_by_id_for_update(self, execution_id: int) -> AIExecutionRecord | None:
        return self.session.query(AIExecutionRecord).filter(AIExecutionRecord.id == execution_id).one_or_none()

    def save(self, execution: AIExecutionRecord) -> None:
        self.session.add(execution)
        self.session.flush()

    def find_latest_by_job_item(self, job_item_id: int) -> AIExecutionRecord | None:
        return (
            self.session.query(AIExecutionRecord)
            .filter(AIExecutionRecord.job_item_id == job_item_id)
            .order_by(AIExecutionRecord.created_at.desc(), AIExecutionRecord.id.desc())
            .first()
        )
