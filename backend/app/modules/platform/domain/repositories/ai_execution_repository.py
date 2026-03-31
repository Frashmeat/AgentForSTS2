from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.infra.persistence.models import AIExecutionRecord


class AIExecutionRepository(ABC):
    @abstractmethod
    def find_by_scoped_idempotency(
        self,
        user_id: int,
        job_item_id: int,
        request_idempotency_key: str,
    ) -> AIExecutionRecord | None: ...

    @abstractmethod
    def create(self, execution: AIExecutionRecord) -> AIExecutionRecord: ...

    @abstractmethod
    def find_by_id_for_update(self, execution_id: int) -> AIExecutionRecord | None: ...

    @abstractmethod
    def save(self, execution: AIExecutionRecord) -> None: ...

    @abstractmethod
    def find_latest_by_job_item(self, job_item_id: int) -> AIExecutionRecord | None: ...
