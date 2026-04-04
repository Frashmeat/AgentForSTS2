from __future__ import annotations

from datetime import UTC, datetime

from fastapi import APIRouter, HTTPException, Request

from app.modules.platform.application.services import JobQueryService, UserCenterService

from ._auth_support import auth_session_scope, build_auth_service, require_current_user

router = APIRouter(prefix="/me")


def _build_user_center_service(session, request: Request) -> UserCenterService:
    container = request.app.state.container
    job_query_repository = container.resolve_singleton("platform.job_query_repository_factory")(session)
    quota_query_repository = container.resolve_singleton("platform.quota_query_repository_factory")(session)
    job_query_service = JobQueryService(
        job_query_repository=job_query_repository,
        quota_query_repository=quota_query_repository,
    )
    auth_service = build_auth_service(session, request)
    return UserCenterService(
        auth_service=auth_service,
        job_query_service=job_query_service,
    )


@router.get("/profile")
def get_profile(request: Request):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        service = _build_user_center_service(session, request)
        return service.get_profile(user.user_id).model_dump(exclude_none=True)


@router.get("/quota")
def get_quota(request: Request):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        service = _build_user_center_service(session, request)
        quota = service.get_quota(user.user_id, datetime.now(UTC))
        if quota is None:
            raise HTTPException(status_code=404, detail="quota not found")
        return quota.model_dump(exclude_none=True)


@router.get("/jobs")
def list_jobs(request: Request):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        service = _build_user_center_service(session, request)
        return [item.model_dump() for item in service.list_jobs(user.user_id)]


@router.get("/jobs/{job_id}")
def get_job_detail(request: Request, job_id: int):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        service = _build_user_center_service(session, request)
        detail = service.get_job_detail(user.user_id, job_id)
        if detail is None:
            raise HTTPException(status_code=404, detail="job not found")
        return detail.model_dump(exclude_none=True)


@router.get("/jobs/{job_id}/items")
def list_job_items(request: Request, job_id: int):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        service = _build_user_center_service(session, request)
        return [item.model_dump() for item in service.list_job_items(user.user_id, job_id)]
