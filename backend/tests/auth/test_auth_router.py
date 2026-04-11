from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

pytest.importorskip("sqlalchemy")
pytest.importorskip("fastapi")
pytest.importorskip("fastapi.testclient")

from fastapi import FastAPI
from fastapi.testclient import TestClient

from app.composition.container import ApplicationContainer
from app.modules.auth.infra.persistence import models as _auth_models  # noqa: F401
from app.modules.auth.infra.persistence.repositories import EmailVerificationRepositorySqlAlchemy
from app.shared.infra.db.base import Base
from routers.auth_router import router


@pytest.fixture()
def client(tmp_path):
    db_path = tmp_path / "auth-router.sqlite3"
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
    session = container.resolve_singleton("auth.db_session_factory")()
    Base.metadata.create_all(session.bind)
    session.close()

    app = FastAPI()
    app.state.container = container
    app.include_router(router, prefix="/api")

    with TestClient(app) as test_client:
        yield test_client


def _build_client(tmp_path, config_override: dict | None = None):
    db_path = tmp_path / "auth-router-custom.sqlite3"
    container = ApplicationContainer.from_config(
        {
            "database": {
                "url": f"sqlite+pysqlite:///{db_path.as_posix()}",
            },
            "auth": {
                "session_secret": "test-session-secret",
            },
            **(config_override or {}),
        },
        runtime_role="web",
    )
    session = container.resolve_singleton("auth.db_session_factory")()
    Base.metadata.create_all(session.bind)
    session.close()

    app = FastAPI()
    app.state.container = container
    app.include_router(router, prefix="/api")
    return TestClient(app)


def _latest_ticket_code(client: TestClient, purpose: str) -> str:
    container = client.app.state.container
    session = container.resolve_singleton("auth.db_session_factory")()
    try:
        record = (
            session.query(_auth_models.EmailVerificationRecord)
            .filter(_auth_models.EmailVerificationRecord.purpose == purpose)
            .order_by(_auth_models.EmailVerificationRecord.id.desc())
            .first()
        )
        assert record is not None
        repository = EmailVerificationRepositorySqlAlchemy(session)
        ticket = repository.get_by_code(record.code, purpose)
        assert ticket is not None
        return ticket.code
    finally:
        session.close()


def test_auth_router_register_login_me_verify_and_logout(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200
    verification_code = registered.json()["verification_code"]

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert login.status_code == 200
    assert "set-cookie" in {key.lower() for key in login.headers.keys()}

    me = client.get("/api/auth/me")
    assert me.status_code == 200
    assert me.json()["authenticated"] is True
    assert me.json()["user"]["username"] == "luna"

    verified = client.post("/api/auth/verify-email", json={"code": verification_code})
    assert verified.status_code == 200
    assert verified.json()["user"]["email_verified"] is True

    logged_out = client.post("/api/auth/logout")
    assert logged_out.status_code == 200


def test_auth_router_supports_password_reset_round_trip(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200

    forgot = client.post("/api/auth/forgot-password", json={"login": "luna@example.com"})
    assert forgot.status_code == 200
    assert forgot.json() == {"ok": True}
    reset_code = _latest_ticket_code(client, "reset_password")

    reset = client.post(
        "/api/auth/reset-password",
        json={
            "code": reset_code,
            "password": "secret-456",
        },
    )
    assert reset.status_code == 200
    assert "set-cookie" in {key.lower() for key in reset.headers.keys()}

    me = client.get("/api/auth/me")
    assert me.status_code == 200
    assert me.json()["authenticated"] is True
    assert me.json()["user"]["username"] == "luna"

    login = client.post(
        "/api/auth/login",
        json={
            "login": "luna",
            "password": "secret-456",
        },
    )
    assert login.status_code == 200


def test_auth_router_requires_authenticated_session_or_password_to_resend_verification(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200

    anonymous_resend = client.post(
        "/api/auth/resend-verification",
        json={
            "login": "luna@example.com",
        },
    )
    assert anonymous_resend.status_code == 401

    credentialed_resend = client.post(
        "/api/auth/resend-verification",
        json={
            "login": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert credentialed_resend.status_code == 200
    assert credentialed_resend.json()["verification_code"]


def test_auth_router_rejects_reused_password_reset_code(client: TestClient):
    registered = client.post(
        "/api/auth/register",
        json={
            "username": "luna",
            "email": "luna@example.com",
            "password": "secret-123",
        },
    )
    assert registered.status_code == 200

    forgot = client.post("/api/auth/forgot-password", json={"login": "luna@example.com"})
    assert forgot.status_code == 200
    assert forgot.json() == {"ok": True}
    reset_code = _latest_ticket_code(client, "reset_password")

    first_reset = client.post(
        "/api/auth/reset-password",
        json={
            "code": reset_code,
            "password": "secret-456",
        },
    )
    assert first_reset.status_code == 200

    second_reset = client.post(
        "/api/auth/reset-password",
        json={
            "code": reset_code,
            "password": "secret-789",
        },
    )
    assert second_reset.status_code == 404


def test_auth_router_uses_configured_cookie_attributes(tmp_path):
    with _build_client(
        tmp_path,
        {
            "auth": {
                "session_secret": "test-session-secret",
                "session_cookie_secure": True,
                "session_cookie_samesite": "none",
                "session_cookie_domain": ".example.com",
            }
        },
    ) as client:
        registered = client.post(
            "/api/auth/register",
            json={
                "username": "luna",
                "email": "luna@example.com",
                "password": "secret-123",
            },
        )
        assert registered.status_code == 200

        login = client.post(
            "/api/auth/login",
            json={
                "login": "luna@example.com",
                "password": "secret-123",
            },
        )
        assert login.status_code == 200

        cookie_header = login.headers["set-cookie"].lower()
        assert "secure" in cookie_header
        assert "samesite=none" in cookie_header
        assert "domain=.example.com" in cookie_header
