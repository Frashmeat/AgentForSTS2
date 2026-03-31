from __future__ import annotations

from datetime import datetime

from app.modules.platform.domain.repositories import JobQueryRepository, QuotaQueryRepository


class JobQueryService:
    def __init__(self, job_query_repository: JobQueryRepository, quota_query_repository: QuotaQueryRepository) -> None:
        self.job_query_repository = job_query_repository
        self.quota_query_repository = quota_query_repository

    def list_jobs(self, user_id: int):
        return self.job_query_repository.list_jobs(user_id)

    def get_job_detail(self, user_id: int, job_id: int):
        return self.job_query_repository.get_job_detail(user_id, job_id)

    def list_job_items(self, user_id: int, job_id: int):
        return self.job_query_repository.list_job_items(user_id, job_id)

    def list_events(self, user_id: int, job_id: int, after_id: int | None = None, limit: int = 50):
        return self.job_query_repository.list_visible_events(user_id, job_id, after_id, limit)

    def get_quota_view(self, user_id: int, now: datetime):
        return self.quota_query_repository.get_current_quota_view(user_id, now)
