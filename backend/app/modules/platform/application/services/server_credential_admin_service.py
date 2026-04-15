from __future__ import annotations

from datetime import UTC, datetime
from urllib.parse import urlparse

from app.modules.platform.contracts import (
    AdminServerCredentialHealthCheckView,
    AdminServerCredentialListItem,
    CreateServerCredentialCommand,
    UpdateServerCredentialCommand,
)
from app.modules.platform.domain.repositories import ServerCredentialAdminRepository

from .server_credential_cipher import ServerCredentialCipher
from .server_credential_health_checker import ServerCredentialHealthChecker


class ServerCredentialAdminService:
    ALLOWED_PROVIDERS = {"openai", "anthropic"}
    ALLOWED_AUTH_TYPES = {"api_key", "ak_sk"}

    def __init__(
        self,
        server_credential_admin_repository: ServerCredentialAdminRepository,
        server_credential_cipher: ServerCredentialCipher,
        server_credential_health_checker: ServerCredentialHealthChecker,
    ) -> None:
        self.server_credential_admin_repository = server_credential_admin_repository
        self.server_credential_cipher = server_credential_cipher
        self.server_credential_health_checker = server_credential_health_checker

    def create_server_credential(self, command: CreateServerCredentialCommand) -> AdminServerCredentialListItem:
        provider = str(command.provider).strip().lower()
        auth_type = str(command.auth_type).strip().lower()
        credential = str(command.credential).strip()
        secret = str(command.secret).strip()
        base_url = str(command.base_url).strip()
        label = str(command.label).strip()

        if provider not in self.ALLOWED_PROVIDERS:
            raise ValueError(f"provider is not supported: {provider}")
        if auth_type not in self.ALLOWED_AUTH_TYPES:
            raise ValueError(f"auth_type is not supported: {auth_type}")
        if not credential:
            raise ValueError("credential is required")
        if auth_type == "ak_sk" and not secret:
            raise ValueError("secret is required when auth_type is ak_sk")
        if not label:
            raise ValueError("label is required")
        if base_url and not self._is_valid_http_url(base_url):
            raise ValueError("base_url must be a valid http or https url")

        secret_ciphertext = None
        if auth_type == "ak_sk":
            secret_ciphertext = self.server_credential_cipher.encrypt(secret)

        return self.server_credential_admin_repository.create_server_credential(
            execution_profile_id=command.execution_profile_id,
            provider=provider,
            auth_type=auth_type,
            credential_ciphertext=self.server_credential_cipher.encrypt(credential),
            secret_ciphertext=secret_ciphertext,
            base_url=base_url,
            label=label,
            priority=command.priority,
            enabled=command.enabled,
        )

    def update_server_credential(
        self,
        credential_id: int,
        command: UpdateServerCredentialCommand,
    ) -> AdminServerCredentialListItem:
        current = self.server_credential_admin_repository.get_server_credential(credential_id)
        if current is None:
            raise LookupError(f"server credential not found: {credential_id}")

        provider = str(command.provider).strip().lower()
        auth_type = str(command.auth_type).strip().lower()
        base_url = str(command.base_url).strip()
        label = str(command.label).strip()
        credential = str(command.credential).strip()
        secret = str(command.secret).strip()

        if provider not in self.ALLOWED_PROVIDERS:
            raise ValueError(f"provider is not supported: {provider}")
        if auth_type not in self.ALLOWED_AUTH_TYPES:
            raise ValueError(f"auth_type is not supported: {auth_type}")
        if not label:
            raise ValueError("label is required")
        if base_url and not self._is_valid_http_url(base_url):
            raise ValueError("base_url must be a valid http or https url")

        credential_ciphertext = current.credential_ciphertext
        if credential:
            credential_ciphertext = self.server_credential_cipher.encrypt(credential)

        if auth_type == "ak_sk":
            if secret:
                secret_ciphertext = self.server_credential_cipher.encrypt(secret)
            elif current.auth_type == "ak_sk" and current.secret_ciphertext:
                secret_ciphertext = current.secret_ciphertext
            else:
                raise ValueError("secret is required when auth_type is ak_sk")
        else:
            secret_ciphertext = None

        return self.server_credential_admin_repository.update_server_credential(
            credential_id=credential_id,
            execution_profile_id=command.execution_profile_id,
            provider=provider,
            auth_type=auth_type,
            credential_ciphertext=credential_ciphertext,
            secret_ciphertext=secret_ciphertext,
            base_url=base_url,
            label=label,
            priority=command.priority,
            enabled=command.enabled,
        )

    def set_server_credential_enabled(self, credential_id: int, enabled: bool) -> AdminServerCredentialListItem:
        return self.server_credential_admin_repository.set_server_credential_enabled(credential_id, enabled)

    def run_health_check(self, credential_id: int) -> AdminServerCredentialHealthCheckView:
        current = self.server_credential_admin_repository.get_server_credential(credential_id)
        if current is None:
            raise LookupError(f"server credential not found: {credential_id}")

        checked_at = datetime.now(UTC)
        if not current.enabled:
            return self.server_credential_admin_repository.record_health_check_result(
                credential_id=credential_id,
                trigger_source="manual",
                status="disabled",
                error_code="disabled",
                error_message="credential is disabled",
                latency_ms=None,
                checked_at=checked_at,
            )

        credential = self.server_credential_cipher.decrypt(current.credential_ciphertext)
        secret = self.server_credential_cipher.decrypt(current.secret_ciphertext) if current.secret_ciphertext else None
        result = self.server_credential_health_checker.check(
            provider=current.provider,
            auth_type=current.auth_type,
            credential=credential,
            secret=secret,
            base_url=current.base_url,
        )
        return self.server_credential_admin_repository.record_health_check_result(
            credential_id=credential_id,
            trigger_source="manual",
            status=result.status,
            error_code=result.error_code,
            error_message=result.error_message,
            latency_ms=result.latency_ms,
            checked_at=checked_at,
        )

    @staticmethod
    def _is_valid_http_url(value: str) -> bool:
        parsed = urlparse(value)
        return parsed.scheme in {"http", "https"} and bool(parsed.netloc)
