from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.contracts import (
    AdminExecutionDetailView,
    AdminExecutionListItem,
    AdminExecutionProfileListItem,
    AdminQuotaLedgerListView,
    AdminServerCredentialListItem,
    AdminUserDetailView,
    AdminUserListView,
    JobEventView,
    RefundRecordView,
)


class AdminQueryRepositories(ABC):
    @abstractmethod
    def list_executions(self, job_id: int | None = None) -> list[AdminExecutionListItem]: ...

    @abstractmethod
    def get_execution_detail(self, execution_id: int) -> AdminExecutionDetailView | None: ...

    @abstractmethod
    def list_refunds(self, user_id: int | None = None) -> list[RefundRecordView]: ...

    @abstractmethod
    def list_users(
        self,
        query: str = "",
        email_verified: bool | None = None,
        is_admin: bool | None = None,
        quota_status: str = "",
        anomaly: str = "",
        limit: int = 50,
        offset: int = 0,
    ) -> AdminUserListView: ...

    @abstractmethod
    def get_user_detail(self, user_id: int) -> AdminUserDetailView | None: ...

    @abstractmethod
    def list_user_quota_ledgers(
        self, user_id: int, after_id: int | None = None, limit: int = 50
    ) -> AdminQuotaLedgerListView: ...

    @abstractmethod
    def list_audit_events(
        self,
        job_id: int | None = None,
        after_id: int | None = None,
        limit: int = 50,
        event_type_prefix: str | None = None,
    ) -> list[JobEventView]: ...

    @abstractmethod
    def list_server_credentials(
        self, execution_profile_id: int | None = None
    ) -> list[AdminServerCredentialListItem]: ...

    @abstractmethod
    def list_execution_profiles(self) -> list[AdminExecutionProfileListItem]: ...
