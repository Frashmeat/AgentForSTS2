from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.platform.domain.repositories import ExecutionChargeRepository
from app.modules.platform.infra.persistence.models import ExecutionChargeRecord


class ExecutionChargeRepositorySqlAlchemy(ExecutionChargeRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def create_reserved(self, charge: ExecutionChargeRecord) -> ExecutionChargeRecord:
        self.session.add(charge)
        self.session.flush()
        return charge

    def find_by_execution_id_for_update(self, execution_id: int) -> ExecutionChargeRecord | None:
        return (
            self.session.query(ExecutionChargeRecord)
            .filter(ExecutionChargeRecord.ai_execution_id == execution_id)
            .one_or_none()
        )

    def save(self, charge: ExecutionChargeRecord) -> None:
        self.session.add(charge)
        self.session.flush()
