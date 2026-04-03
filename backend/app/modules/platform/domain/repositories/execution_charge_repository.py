from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.infra.persistence.models import ExecutionChargeRecord


class ExecutionChargeRepository(ABC):
    @abstractmethod
    def create_reserved(self, charge: ExecutionChargeRecord) -> ExecutionChargeRecord: ...

    @abstractmethod
    def find_by_execution_id_for_update(self, execution_id: int) -> ExecutionChargeRecord | None: ...

    @abstractmethod
    def save(self, charge: ExecutionChargeRecord) -> None: ...
