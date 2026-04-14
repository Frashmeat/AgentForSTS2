from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.contracts import (
    AdminExecutionDetailView,
    AdminExecutionListItem,
    AdminExecutionProfileListItem,
    AdminServerCredentialListItem,
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
    def list_audit_events(self, job_id: int | None = None) -> list[JobEventView]: ...

    @abstractmethod
    def list_server_credentials(self, execution_profile_id: int | None = None) -> list[AdminServerCredentialListItem]: ...

    @abstractmethod
    def list_execution_profiles(self) -> list[AdminExecutionProfileListItem]: ...
