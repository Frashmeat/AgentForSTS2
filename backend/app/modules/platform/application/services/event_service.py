from __future__ import annotations

from app.modules.platform.domain.repositories import JobEventRepository


class EventService:
    def __init__(self, job_event_repository: JobEventRepository) -> None:
        self.job_event_repository = job_event_repository
