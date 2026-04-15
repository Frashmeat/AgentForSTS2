from __future__ import annotations

from datetime import UTC, datetime

from app.modules.platform.contracts import AdminServerCredentialHealthCheckView, AdminServerCredentialListItem
from app.modules.platform.domain.repositories import ServerCredentialAdminRecord, ServerCredentialAdminRepository
from app.modules.platform.infra.persistence.models import CredentialHealthCheckRecord, ExecutionProfileRecord, ServerCredentialRecord


class ServerCredentialAdminRepositorySqlAlchemy(ServerCredentialAdminRepository):
    def __init__(self, session) -> None:
        self.session = session

    def _ensure_execution_profile_exists(self, execution_profile_id: int) -> None:
        profile_exists = (
            self.session.query(ExecutionProfileRecord.id)
            .filter(ExecutionProfileRecord.id == execution_profile_id)
            .first()
            is not None
        )
        if not profile_exists:
            raise LookupError(f"execution profile not found: {execution_profile_id}")

    @staticmethod
    def _to_iso(value: object | None) -> str | None:
        if value is None:
            return None
        if isinstance(value, datetime):
            if value.tzinfo is None:
                value = value.replace(tzinfo=UTC)
            else:
                value = value.astimezone(UTC)
        return value.isoformat()

    @staticmethod
    def _to_list_item(row: ServerCredentialRecord) -> AdminServerCredentialListItem:
        return AdminServerCredentialListItem(
            id=row.id,
            execution_profile_id=row.execution_profile_id,
            provider=row.provider,
            auth_type=row.auth_type,
            label=row.label,
            base_url=row.base_url,
            priority=row.priority,
            enabled=row.enabled,
            health_status=row.health_status,
            last_checked_at=ServerCredentialAdminRepositorySqlAlchemy._to_iso(row.last_checked_at),
            last_error_code=row.last_error_code,
            last_error_message=row.last_error_message,
        )

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
        self._ensure_execution_profile_exists(execution_profile_id)

        row = ServerCredentialRecord(
            execution_profile_id=execution_profile_id,
            provider=provider,
            auth_type=auth_type,
            credential_ciphertext=credential_ciphertext,
            secret_ciphertext=secret_ciphertext,
            base_url=base_url,
            label=label,
            priority=priority,
            enabled=enabled,
            health_status="healthy" if enabled else "disabled",
            last_checked_at=None,
            last_error_code="",
            last_error_message="",
        )
        self.session.add(row)
        self.session.flush()
        return self._to_list_item(row)

    def get_server_credential(self, credential_id: int) -> ServerCredentialAdminRecord | None:
        row = (
            self.session.query(ServerCredentialRecord)
            .filter(ServerCredentialRecord.id == credential_id)
            .one_or_none()
        )
        if row is None:
            return None
        return ServerCredentialAdminRecord(
            id=row.id,
            execution_profile_id=row.execution_profile_id,
            provider=row.provider,
            auth_type=row.auth_type,
            credential_ciphertext=row.credential_ciphertext,
            secret_ciphertext=row.secret_ciphertext,
            base_url=row.base_url,
            label=row.label,
            priority=row.priority,
            enabled=row.enabled,
            health_status=row.health_status,
            last_checked_at=row.last_checked_at,
            last_error_code=row.last_error_code,
            last_error_message=row.last_error_message,
        )

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
        self._ensure_execution_profile_exists(execution_profile_id)
        row = (
            self.session.query(ServerCredentialRecord)
            .filter(ServerCredentialRecord.id == credential_id)
            .one_or_none()
        )
        if row is None:
            raise LookupError(f"server credential not found: {credential_id}")

        row.execution_profile_id = execution_profile_id
        row.provider = provider
        row.auth_type = auth_type
        row.credential_ciphertext = credential_ciphertext
        row.secret_ciphertext = secret_ciphertext
        row.base_url = base_url
        row.label = label
        row.priority = priority
        row.enabled = enabled
        if not enabled:
            row.health_status = "disabled"
        elif row.health_status == "disabled":
            row.health_status = "degraded"
        self.session.flush()
        return self._to_list_item(row)

    def set_server_credential_enabled(self, credential_id: int, enabled: bool) -> AdminServerCredentialListItem:
        row = (
            self.session.query(ServerCredentialRecord)
            .filter(ServerCredentialRecord.id == credential_id)
            .one_or_none()
        )
        if row is None:
            raise LookupError(f"server credential not found: {credential_id}")
        row.enabled = enabled
        if enabled:
            if row.health_status == "disabled":
                row.health_status = "degraded"
        else:
            row.health_status = "disabled"
        self.session.flush()
        return self._to_list_item(row)

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
        row = (
            self.session.query(ServerCredentialRecord)
            .filter(ServerCredentialRecord.id == credential_id)
            .one_or_none()
        )
        if row is None:
            raise LookupError(f"server credential not found: {credential_id}")

        row.health_status = status
        row.last_checked_at = checked_at
        row.last_error_code = error_code
        row.last_error_message = error_message
        self.session.add(
            CredentialHealthCheckRecord(
                server_credential_id=credential_id,
                trigger_source=trigger_source,
                status=status,
                error_code=error_code,
                error_message=error_message,
                latency_ms=latency_ms,
                checked_at=checked_at,
            )
        )
        self.session.flush()
        return AdminServerCredentialHealthCheckView(
            credential_id=credential_id,
            health_status=status,
            error_code=error_code,
            error_message=error_message,
            checked_at=self._to_iso(checked_at),
        )
