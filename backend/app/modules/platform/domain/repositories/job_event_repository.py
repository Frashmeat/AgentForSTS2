from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.infra.persistence.models import JobEventRecord


class JobEventRepository(ABC):
    @abstractmethod
    def append(
        self,
        *,
        job_id: int,
        user_id: int,
        event_type: str,
        payload: dict[str, object],
        job_item_id: int | None = None,
        ai_execution_id: int | None = None,
    ) -> JobEventRecord: ...

    @abstractmethod
    def list_by_job(self, job_id: int, after_id: int | None, limit: int) -> list[JobEventRecord]: ...
