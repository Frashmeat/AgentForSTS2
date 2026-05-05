from __future__ import annotations

import sys
from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.server_credential_admin_service import ServerCredentialAdminService
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.application.services.server_credential_health_checker import ServerCredentialHealthCheckResult
from app.modules.platform.contracts import (
    AdminServerCredentialCliHealthCheckView,
    AdminServerCredentialHealthCheckView,
    AdminServerCredentialListItem,
    CreateServerCredentialCommand,
    UpdateServerCredentialCommand,
)
from app.modules.platform.domain.repositories import ServerCredentialAdminRecord


class FakeServerCredentialAdminRepository:
    def __init__(self) -> None:
        self.payload = None
        self.entries = {
            1: ServerCredentialAdminRecord(
                id=1,
                execution_profile_id=1,
                execution_profile_runner_type="codex_cli",
                execution_profile_model="gpt-5.4",
                api_protocol="openai_compatible",
                auth_type="api_key",
                credential_ciphertext="",
                secret_ciphertext=None,
                api_base_url="https://api.openai.com/v1",
                label="openai-main-a",
                priority=10,
                enabled=True,
                health_status="healthy",
                last_checked_at=None,
                last_error_code="",
                last_error_message="",
            )
        }
        self.last_health_payload = None

    def create_server_credential(self, **payload) -> AdminServerCredentialListItem:
        self.payload = payload
        self.entries[1] = ServerCredentialAdminRecord(
            id=1,
            execution_profile_id=payload["execution_profile_id"],
            execution_profile_runner_type="codex_cli",
            execution_profile_model="gpt-5.4",
            api_protocol=payload["api_protocol"],
            auth_type=payload["auth_type"],
            credential_ciphertext=payload["credential_ciphertext"],
            secret_ciphertext=payload["secret_ciphertext"],
            api_base_url=payload["api_base_url"],
            label=payload["label"],
            priority=payload["priority"],
            enabled=payload["enabled"],
            health_status="healthy" if payload["enabled"] else "disabled",
            last_checked_at=None,
            last_error_code="",
            last_error_message="",
        )
        return AdminServerCredentialListItem(
            id=101,
            execution_profile_id=payload["execution_profile_id"],
            api_protocol=payload["api_protocol"],
            auth_type=payload["auth_type"],
            label=payload["label"],
            api_base_url=payload["api_base_url"],
            priority=payload["priority"],
            enabled=payload["enabled"],
            health_status="healthy" if payload["enabled"] else "disabled",
            last_checked_at=None,
            last_error_code="",
            last_error_message="",
        )

    def get_server_credential(self, credential_id: int) -> ServerCredentialAdminRecord | None:
        return self.entries.get(credential_id)

    def update_server_credential(self, **payload) -> AdminServerCredentialListItem:
        current = self.entries[payload["credential_id"]]
        updated = ServerCredentialAdminRecord(
            id=current.id,
            execution_profile_id=payload["execution_profile_id"],
            execution_profile_runner_type=current.execution_profile_runner_type,
            execution_profile_model=current.execution_profile_model,
            api_protocol=payload["api_protocol"],
            auth_type=payload["auth_type"],
            credential_ciphertext=payload["credential_ciphertext"],
            secret_ciphertext=payload["secret_ciphertext"],
            api_base_url=payload["api_base_url"],
            label=payload["label"],
            priority=payload["priority"],
            enabled=payload["enabled"],
            health_status="disabled" if not payload["enabled"] else current.health_status,
            last_checked_at=current.last_checked_at,
            last_error_code=current.last_error_code,
            last_error_message=current.last_error_message,
        )
        self.entries[payload["credential_id"]] = updated
        return AdminServerCredentialListItem(
            id=updated.id,
            execution_profile_id=updated.execution_profile_id,
            api_protocol=updated.api_protocol,
            auth_type=updated.auth_type,
            label=updated.label,
            api_base_url=updated.api_base_url,
            priority=updated.priority,
            enabled=updated.enabled,
            health_status=updated.health_status,
            last_checked_at=None,
            last_error_code=updated.last_error_code,
            last_error_message=updated.last_error_message,
        )

    def set_server_credential_enabled(self, credential_id: int, enabled: bool) -> AdminServerCredentialListItem:
        current = self.entries[credential_id]
        health_status = "disabled" if not enabled else "degraded"
        self.entries[credential_id] = ServerCredentialAdminRecord(
            id=current.id,
            execution_profile_id=current.execution_profile_id,
            execution_profile_runner_type=current.execution_profile_runner_type,
            execution_profile_model=current.execution_profile_model,
            api_protocol=current.api_protocol,
            auth_type=current.auth_type,
            credential_ciphertext=current.credential_ciphertext,
            secret_ciphertext=current.secret_ciphertext,
            api_base_url=current.api_base_url,
            label=current.label,
            priority=current.priority,
            enabled=enabled,
            health_status=health_status,
            last_checked_at=current.last_checked_at,
            last_error_code=current.last_error_code,
            last_error_message=current.last_error_message,
        )
        return AdminServerCredentialListItem(
            id=current.id,
            execution_profile_id=current.execution_profile_id,
            api_protocol=current.api_protocol,
            auth_type=current.auth_type,
            label=current.label,
            api_base_url=current.api_base_url,
            priority=current.priority,
            enabled=enabled,
            health_status=health_status,
            last_checked_at=None,
            last_error_code=current.last_error_code,
            last_error_message=current.last_error_message,
        )

    def delete_server_credential(self, credential_id: int) -> None:
        if credential_id not in self.entries:
            raise LookupError(f"server credential not found: {credential_id}")
        del self.entries[credential_id]

    def record_health_check_result(self, **payload) -> AdminServerCredentialHealthCheckView:
        current = self.entries[payload["credential_id"]]
        self.last_health_payload = payload
        self.entries[payload["credential_id"]] = ServerCredentialAdminRecord(
            id=current.id,
            execution_profile_id=current.execution_profile_id,
            execution_profile_runner_type=current.execution_profile_runner_type,
            execution_profile_model=current.execution_profile_model,
            api_protocol=current.api_protocol,
            auth_type=current.auth_type,
            credential_ciphertext=current.credential_ciphertext,
            secret_ciphertext=current.secret_ciphertext,
            api_base_url=current.api_base_url,
            label=current.label,
            priority=current.priority,
            enabled=current.enabled,
            health_status=payload["status"],
            last_checked_at=payload["checked_at"],
            last_error_code=payload["error_code"],
            last_error_message=payload["error_message"],
        )
        return AdminServerCredentialHealthCheckView(
            credential_id=payload["credential_id"],
            health_status=payload["status"],
            error_code=payload["error_code"],
            error_message=payload["error_message"],
            checked_at=payload["checked_at"].isoformat(),
        )


