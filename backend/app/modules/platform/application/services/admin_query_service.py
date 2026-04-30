from __future__ import annotations

from app.modules.platform.domain.repositories import AdminQueryRepositories

from .platform_runtime_audit_service import PlatformRuntimeAuditService


class AdminQueryService:
    def __init__(
        self,
        admin_query_repositories: AdminQueryRepositories,
        runtime_audit_service: PlatformRuntimeAuditService | None = None,
    ) -> None:
        self.admin_query_repositories = admin_query_repositories
        self.runtime_audit_service = runtime_audit_service

    def list_executions(self, job_id: int | None = None):
        return self.admin_query_repositories.list_executions(job_id=job_id)

    def get_execution_detail(self, execution_id: int):
        return self.admin_query_repositories.get_execution_detail(execution_id)

    def list_refunds(self, user_id: int | None = None):
        return self.admin_query_repositories.list_refunds(user_id=user_id)

    def list_users(
        self,
        query: str = "",
        email_verified: bool | None = None,
        is_admin: bool | None = None,
        quota_status: str = "",
        anomaly: str = "",
        limit: int = 50,
        offset: int = 0,
    ):
        return self.admin_query_repositories.list_users(
            query=query,
            email_verified=email_verified,
            is_admin=is_admin,
            quota_status=quota_status,
            anomaly=anomaly,
            limit=limit,
            offset=offset,
        )

    def get_user_detail(self, user_id: int):
        return self.admin_query_repositories.get_user_detail(user_id)

    def list_user_quota_ledgers(self, user_id: int, after_id: int | None = None, limit: int = 50):
        return self.admin_query_repositories.list_user_quota_ledgers(user_id=user_id, after_id=after_id, limit=limit)

    def list_audit_events(
        self,
        job_id: int | None = None,
        after_id: int | None = None,
        limit: int = 50,
        event_type_prefix: str | None = None,
    ):
        db_events = self.admin_query_repositories.list_audit_events(
            job_id=job_id,
            after_id=after_id,
            limit=limit,
            event_type_prefix=event_type_prefix,
        )
        if job_id is not None or self.runtime_audit_service is None:
            return db_events
        runtime_events = self.runtime_audit_service.list_events(
            after_id=after_id,
            limit=limit,
            event_type_prefix=event_type_prefix,
        )
        merged = [*db_events, *runtime_events]
        merged = sorted(merged, key=lambda item: item.event_id)
        return merged[: max(int(limit or 0), 1)]

    def list_server_credentials(self, execution_profile_id: int | None = None):
        return self.admin_query_repositories.list_server_credentials(execution_profile_id=execution_profile_id)

    def list_execution_profiles(self):
        return self.admin_query_repositories.list_execution_profiles()
