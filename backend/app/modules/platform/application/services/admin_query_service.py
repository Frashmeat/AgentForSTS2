from __future__ import annotations

from app.modules.platform.domain.repositories import AdminQueryRepositories


class AdminQueryService:
    def __init__(self, admin_query_repositories: AdminQueryRepositories) -> None:
        self.admin_query_repositories = admin_query_repositories

    def list_executions(self, job_id: int | None = None):
        return self.admin_query_repositories.list_executions(job_id=job_id)

    def get_execution_detail(self, execution_id: int):
        return self.admin_query_repositories.get_execution_detail(execution_id)

    def list_refunds(self, user_id: int | None = None):
        return self.admin_query_repositories.list_refunds(user_id=user_id)

    def list_audit_events(self, job_id: int | None = None):
        return self.admin_query_repositories.list_audit_events(job_id=job_id)