class FakeServerCredentialHealthChecker:
    def __init__(self, result: ServerCredentialHealthCheckResult | None = None) -> None:
        self.result = result or ServerCredentialHealthCheckResult(status="healthy", latency_ms=12)
        self.calls = []

    def check(self, **payload) -> ServerCredentialHealthCheckResult:
        self.calls.append(payload)
        return self.result


class FakeCliHealthCheckRunner:
    def __init__(self, result: str | Exception = "OK") -> None:
        self.result = result
        self.calls: list[tuple[str, dict, object | None]] = []

    async def __call__(self, prompt: str, llm_cfg: dict, cwd: object | None = None) -> str:
        self.calls.append((prompt, llm_cfg, cwd))
        if isinstance(self.result, Exception):
            raise self.result
        return self.result


def test_server_credential_admin_service_encrypts_plaintext_before_persisting():
    repository = FakeServerCredentialAdminRepository()
    cipher = ServerCredentialCipher("test-server-credential-secret")
    checker = FakeServerCredentialHealthChecker()
    service = ServerCredentialAdminService(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=checker,
    )

    item = service.create_server_credential(
        CreateServerCredentialCommand.model_validate(
            {
                "execution_profile_id": 1,
                "api_protocol": "openai_compatible",
                "auth_type": "api_key",
                "credential": "sk-live-credential",
                "api_base_url": "https://api.openai.com/v1",
                "label": "openai-main-a",
                "priority": 10,
                "enabled": True,
            }
        )
    )

    assert item.api_protocol == "openai_compatible"
    assert repository.payload is not None
    assert repository.payload["credential_ciphertext"] != "sk-live-credential"
    assert cipher.decrypt(repository.payload["credential_ciphertext"]) == "sk-live-credential"
    assert repository.payload["secret_ciphertext"] is None


def test_server_credential_admin_service_requires_secret_for_ak_sk_mode():
    repository = FakeServerCredentialAdminRepository()
    cipher = ServerCredentialCipher("test-server-credential-secret")
    checker = FakeServerCredentialHealthChecker()
    service = ServerCredentialAdminService(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=checker,
    )

    try:
        service.create_server_credential(
            CreateServerCredentialCommand.model_validate(
                {
                    "execution_profile_id": 1,
                    "api_protocol": "openai_compatible",
                    "auth_type": "ak_sk",
                    "credential": "ak-live",
                    "label": "openai-aksk",
                }
            )
        )
    except ValueError as error:
        assert str(error) == "secret is required when auth_type is ak_sk"
    else:
        raise AssertionError("expected ValueError for missing secret")


