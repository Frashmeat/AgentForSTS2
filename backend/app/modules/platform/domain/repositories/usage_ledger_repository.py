from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.infra.persistence.models import UsageLedgerRecord


class UsageLedgerRepository(ABC):
    @abstractmethod
    def append_reserve(self, entry: UsageLedgerRecord) -> UsageLedgerRecord: ...

    @abstractmethod
    def append_capture(self, entry: UsageLedgerRecord) -> UsageLedgerRecord: ...

    @abstractmethod
    def append_refund(self, entry: UsageLedgerRecord) -> UsageLedgerRecord: ...

    @abstractmethod
    def append_admin_adjustment(self, entry: UsageLedgerRecord) -> UsageLedgerRecord: ...

    @abstractmethod
    def list_by_execution_id(self, execution_id: int) -> list[UsageLedgerRecord]: ...

    @abstractmethod
    def list_by_user_id(self, user_id: int, limit: int = 50, after_id: int | None = None) -> list[UsageLedgerRecord]: ...
