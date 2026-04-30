from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

pytest.importorskip("sqlalchemy")
pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.composition.container import ApplicationContainer
from app.modules.auth.infra.persistence import models as _auth_models
from app.modules.platform.infra.persistence import models as _platform_models
from app.modules.platform.infra.persistence.models import (
    ExecutionProfileRecord,
    ServerCredentialRecord,
    UserPlatformPreferenceRecord,
)
from app.shared.infra.db.base import Base
from routers.auth_router import router as auth_router
from routers.me_router import router as me_router
from routers.platform_jobs import router as platform_router


@pytest.fixture()
def client(tmp_path):
    db_path = tmp_path / "platform-server-execution.sqlite3"
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
    session = container.resolve_singleton("platform.db_session_factory")()
    Base.metadata.create_all(session.bind)
    session.close()

    app = FastAPI()
    app.state.container = container
    app.include_router(auth_router, prefix="/api")
    app.include_router(me_router, prefix="/api")
    app.include_router(platform_router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client


def _register_login_and_verify(client: TestClient, username: str, email: str) -> int:
    registered = client.post(
        "/api/auth/register",
        json={
            "username": username,
            "email": email,
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    user_id = registered.json()["user"]["user_id"]
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": username,
            "password": "secret-123",
        },
    )
    assert login.status_code == 200

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200
    return user_id


def test_platform_execution_profiles_lists_enabled_profiles_and_availability(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        codex = ExecutionProfileRecord(
            code="codex-gpt-5-4",
            display_name="Codex CLI / gpt-5.4",
            agent_backend="codex",
            model="gpt-5.4",
            description="默认推荐",
            enabled=True,
            recommended=True,
            sort_order=10,
        )
        claude = ExecutionProfileRecord(
            code="claude-sonnet-4-6",
            display_name="Claude CLI / claude-sonnet-4-6",
            agent_backend="claude",
            model="claude-sonnet-4-6",
            description="备用组合",
            enabled=True,
            recommended=False,
            sort_order=20,
        )
        disabled = ExecutionProfileRecord(
            code="disabled-profile",
            display_name="Disabled",
            agent_backend="codex",
            model="gpt-x",
            description="不展示",
            enabled=False,
            recommended=False,
            sort_order=30,
        )
        session.add_all([codex, claude, disabled])
        session.flush()
        session.add(
            ServerCredentialRecord(
                execution_profile_id=codex.id,
                provider="openai",
                auth_type="api_key",
                credential_ciphertext="cipher",
                secret_ciphertext=None,
                base_url="https://api.openai.com/v1",
                label="main",
                priority=1,
                enabled=True,
                health_status="healthy",
                last_checked_at=None,
                last_error_code="",
                last_error_message="",
            )
        )
        session.add(
            ServerCredentialRecord(
                execution_profile_id=claude.id,
                provider="anthropic",
                auth_type="api_key",
                credential_ciphertext="cipher-2",
                secret_ciphertext=None,
                base_url="https://api.anthropic.com",
                label="backup",
                priority=1,
                enabled=True,
                health_status="rate_limited",
                last_checked_at=None,
                last_error_code="rate_limited",
                last_error_message="limited",
            )
        )
        session.commit()
    finally:
        session.close()

    response = client.get("/api/platform/execution-profiles")
    assert response.status_code == 200
    payload = response.json()
    assert [item["display_name"] for item in payload["items"]] == [
        "Codex CLI / gpt-5.4",
        "Claude CLI / claude-sonnet-4-6",
    ]
    assert payload["items"][0]["available"] is True
    assert payload["items"][1]["available"] is False


def test_platform_execution_profiles_requires_authenticated_platform_user(client: TestClient):
    response = client.get("/api/platform/execution-profiles")
    assert response.status_code == 401


def test_me_server_preferences_reads_and_updates_current_user_default_profile(client: TestClient):
    user_id = _register_login_and_verify(client, "luna", "luna@example.com")

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        profile = ExecutionProfileRecord(
            code="codex-gpt-5-4",
            display_name="Codex CLI / gpt-5.4",
            agent_backend="codex",
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
                provider="openai",
                auth_type="api_key",
                credential_ciphertext="cipher",
                secret_ciphertext=None,
                base_url="https://api.openai.com/v1",
                label="main",
                priority=1,
                enabled=True,
                health_status="healthy",
                last_checked_at=None,
                last_error_code="",
                last_error_message="",
            )
        )
        session.commit()
    finally:
        session.close()

    empty = client.get("/api/me/server-preferences")
    assert empty.status_code == 200
    assert empty.json()["default_execution_profile_id"] is None

    updated = client.put(
        "/api/me/server-preferences",
        json={"default_execution_profile_id": 1},
    )
    assert updated.status_code == 200
    assert updated.json()["default_execution_profile_id"] == 1
    assert updated.json()["available"] is True

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        preference = session.query(UserPlatformPreferenceRecord).filter_by(user_id=user_id).one()
        assert preference.default_execution_profile_id == 1
    finally:
        session.close()

    cleared = client.put(
        "/api/me/server-preferences",
        json={"default_execution_profile_id": None},
    )
    assert cleared.status_code == 200
    assert cleared.json()["default_execution_profile_id"] is None


def test_me_server_preferences_rejects_unavailable_profile_as_default(client: TestClient):
    _register_login_and_verify(client, "luna", "luna@example.com")

    session = client.app.state.container.resolve_singleton("platform.db_session_factory")()
    try:
        profile = ExecutionProfileRecord(
            code="claude-sonnet-4-6",
            display_name="Claude CLI / claude-sonnet-4-6",
            agent_backend="claude",
            model="claude-sonnet-4-6",
            description="当前不可用",
            enabled=True,
            recommended=False,
            sort_order=20,
        )
        session.add(profile)
        session.commit()
    finally:
        session.close()

    response = client.put(
        "/api/me/server-preferences",
        json={"default_execution_profile_id": 1},
    )
    assert response.status_code == 400
    assert "not available" in response.json()["detail"]
