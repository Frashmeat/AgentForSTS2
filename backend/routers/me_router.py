from __future__ import annotations

from fastapi import APIRouter, Request

from ._auth_support import auth_session_scope, require_current_user

router = APIRouter(prefix="/me")


@router.get("/profile")
def get_profile(request: Request):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        return {
            "user_id": user.user_id,
            "username": user.username,
            "email": user.email,
            "email_verified": user.email_verified,
            "created_at": user.created_at.isoformat(),
            "email_verified_at": user.email_verified_at.isoformat() if user.email_verified_at else None,
        }
