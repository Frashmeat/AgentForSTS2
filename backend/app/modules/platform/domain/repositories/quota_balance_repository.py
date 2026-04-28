from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.infra.persistence.models import QuotaBalanceRecord


class QuotaBalanceRepository(ABC):
    @abstractmethod
    def create_balance(self, balance: QuotaBalanceRecord) -> QuotaBalanceRecord: ...

    @abstractmethod
    def find_by_user_id(self, user_id: int) -> QuotaBalanceRecord | None: ...

    @abstractmethod
    def find_by_user_id_for_update(self, user_id: int) -> QuotaBalanceRecord | None: ...

    @abstractmethod
    def save_balance(self, balance: QuotaBalanceRecord) -> None: ...

