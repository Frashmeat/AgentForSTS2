from __future__ import annotations

from datetime import UTC, datetime

from fastapi import APIRouter, HTTPException, Request, Response

from ._auth_support import (
    auth_session_scope,
    build_auth_service,
    build_email_verification_repository,
    clear_session_cookie,
    create_email_ticket,
    get_current_user,
    issue_session_cookie,
)

router = APIRouter(prefix="/auth")


def _user_payload(user) -> dict:
    return {
        "user_id": user.user_id,
        "username": user.username,
        "email": user.email,
        "email_verified": user.email_verified,
        "is_admin": user.is_admin,
        "created_at": user.created_at.isoformat(),
        "email_verified_at": user.email_verified_at.isoformat() if user.email_verified_at else None,
    }


@router.post("/register")
def register(request: Request, body: dict):
    with auth_session_scope(request) as session:
        service = build_auth_service(session, request)
        user = service.register_user(
            username=str(body.get("username", "")).strip(),
            email=str(body.get("email", "")).strip(),
            password=str(body.get("password", "")),
        )
        ticket = create_email_ticket(user.user_id, user.email, "verify_email", request, session)
        return {
            "user": _user_payload(user),
            "verification_code": ticket.code,
        }


@router.post("/login")
def login(request: Request, response: Response, body: dict):
    with auth_session_scope(request) as session:
        service = build_auth_service(session, request)
        user = service.authenticate_user(
            login=str(body.get("login", "")).strip(),
            password=str(body.get("password", "")),
        )
        if user is None:
            raise HTTPException(status_code=401, detail="invalid credentials")
        issue_session_cookie(request, response, user.user_id)
        return {"user": _user_payload(user)}


@router.post("/logout")
def logout(request: Request, response: Response):
    clear_session_cookie(request, response)
    return {"ok": True}


@router.get("/me")
def me(request: Request):
    with auth_session_scope(request) as session:
        user = get_current_user(request, session)
        if user is None:
            return {"authenticated": False, "user": None}
        return {"authenticated": True, "user": _user_payload(user)}


@router.post("/verify-email")
def verify_email(request: Request, body: dict):
    code = str(body.get("code", "")).strip()
    if not code:
        raise HTTPException(status_code=400, detail="verification code is required")
    with auth_session_scope(request) as session:
        repository = build_email_verification_repository(session, request)
        ticket = repository.get_by_code(code, "verify_email")
        if ticket is None:
            raise HTTPException(status_code=404, detail="verification code not found")
        if ticket.is_expired(datetime.now(UTC)):
            raise HTTPException(status_code=400, detail="verification code expired")
        service = build_auth_service(session, request)
        user = service.mark_email_verified(ticket.user_id, datetime.now(UTC))
        repository.save(ticket.mark_consumed(datetime.now(UTC)))
        return {"user": _user_payload(user)}


@router.post("/resend-verification")
def resend_verification(request: Request, body: dict):
    login = str(body.get("login", "")).strip()
    password = str(body.get("password", ""))
    with auth_session_scope(request) as session:
        service = build_auth_service(session, request)
        user = service.authenticate_user(login=login, password=password) if password else None
        if user is None and not password:
            user = get_current_user(request, session)
        if user is None:
            raise HTTPException(status_code=401, detail="authentication required")
        ticket = create_email_ticket(user.user_id, user.email, "verify_email", request, session)
        return {"verification_code": ticket.code}


@router.post("/forgot-password")
def forgot_password(request: Request, body: dict):
    login = str(body.get("login", "")).strip()
    if not login:
        raise HTTPException(status_code=400, detail="login is required")
    with auth_session_scope(request) as session:
        repository = build_auth_service(session, request).user_repository
        user = repository.get_by_login(login)
        if user is None:
            raise HTTPException(status_code=404, detail="user not found")
        create_email_ticket(user.user_id, user.email, "reset_password", request, session)
        return {"ok": True}


@router.post("/reset-password")
def reset_password(request: Request, response: Response, body: dict):
    code = str(body.get("code", "")).strip()
    new_password = str(body.get("password", ""))
    if not code or not new_password:
        raise HTTPException(status_code=400, detail="code and password are required")
    with auth_session_scope(request) as session:
        repository = build_email_verification_repository(session, request)
        ticket = repository.get_by_code(code, "reset_password")
        if ticket is None:
            raise HTTPException(status_code=404, detail="reset code not found")
        if ticket.is_expired(datetime.now(UTC)):
            raise HTTPException(status_code=400, detail="reset code expired")
        service = build_auth_service(session, request)
        user = service.reset_password(ticket.user_id, new_password)
        repository.save(ticket.mark_consumed(datetime.now(UTC)))
        issue_session_cookie(request, response, user.user_id)
        return {"user": _user_payload(user)}
