from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from datetime import datetime

from app.modules.platform.contracts import AdminServerCredentialHealthCheckView, AdminServerCredentialListItem


@dataclass(slots=True)
class ServerCredentialAdminRecord:
    id: int
    execution_profile_id: int
    provider: str
    auth_type: str
    credential_ciphertext: str
    secret_ciphertext: str | None
    base_url: str
    label: str
    priority: int
    enabled: bool
    health_status: str
    last_checked_at: datetime | None
    last_error_code: str
    last_error_message: str


class ServerCredentialAdminRepository(ABC):
    @abstractmethod
    def create_server_credential(
        self,
        *,
        execution_profile_id: int,
        provider: str,
        auth_type: str,
        credential_ciphertext: str,
        secret_ciphertext: str | None,
        base_url: str,
        label: str,
        priority: int,
        enabled: bool,
    ) -> AdminServerCredentialListItem:
        raise NotImplementedError

    @abstractmethod
    def get_server_credential(self, credential_id: int) -> ServerCredentialAdminRecord | None:
        raise NotImplementedError

    @abstractmethod
    def update_server_credential(
        self,
        *,
        credential_id: int,
        execution_profile_id: int,
        provider: str,
        auth_type: str,
        credential_ciphertext: str,
        secret_ciphertext: str | None,
        base_url: str,
        label: str,
        priority: int,
        enabled: bool,
    ) -> AdminServerCredentialListItem:
        raise NotImplementedError

    @abstractmethod
    def set_server_credential_enabled(self, credential_id: int, enabled: bool) -> AdminServerCredentialListItem:
        raise NotImplementedError

    @abstractmethod
    def record_health_check_result(
        self,
        *,
        credential_id: int,
        trigger_source: str,
        status: str,
        error_code: str,
        error_message: str,
        latency_ms: int | None,
        checked_at: datetime,
    ) -> AdminServerCredentialHealthCheckView:
        raise NotImplementedError
