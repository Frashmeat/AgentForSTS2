from __future__ import annotations

from sqlalchemy.orm import Session

from app.modules.platform.domain.repositories import QuotaBalanceRepository
from app.modules.platform.infra.persistence.models import QuotaBalanceRecord


class QuotaBalanceRepositorySqlAlchemy(QuotaBalanceRepository):
    def __init__(self, session: Session) -> None:
        self.session = session

    def create_balance(self, balance: QuotaBalanceRecord) -> QuotaBalanceRecord:
        self.session.add(balance)
        self.session.flush()
        return balance

    def find_by_user_id(self, user_id: int) -> QuotaBalanceRecord | None:
        return self.session.query(QuotaBalanceRecord).filter(QuotaBalanceRecord.user_id == user_id).one_or_none()

    def find_by_user_id_for_update(self, user_id: int) -> QuotaBalanceRecord | None:
        return self.find_by_user_id(user_id)

    def save_balance(self, balance: QuotaBalanceRecord) -> None:
        self.session.add(balance)
        self.session.flush()
