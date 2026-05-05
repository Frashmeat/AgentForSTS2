from __future__ import annotations

import sys
from datetime import UTC, datetime, timedelta
from pathlib import Path
import zipfile

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

pytest.importorskip("sqlalchemy")
fastapi = pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.composition.container import ApplicationContainer
from app.modules.auth.application.services import PBKDF2PasswordHasher
from app.modules.auth.infra.persistence import models as _auth_models
from app.modules.auth.infra.persistence.models import UserRecord
from app.modules.platform.application.services.platform_runtime_audit_service import PlatformRuntimeAuditService
from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher
from app.modules.platform.application.services.server_credential_health_checker import ServerCredentialHealthCheckResult
from app.modules.platform.contracts.job_commands import CreateJobCommand
from app.modules.platform.infra.persistence import models as _platform_models
from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    CredentialHealthCheckRecord,
    ExecutionChargeRecord,
    ExecutionProfileRecord,
    JobEventRecord,
    JobItemRecord,
    QuotaAccountRecord,
    QuotaAccountStatus,
    QuotaBalanceRecord,
    ServerCredentialRecord,
    UserPlatformPreferenceRecord,
)
from app.modules.platform.infra.persistence.repositories.job_repository_sqlalchemy import JobRepositorySqlAlchemy
from app.shared.infra.db.base import Base
from routers.auth_router import router as auth_router
from routers.platform_admin import router
from app.modules.knowledge.infra import knowledge_runtime


def _write_required_resource_docs_to_zip(archive: zipfile.ZipFile, text_prefix: str = "resource") -> None:
    for resource_path in knowledge_runtime.REQUIRED_RESOURCE_FILES:
        archive.writestr(resource_path, f"{text_prefix} {resource_path}\n")


class FakePlatformHealthChecker:
    def __init__(self, result: ServerCredentialHealthCheckResult | None = None) -> None:
        self.result = result or ServerCredentialHealthCheckResult(status="healthy", latency_ms=8)
        self.calls: list[dict] = []

    def check(self, **payload) -> ServerCredentialHealthCheckResult:
        self.calls.append(payload)
        return self.result


class FakeCliHealthCheckRunner:
    def __init__(self) -> None:
        self.calls: list[tuple[str, dict, object | None]] = []

    async def __call__(self, prompt: str, llm_cfg: dict, cwd: object | None = None) -> str:
        self.calls.append((prompt, llm_cfg, cwd))
        return "OK"


class FakeWorkstationRuntimeStatus:
    def __init__(self, capabilities: dict[str, object] | None = None) -> None:
        self._capabilities = capabilities

    def model_dump(self):
        return {
            "available": True,
            "auto_start": True,
            "managed": True,
            "running": True,
            "workstation_url": "http://127.0.0.1:7860",
            "control_token_env": "ATS_WORKSTATION_CONTROL_TOKEN",
            "pid": 12345,
            "last_error": "",
            "capabilities": self._capabilities,
            "stdout_log_path": "runtime/logs/web-workstation.stdout.log",
            "stderr_log_path": "runtime/logs/web-workstation.stderr.log",
            "workstation_config_path": "runtime/workstation.config.json",
            "runtime_root": "runtime",
        }


class FakeWorkstationRuntimeManager:
    def __init__(self, capabilities: dict[str, object] | None) -> None:
        self._status = FakeWorkstationRuntimeStatus(capabilities)

    def get_runtime_status(self):
        return self._status

    def read_runtime_log_tail(self, stream: str, tail_bytes: int = 65_536):
        if stream not in {"stdout", "stderr"}:
            raise ValueError("stream must be stdout or stderr")
        return type(
            "FakeWorkstationRuntimeLogTail",
            (),
            {
                "model_dump": lambda _self: {
                    "stream": stream,
                    "path": f"runtime/logs/web-workstation.{stream}.log",
                    "exists": True,
                    "size_bytes": 21,
                    "tail_bytes": tail_bytes,
                    "truncated": False,
                    "content": f"{stream} log tail raw_error=blocked",
                }
            },
        )()


