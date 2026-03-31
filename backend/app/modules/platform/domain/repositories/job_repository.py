from __future__ import annotations

from abc import ABC, abstractmethod
from datetime import datetime

from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence.models import JobRecord


class JobRepository(ABC):
    @abstractmethod
    def create_job_with_items(self, user_id: int, command: CreateJobCommand) -> JobRecord: ...

    @abstractmethod
    def find_by_id_for_user(self, job_id: int, user_id: int) -> JobRecord | None: ...

    @abstractmethod
    def find_by_id_for_update(self, job_id: int, user_id: int) -> JobRecord | None: ...

    @abstractmethod
    def save(self, job: JobRecord) -> None: ...

    @abstractmethod
    def mark_cancel_requested(self, job_id: int, user_id: int, requested_at: datetime) -> bool: ...
