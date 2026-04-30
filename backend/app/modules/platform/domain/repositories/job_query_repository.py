from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.contracts import ArtifactSummary, JobDetailView, JobEventView, JobItemListItem, JobListItem


class JobQueryRepository(ABC):
    @abstractmethod
    def list_jobs(self, user_id: int) -> list[JobListItem]: ...

    @abstractmethod
    def get_job_detail(self, user_id: int, job_id: int) -> JobDetailView | None: ...

    @abstractmethod
    def list_job_items(self, user_id: int, job_id: int) -> list[JobItemListItem]: ...

    @abstractmethod
    def list_visible_events(
        self, user_id: int, job_id: int, after_id: int | None, limit: int
    ) -> list[JobEventView]: ...

    @abstractmethod
    def list_artifact_summaries(self, user_id: int, job_id: int) -> list[ArtifactSummary]: ...