def test_server_credential_admin_service_updates_existing_ciphertext_when_new_value_is_provided():
    repository = FakeServerCredentialAdminRepository()
    cipher = ServerCredentialCipher("test-server-credential-secret")
    repository.entries[1] = replace(repository.entries[1], credential_ciphertext=cipher.encrypt("old-secret"))
    checker = FakeServerCredentialHealthChecker()
    service = ServerCredentialAdminService(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=checker,
    )

    item = service.update_server_credential(
        1,
        UpdateServerCredentialCommand.model_validate(
            {
                "execution_profile_id": 1,
                "api_protocol": "openai_compatible",
                "auth_type": "api_key",
                "credential": "new-secret",
                "api_base_url": "https://api.openai.com/v1",
                "label": "openai-main-a",
                "priority": 11,
                "enabled": True,
            }
        ),
    )

    assert item.priority == 11
    assert cipher.decrypt(repository.entries[1].credential_ciphertext) == "new-secret"


def test_server_credential_admin_service_disables_credential_without_running_health_check():
    repository = FakeServerCredentialAdminRepository()
    cipher = ServerCredentialCipher("test-server-credential-secret")
    checker = FakeServerCredentialHealthChecker()
    service = ServerCredentialAdminService(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=checker,
    )

    item = service.set_server_credential_enabled(1, False)

    assert item.enabled is False
    assert item.health_status == "disabled"
    assert checker.calls == []


def test_server_credential_admin_service_does_not_expose_delete_operation():
    repository = FakeServerCredentialAdminRepository()
    cipher = ServerCredentialCipher("test-server-credential-secret")
    checker = FakeServerCredentialHealthChecker()
    service = ServerCredentialAdminService(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=checker,
    )

    assert not hasattr(service, "delete_server_credential")
    assert repository.get_server_credential(1) is not None


def test_server_credential_admin_service_runs_health_check_and_records_result():
    repository = FakeServerCredentialAdminRepository()
    cipher = ServerCredentialCipher("test-server-credential-secret")
    repository.entries[1] = replace(
        repository.entries[1],
        credential_ciphertext=cipher.encrypt("live-token"),
        last_checked_at=datetime.now(UTC),
    )
    checker = FakeServerCredentialHealthChecker(
        ServerCredentialHealthCheckResult(
            status="rate_limited", error_code="http_429", error_message="limited", latency_ms=55
        )
    )
    service = ServerCredentialAdminService(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=checker,
    )

    result = service.run_health_check(1)

    assert result.credential_id == 1
    assert result.health_status == "rate_limited"
    assert repository.last_health_payload is not None
    assert repository.last_health_payload["status"] == "rate_limited"
    assert checker.calls[0]["credential"] == "live-token"


@pytest.mark.asyncio
async def test_server_credential_admin_service_runs_cli_health_check_with_bound_profile():
    repository = FakeServerCredentialAdminRepository()
    cipher = ServerCredentialCipher("test-server-credential-secret")
    repository.entries[1] = replace(repository.entries[1], credential_ciphertext=cipher.encrypt("live-token"))
    checker = FakeServerCredentialHealthChecker()
    cli_runner = FakeCliHealthCheckRunner()
    service = ServerCredentialAdminService(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=checker,
        cli_health_check_runner=cli_runner,
    )

    result = await service.run_cli_health_check(1)

    assert isinstance(result, AdminServerCredentialCliHealthCheckView)
    assert result.cli_health_status == "healthy"
    assert result.runner_type == "codex_cli"
    assert result.model == "gpt-5.4"
    assert cli_runner.calls[0][1]["mode"] == "agent_cli"
    assert cli_runner.calls[0][1]["agent_backend"] == "codex"
    assert cli_runner.calls[0][1]["provider"] == "openai"
    assert cli_runner.calls[0][1]["api_key"] == "live-token"


@pytest.mark.asyncio
async def test_server_credential_admin_service_reports_cli_health_check_failure_detail():
    repository = FakeServerCredentialAdminRepository()
    cipher = ServerCredentialCipher("test-server-credential-secret")
    repository.entries[1] = replace(repository.entries[1], credential_ciphertext=cipher.encrypt("bad-token"))
    checker = FakeServerCredentialHealthChecker()
    cli_runner = FakeCliHealthCheckRunner(RuntimeError("Codex CLI 退出码 1\n401 Unauthorized"))
    service = ServerCredentialAdminService(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=checker,
        cli_health_check_runner=cli_runner,
    )

    result = await service.run_cli_health_check(1)

    assert result.cli_health_status == "degraded"
    assert result.error_code == "cli_execution_failed"
    assert "401 Unauthorized" in result.error_message
