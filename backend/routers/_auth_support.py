from __future__ import annotations

import hashlib
import hmac
import secrets
from contextlib import contextmanager
from datetime import UTC, datetime, timedelta

from fastapi import HTTPException, Request, Response

from app.modules.auth.application import AuthService
from app.modules.auth.domain import EmailVerificationTicket, UserAccount

SESSION_MAX_AGE_SECONDS = 60 * 60 * 24 * 30


def _container(request: Request):
    return request.app.state.container


def _settings(request: Request):
    return _container(request).resolve_singleton("settings")


@contextmanager
def auth_session_scope(request: Request):
    session_factory = _container(request).resolve_optional_singleton("auth.db_session_factory")
    if session_factory is None:
        raise HTTPException(status_code=503, detail="auth database session is not configured")
    session = session_factory()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()


def build_auth_service(session, request: Request) -> AuthService:
    container = _container(request)
    user_repository = container.resolve_singleton("auth.user_repository_factory")(session)
    password_hasher = container.resolve_singleton("auth.password_hasher_factory")()
    return container.resolve_singleton("auth.auth_service_factory")(
        user_repository=user_repository,
        password_hasher=password_hasher,
    )


def build_email_verification_repository(session, request: Request):
    container = _container(request)
    return container.resolve_singleton("auth.email_verification_repository_factory")(session)


def build_user_repository(session, request: Request):
    container = _container(request)
    return container.resolve_singleton("auth.user_repository_factory")(session)


def _session_cookie_name(request: Request) -> str:
    return str(_settings(request).auth.get("session_cookie_name", "agentthespire_session"))


def _session_secret(request: Request) -> str:
    configured = str(_settings(request).auth.get("session_secret", "")).strip()
    return configured or "agentthespire-dev-session-secret"


def create_session_token(request: Request, user_id: int) -> str:
    payload = str(user_id)
    signature = hmac.new(
        _session_secret(request).encode("utf-8"),
        payload.encode("utf-8"),
        hashlib.sha256,
    ).hexdigest()
    return f"{payload}.{signature}"


def parse_session_token(request: Request, token: str | None) -> int | None:
    if not token:
        return None
    try:
        payload, signature = token.split(".", 1)
    except ValueError:
        return None
    expected = hmac.new(
        _session_secret(request).encode("utf-8"),
        payload.encode("utf-8"),
        hashlib.sha256,
    ).hexdigest()
    if not hmac.compare_digest(signature, expected):
        return None
    try:
        return int(payload)
    except ValueError:
        return None


def issue_session_cookie(request: Request, response: Response, user_id: int) -> None:
    response.set_cookie(
        key=_session_cookie_name(request),
        value=create_session_token(request, user_id),
        httponly=True,
        samesite="lax",
        max_age=SESSION_MAX_AGE_SECONDS,
    )


def clear_session_cookie(request: Request, response: Response) -> None:
    response.delete_cookie(_session_cookie_name(request))


def get_current_user(request: Request, session) -> UserAccount | None:
    token = request.cookies.get(_session_cookie_name(request))
    user_id = parse_session_token(request, token)
    if user_id is None:
        return None
    user_repository = build_user_repository(session, request)
    return user_repository.get_by_user_id(user_id)


def require_current_user(request: Request, session) -> UserAccount:
    user = get_current_user(request, session)
    if user is None:
        raise HTTPException(status_code=401, detail="authentication required")
    return user


def create_verification_code() -> str:
    return secrets.token_urlsafe(16)


def create_email_ticket(user_id: int, email: str, purpose: str, request: Request, session) -> EmailVerificationTicket:
    auth_settings = _settings(request).auth
    ttl_key = "password_reset_code_ttl_seconds" if purpose == "reset_password" else "email_verification_code_ttl_seconds"
    ttl_seconds = int(auth_settings.get(ttl_key, 1800))
    repository = build_email_verification_repository(session, request)
    return repository.create_ticket(
        user_id=user_id,
        purpose=purpose,
        code=create_verification_code(),
        email=email,
        expires_at=datetime.now(UTC) + timedelta(seconds=ttl_seconds),
    )
