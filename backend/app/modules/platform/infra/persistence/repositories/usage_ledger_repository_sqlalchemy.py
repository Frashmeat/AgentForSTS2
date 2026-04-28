from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.platform.domain.repositories import UsageLedgerRepository
from app.modules.platform.infra.persistence.models import UsageLedgerRecord


class UsageLedgerRepositorySqlAlchemy(UsageLedgerRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def _append(self, entry: UsageLedgerRecord) -> UsageLedgerRecord:
        self.session.add(entry)
        self.session.flush()
        return entry

    def append_reserve(self, entry: UsageLedgerRecord) -> UsageLedgerRecord:
        return self._append(entry)

    def append_capture(self, entry: UsageLedgerRecord) -> UsageLedgerRecord:
        return self._append(entry)

    def append_refund(self, entry: UsageLedgerRecord) -> UsageLedgerRecord:
        return self._append(entry)

    def append_admin_adjustment(self, entry: UsageLedgerRecord) -> UsageLedgerRecord:
        return self._append(entry)

    def list_by_execution_id(self, execution_id: int) -> list[UsageLedgerRecord]:
        return (
            self.session.query(UsageLedgerRecord)
            .filter(UsageLedgerRecord.ai_execution_id == execution_id)
            .order_by(UsageLedgerRecord.id.asc())
            .all()
        )

    def list_by_user_id(self, user_id: int, limit: int = 50, after_id: int | None = None) -> list[UsageLedgerRecord]:
        query = self.session.query(UsageLedgerRecord).filter(UsageLedgerRecord.user_id == user_id)
        if after_id is not None:
            query = query.filter(UsageLedgerRecord.id > after_id)
        return query.order_by(UsageLedgerRecord.id.desc()).limit(max(int(limit or 0), 1)).all()