@pytest.fixture()
def client(tmp_path):
    db_path = tmp_path / "platform-admin.sqlite3"
    container = ApplicationContainer.from_config(
        {
            "database": {
                "url": f"sqlite+pysqlite:///{db_path.as_posix()}",
            },
            "auth": {
                "session_secret": "test-session-secret",
            },
        },
        runtime_role="web",
    )
    fake_health_checker = FakePlatformHealthChecker()
    fake_cli_health_runner = FakeCliHealthCheckRunner()
    container.register_singleton("platform.server_credential_health_checker_factory", lambda: fake_health_checker)
    server_credential_admin_service_factory = container.resolve_singleton(
        "platform.server_credential_admin_service_factory"
    )
    container.register_singleton(
        "platform.server_credential_admin_service_factory",
        lambda **kwargs: server_credential_admin_service_factory(
            **kwargs,
            cli_health_check_runner=fake_cli_health_runner,
        ),
    )
    runtime_audit_service = PlatformRuntimeAuditService(
        session_factory=container.resolve_singleton("platform.db_session_factory"),
        storage_root=tmp_path / "runtime-audit",
    )
    runtime_audit_service.append_event(
        event_type="runtime.queue_worker.leader_acquired",
        payload={
            "owner_id": "queue-worker:test",
            "leader_epoch": 1,
            "detail": "queue worker became leader",
        },
    )
    container.register_singleton("platform.runtime_audit_service_factory", runtime_audit_service)
    session = container.resolve_singleton("platform.db_session_factory")()
    cipher = ServerCredentialCipher.from_settings(container.resolve_singleton("settings"))
    Base.metadata.create_all(session.bind)
    job_repository = JobRepositorySqlAlchemy(session)
    job = job_repository.create_job_with_items(
        user_id=1001,
        command=CreateJobCommand.model_validate(
            {"job_type": "single_generate", "workflow_version": "2026.03.31", "items": [{"item_type": "card"}]}
        ),
    )
    session.flush()
    execution = AIExecutionRecord(
        job_id=job.id,
        job_item_id=job.items[0].id,
        user_id=1001,
        status="succeeded",
        api_protocol="openai_compatible",
        model="gpt-5.4",
        credential_ref="cred-a",
        retry_attempt=1,
        switched_credential=True,
        request_idempotency_key="idem-admin",
        workflow_version="2026.03.31",
        step_protocol_version="v1",
        result_schema_version="v1",
        step_type="image.generate",
        step_id="step-1",
    )
    session.add(execution)
    session.flush()
    session.add(
        ExecutionChargeRecord(
            ai_execution_id=execution.id,
            user_id=1001,
            charge_status="refunded",
            charge_amount=1,
            refund_reason="system_error",
        )
    )
    session.add(
        JobEventRecord(
            job_id=job.id,
            job_item_id=job.items[0].id,
            ai_execution_id=execution.id,
            user_id=1001,
            event_type="ai_execution.finished",
            event_payload={"status": "succeeded"},
        )
    )
    profile = ExecutionProfileRecord(
        code="codex-gpt-5-4",
        display_name="Codex CLI / gpt-5.4",
        runner_type="codex_cli",
        model="gpt-5.4",
        description="默认推荐",
        enabled=True,
        recommended=True,
        sort_order=10,
    )
    session.add(profile)
    session.flush()
    session.add(
        ServerCredentialRecord(
            execution_profile_id=profile.id,
            api_protocol="openai_compatible",
            auth_type="api_key",
            credential_ciphertext=cipher.encrypt("seed-openai-main"),
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
    )
    session.add(
        UserRecord(
            username="admin",
            email="admin@example.com",
            password_hash=PBKDF2PasswordHasher(iterations=1).hash_password("admin-pass"),
            email_verified=True,
            is_admin=True,
        )
    )
    session.add(
        UserRecord(
            username="user",
            email="user@example.com",
            password_hash=PBKDF2PasswordHasher(iterations=1).hash_password("user-pass"),
            email_verified=True,
            is_admin=False,
        )
    )
    session.add(
        UserRecord(
            user_id=1001,
            username="quota-user",
            email="quota-user@example.com",
            password_hash=PBKDF2PasswordHasher(iterations=1).hash_password("quota-pass"),
            email_verified=True,
            is_admin=False,
        )
    )
    account = QuotaAccountRecord(user_id=1001, status=QuotaAccountStatus.ACTIVE)
    session.add(account)
    session.flush()
    session.add(
        QuotaBalanceRecord(
            user_id=1001,
            quota_account_id=account.id,
            total_limit=10,
            used_amount=2,
            refunded_amount=1,
            adjusted_amount=0,
            status=QuotaAccountStatus.ACTIVE,
        )
    )
    session.commit()
    session.close()

    app = FastAPI()
    app.state.container = container
    app.include_router(auth_router, prefix="/api")
    app.include_router(router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client, job.id, execution.id, fake_health_checker, fake_cli_health_runner


def test_platform_admin_router_supports_execution_refund_and_audit_queries(client):
    test_client, job_id, execution_id, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    executions = test_client.get(f"/api/admin/jobs/{job_id}/executions")
    assert executions.status_code == 200
    assert executions.json()[0]["job_id"] == job_id

    detail = test_client.get(f"/api/admin/executions/{execution_id}")
    assert detail.status_code == 200
    assert detail.json()["request_idempotency_key"] == "idem-admin"
    assert detail.json()["credential_ref"] == "cred-a"
    assert detail.json()["retry_attempt"] == 1
    assert detail.json()["switched_credential"] is True

    refunds = test_client.get("/api/admin/quota/refunds", params={"user_id": 1001})
    assert refunds.status_code == 200
    assert refunds.json()[0]["refund_reason"] == "system_error"

    audit = test_client.get("/api/admin/audit/events", params={"job_id": job_id})
    assert audit.status_code == 200
    assert audit.json()[0]["event_type"] == "ai_execution.finished"

    merged_audit = test_client.get("/api/admin/audit/events")
    assert merged_audit.status_code == 200
    assert any(item["event_type"] == "runtime.queue_worker.leader_acquired" for item in merged_audit.json())

    filtered_runtime_audit = test_client.get(
        "/api/admin/audit/events",
        params={"event_type_prefix": "runtime.queue_worker."},
    )
    assert filtered_runtime_audit.status_code == 200
    assert filtered_runtime_audit.json()
    assert all(item["event_type"].startswith("runtime.queue_worker.") for item in filtered_runtime_audit.json())

    limited_runtime_audit = test_client.get(
        "/api/admin/audit/events",
        params={"event_type_prefix": "runtime.queue_worker.", "after_id": 0, "limit": 1},
    )
    assert limited_runtime_audit.status_code == 200
    assert len(limited_runtime_audit.json()) == 1

    credentials = test_client.get("/api/admin/platform/server-credentials")
    assert credentials.status_code == 200
    assert credentials.json()["items"][0]["label"] == "openai-main-a"
    assert credentials.json()["items"][0]["execution_profile_id"] == 1

    profiles = test_client.get("/api/admin/platform/execution-profiles")
    assert profiles.status_code == 200
    assert profiles.json()["items"][0]["code"] == "codex-gpt-5-4"
    assert profiles.json()["items"][0]["description"] == "默认推荐"


def test_platform_admin_router_lists_failed_job_items_without_execution_detail(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    session = test_client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        job_repository = JobRepositorySqlAlchemy(session)
        job = job_repository.create_job_with_items(
            user_id=1001,
            command=CreateJobCommand.model_validate(
                {
                    "job_type": "batch_generate",
                    "workflow_version": "2026.03.31",
                    "items": [{"item_type": "card", "input_summary": "预检查失败"}],
                }
            ),
        )
        session.flush()
        failed_item = session.query(JobItemRecord).filter(JobItemRecord.job_id == job.id).one()
        failed_item.status = "failed_system"
        failed_item.error_summary = "创建 AI 执行前失败"
        failed_job_id = job.id
        failed_job_item_id = failed_item.id
        session.commit()
    finally:
        session.close()

    executions = test_client.get(f"/api/admin/jobs/{failed_job_id}/executions")

    assert executions.status_code == 200
    payload = executions.json()
    assert len(payload) == 1
    assert payload[0]["record_kind"] == "job_item"
    assert payload[0]["id"] is None
    assert payload[0]["execution_id"] is None
    assert payload[0]["job_item_id"] == failed_job_item_id
    assert payload[0]["status"] == "failed_system"
    assert payload[0]["error_summary"] == "创建 AI 执行前失败"


def test_platform_admin_router_returns_workstation_runtime_status(client, monkeypatch, tmp_path: Path):
    test_client, _, _, _, _ = client
    runtime_root = tmp_path / "runtime"
    knowledge_root = runtime_root / "knowledge"
    monkeypatch.setattr(knowledge_runtime, "RUNTIME_ROOT", runtime_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(
        knowledge_runtime,
        "get_active_knowledge_pack",
        lambda: {"pack_id": "pack-web", "label": "Web Knowledge"},
    )

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    missing = test_client.get("/api/admin/platform/workstation-runtime-status")
    assert missing.status_code == 200
    missing_payload = missing.json()
    assert missing_payload["available"] is False
    assert missing_payload["reason"] == "workstation_runtime_manager_not_registered"
    assert missing_payload["web_knowledge"]["active_pack_id"] == "pack-web"
    assert missing_payload["knowledge_runtime_consistent"] is False

    test_client.app.state.workstation_runtime_manager = FakeWorkstationRuntimeManager(
        {
            "available": True,
            "runtime_root": str(runtime_root),
            "knowledge_root": str(knowledge_root),
            "knowledge": {"active_knowledge_pack_id": "pack-web"},
        }
    )
    response = test_client.get("/api/admin/platform/workstation-runtime-status")
    assert response.status_code == 200
    payload = response.json()
    assert payload["available"] is True
    assert payload["running"] is True
    assert payload["stdout_log_path"] == "runtime/logs/web-workstation.stdout.log"
    assert payload["stderr_log_path"] == "runtime/logs/web-workstation.stderr.log"
    assert payload["web_knowledge"]["active_pack_id"] == "pack-web"
    assert payload["web_knowledge"]["active_pack_label"] == "Web Knowledge"
    assert payload["knowledge_runtime_consistent"] is True
    assert payload["knowledge_runtime_mismatch_reason"] == ""

    test_client.app.state.workstation_runtime_manager = FakeWorkstationRuntimeManager(
        {
            "available": True,
            "runtime_root": str(runtime_root / "other"),
            "knowledge_root": str(knowledge_root),
            "knowledge": {"active_knowledge_pack_id": "pack-web"},
        }
    )
    mismatch = test_client.get("/api/admin/platform/workstation-runtime-status")
    assert mismatch.status_code == 200
    mismatch_payload = mismatch.json()
    assert mismatch_payload["knowledge_runtime_consistent"] is False
    assert "runtime root mismatch" in mismatch_payload["knowledge_runtime_mismatch_reason"]


def test_platform_admin_router_returns_workstation_runtime_logs(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    missing = test_client.get("/api/admin/platform/workstation-runtime-logs")
    assert missing.status_code == 503

    test_client.app.state.workstation_runtime_manager = FakeWorkstationRuntimeManager(
        {
            "available": True,
            "runtime_root": "runtime",
            "knowledge_root": "runtime/knowledge",
            "knowledge": {"active_knowledge_pack_id": ""},
        }
    )
    response = test_client.get(
        "/api/admin/platform/workstation-runtime-logs",
        params={"stream": "stderr", "tail_bytes": 4096},
    )
    assert response.status_code == 200
    payload = response.json()
    assert payload["stream"] == "stderr"
    assert payload["path"] == "runtime/logs/web-workstation.stderr.log"
    assert payload["tail_bytes"] == 4096
    assert "raw_error=blocked" in payload["content"]

    rejected = test_client.get("/api/admin/platform/workstation-runtime-logs", params={"stream": "../config"})
    assert rejected.status_code == 400


def test_platform_admin_router_deletes_knowledge_pack(client, monkeypatch, tmp_path: Path):
    test_client, _, _, _, _ = client
    knowledge_root = tmp_path / "runtime" / "knowledge"
    packs_dir = knowledge_root / "packs"
    active_path = knowledge_root / "active-knowledge-pack.json"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"
    archive_path = tmp_path / "pack.zip"

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)
    monkeypatch.setattr(knowledge_runtime, "ACTIVE_KNOWLEDGE_PACK_PATH", active_path)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_runtime_dir)

    with zipfile.ZipFile(archive_path, "w") as archive:
        _write_required_resource_docs_to_zip(archive, "common")
        for index in range(knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT):
            archive.writestr(f"game/Game{index}.cs", "// game\n")
        archive.writestr("baselib/BaseLib.decompiled.cs", "// baselib\n")

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    uploaded = test_client.post(
        "/api/admin/platform/knowledge-packs",
        files={"file": ("pack.zip", archive_path.read_bytes(), "application/zip")},
    )
    assert uploaded.status_code == 200
    pack_id = uploaded.json()["pack_id"]

    activated = test_client.post(f"/api/admin/platform/knowledge-packs/{pack_id}/activate")
    assert activated.status_code == 200

    deleted = test_client.delete(f"/api/admin/platform/knowledge-packs/{pack_id}")
    assert deleted.status_code == 200
    assert deleted.json()["deleted"] is True
    assert deleted.json()["was_active"] is True
    assert deleted.json()["active_pack_id"] == ""

    listed = test_client.get("/api/admin/platform/knowledge-packs")
    assert listed.status_code == 200
    assert listed.json()["items"] == []


def test_platform_admin_router_manages_execution_profiles(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    created = test_client.post(
        "/api/admin/platform/execution-profiles",
        json={
            "code": "codex-gpt-5-5",
            "display_name": "Codex CLI / gpt-5.5",
            "runner_type": "codex_cli",
            "model": "gpt-5.5",
            "description": "新模型配置",
            "enabled": True,
            "recommended": False,
            "sort_order": 30,
        },
    )
    assert created.status_code == 200
    created_payload = created.json()
    assert created_payload["code"] == "codex-gpt-5-5"
    assert created_payload["model"] == "gpt-5.5"
    assert created_payload["enabled"] is True

    duplicate = test_client.post(
        "/api/admin/platform/execution-profiles",
        json={
            "code": "codex-gpt-5-5",
            "display_name": "Duplicate",
            "runner_type": "codex_cli",
            "model": "gpt-5.5",
        },
    )
    assert duplicate.status_code == 400

    updated = test_client.put(
        f"/api/admin/platform/execution-profiles/{created_payload['id']}",
        json={
            "code": "codex-gpt-5-5-latest",
            "display_name": "Codex CLI / gpt-5.5 latest",
            "runner_type": "codex_cli",
            "model": "gpt-5.5",
            "description": "更新后的配置",
            "enabled": True,
            "recommended": True,
            "sort_order": 5,
        },
    )
    assert updated.status_code == 200
    assert updated.json()["code"] == "codex-gpt-5-5-latest"
    assert updated.json()["recommended"] is True

    disabled = test_client.post(f"/api/admin/platform/execution-profiles/{created_payload['id']}/disable")
    assert disabled.status_code == 200
    assert disabled.json()["enabled"] is False

    enabled = test_client.post(f"/api/admin/platform/execution-profiles/{created_payload['id']}/enable")
    assert enabled.status_code == 200
    assert enabled.json()["enabled"] is True

    deleted = test_client.delete(f"/api/admin/platform/execution-profiles/{created_payload['id']}")
    assert deleted.status_code == 200
    assert deleted.json()["deleted"] is True


def test_platform_admin_router_rejects_deleting_referenced_execution_profile(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    referenced = test_client.delete("/api/admin/platform/execution-profiles/1")
    assert referenced.status_code == 409
    assert "referenced" in referenced.json()["detail"]

    session = test_client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        profile = ExecutionProfileRecord(
            code="claude-extra",
            display_name="Claude extra",
            runner_type="claude_cli",
            model="claude-sonnet-4-6",
            description="",
            enabled=True,
            recommended=False,
            sort_order=40,
        )
        session.add(profile)
        session.flush()
        session.add(UserPlatformPreferenceRecord(user_id=1001, default_execution_profile_id=profile.id))
        profile_id = profile.id
        session.commit()
    finally:
        session.close()

    preferred = test_client.delete(f"/api/admin/platform/execution-profiles/{profile_id}")
    assert preferred.status_code == 409
    assert "referenced" in preferred.json()["detail"]


def test_platform_admin_router_supports_user_quota_management(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    users = test_client.get("/api/admin/users")
    assert users.status_code == 200
    assert any(item["user_id"] == 1001 for item in users.json()["items"])

    quota = test_client.get("/api/admin/users/1001/quota")
    assert quota.status_code == 200
    assert quota.json()["remaining"] == 9

    adjusted = test_client.post(
        "/api/admin/users/1001/quota/adjust",
        json={"direction": "grant", "amount": 5, "reason": "补偿测试额度"},
    )
    assert adjusted.status_code == 200
    assert adjusted.json()["adjusted_amount"] == 5
    assert adjusted.json()["remaining"] == 14

    ledger = test_client.get("/api/admin/users/1001/quota/ledger")
    assert ledger.status_code == 200
    assert ledger.json()["items"][0]["ledger_type"] == "admin_grant"

    rejected = test_client.post(
        "/api/admin/users/1001/quota/adjust",
        json={"direction": "deduct", "amount": 99, "reason": "too much"},
    )
    assert rejected.status_code == 400


def test_platform_admin_router_creates_server_credential_with_ciphertext_storage(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    created = test_client.post(
        "/api/admin/platform/server-credentials",
        json={
            "execution_profile_id": 1,
            "api_protocol": "openai_compatible",
            "auth_type": "api_key",
            "credential": "sk-live-main",
            "secret": "",
            "api_base_url": "https://api.openai.com/v1",
            "label": "openai-main-b",
            "priority": 20,
            "enabled": True,
        },
    )
    assert created.status_code == 200
    payload = created.json()
    assert payload["label"] == "openai-main-b"
    assert payload["api_protocol"] == "openai_compatible"
    assert "credential" not in payload
    assert "secret" not in payload

    session = test_client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        row = session.query(ServerCredentialRecord).filter_by(label="openai-main-b").one()
        assert row.credential_ciphertext != "sk-live-main"
        assert row.secret_ciphertext is None
        cipher = ServerCredentialCipher.from_settings(test_client.app.state.container.resolve_singleton("settings"))
        assert cipher.decrypt(row.credential_ciphertext) == "sk-live-main"
    finally:
        session.close()


def test_platform_admin_router_updates_and_toggles_server_credential(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    updated = test_client.put(
        "/api/admin/platform/server-credentials/1",
        json={
            "execution_profile_id": 1,
            "api_protocol": "openai_compatible",
            "auth_type": "api_key",
            "credential": "openai-key-1",
            "api_base_url": "https://api.openai.com/v1",
            "label": "openai-main-a-updated",
            "priority": 15,
            "enabled": True,
        },
    )
    assert updated.status_code == 200
    assert updated.json()["api_protocol"] == "openai_compatible"
    assert updated.json()["label"] == "openai-main-a-updated"

    disabled = test_client.post("/api/admin/platform/server-credentials/1/disable")
    assert disabled.status_code == 200
    assert disabled.json()["enabled"] is False
    assert disabled.json()["health_status"] == "disabled"

    enabled = test_client.post("/api/admin/platform/server-credentials/1/enable")
    assert enabled.status_code == 200
    assert enabled.json()["enabled"] is True
    assert enabled.json()["health_status"] == "degraded"


def test_platform_admin_router_rejects_server_credential_incompatible_with_execution_profile(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    created = test_client.post(
        "/api/admin/platform/server-credentials",
        json={
            "execution_profile_id": 1,
            "api_protocol": "anthropic_compatible",
            "auth_type": "api_key",
            "credential": "anthropic-key",
            "api_base_url": "https://api.anthropic.com",
            "label": "anthropic-wrong-profile",
            "priority": 20,
            "enabled": True,
        },
    )

    assert created.status_code == 400
    assert "api_protocol must be one of openai_compatible" in created.json()["detail"]


def test_platform_admin_router_does_not_delete_server_credential(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    deleted = test_client.delete("/api/admin/platform/server-credentials/1")
    assert deleted.status_code == 405

    credentials = test_client.get("/api/admin/platform/server-credentials")
    assert credentials.status_code == 200
    assert credentials.json()["items"][0]["id"] == 1


def test_platform_admin_router_runs_manual_health_check_and_writes_result(client):
    test_client, _, _, fake_health_checker, _ = client

    fake_health_checker.result = ServerCredentialHealthCheckResult(
        status="rate_limited",
        error_code="http_429",
        error_message="limited",
        latency_ms=21,
    )
    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    checked = test_client.post("/api/admin/platform/server-credentials/1/health-check")
    assert checked.status_code == 200
    payload = checked.json()
    assert payload["credential_id"] == 1
    assert payload["health_status"] == "rate_limited"
    assert payload["error_code"] == "http_429"

    session = test_client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        row = session.query(ServerCredentialRecord).filter_by(id=1).one()
        assert row.health_status == "rate_limited"
        checks = session.query(CredentialHealthCheckRecord).filter_by(server_credential_id=1).all()
        assert len(checks) == 1
        assert checks[0].trigger_source == "manual"
        assert checks[0].status == "rate_limited"
    finally:
        session.close()


def test_platform_admin_router_runs_cli_health_check_with_bound_credential(client):
    test_client, _, _, _, fake_cli_health_runner = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    checked = test_client.post("/api/admin/platform/server-credentials/1/cli-health-check")
    assert checked.status_code == 200
    payload = checked.json()
    assert payload["credential_id"] == 1
    assert payload["execution_profile_id"] == 1
    assert payload["runner_type"] == "codex_cli"
    assert payload["api_protocol"] == "openai_compatible"
    assert payload["model"] == "gpt-5.4"
    assert payload["cli_health_status"] == "healthy"
    assert fake_cli_health_runner.calls[0][1]["agent_backend"] == "codex"
    assert fake_cli_health_runner.calls[0][1]["api_key"] == "seed-openai-main"


def test_platform_admin_router_requires_authenticated_admin_session(client):
    test_client, job_id, _, _, _ = client

    unauthenticated = test_client.get(f"/api/admin/jobs/{job_id}/executions")
    assert unauthenticated.status_code == 401

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "user@example.com",
            "password": "user-pass",
        },
    )
    assert login.status_code == 200

    forbidden = test_client.get(f"/api/admin/jobs/{job_id}/executions")
    assert forbidden.status_code == 403
    assert forbidden.json()["detail"] == "admin permission required"

    forbidden_write = test_client.post(
        "/api/admin/platform/server-credentials",
        json={
            "execution_profile_id": 1,
            "api_protocol": "openai_compatible",
            "auth_type": "api_key",
            "credential": "sk-denied",
            "label": "should-fail",
        },
    )
    assert forbidden_write.status_code == 403
    assert forbidden_write.json()["detail"] == "admin permission required"


def test_platform_admin_router_rejects_invalid_server_credential_payload(client):
    test_client, _, _, _, _ = client

    login = test_client.post(
        "/api/auth/login",
        json={
            "login": "admin@example.com",
            "password": "admin-pass",
        },
    )
    assert login.status_code == 200

    response = test_client.post(
        "/api/admin/platform/server-credentials",
        json={
            "execution_profile_id": 1,
            "api_protocol": "openai_compatible",
            "auth_type": "ak_sk",
            "credential": "ak-live",
            "label": "missing-secret",
        },
    )
    assert response.status_code == 400
    assert response.json()["detail"] == "secret is required when auth_type is ak_sk"

    missing = test_client.put(
        "/api/admin/platform/server-credentials/999",
        json={
            "execution_profile_id": 1,
            "api_protocol": "openai_compatible",
            "auth_type": "api_key",
            "api_base_url": "https://api.openai.com/v1",
            "label": "missing",
            "priority": 1,
            "enabled": True,
        },
    )
    assert missing.status_code == 404
