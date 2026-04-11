from __future__ import annotations

from datetime import UTC, datetime

from fastapi import APIRouter, HTTPException, Request

from app.modules.platform.application.services import JobApplicationService, JobQueryService
from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand

from ._auth_support import auth_session_scope, require_current_user

router = APIRouter(prefix="/platform")


def _container(request: Request):
    return request.app.state.container


def _build_job_application_service(session, request: Request) -> JobApplicationService:
    container = _container(request)
    job_repository = container.resolve_singleton("platform.job_repository_factory")(session)
    job_event_repository = container.resolve_singleton("platform.job_event_repository_factory")(session)
    return container.resolve_singleton("platform.job_application_service_factory")(
        job_repository=job_repository,
        job_event_repository=job_event_repository,
    )


def _build_job_query_service(session, request: Request) -> JobQueryService:
    container = _container(request)
    job_query_repository = container.resolve_singleton("platform.job_query_repository_factory")(session)
    quota_query_repository = container.resolve_singleton("platform.quota_query_repository_factory")(session)
    return container.resolve_singleton("platform.job_query_service_factory")(
        job_query_repository=job_query_repository,
        quota_query_repository=quota_query_repository,
    )


def _require_platform_user(request: Request, session):
    user = require_current_user(request, session)
    if not user.can_use_platform():
        raise HTTPException(status_code=403, detail="email verification required")
    return user


@router.post("/jobs")
def create_job(request: Request, body: dict):
    command = CreateJobCommand.model_validate(body)
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_application_service(session, request)
        job = service.create_job(user_id=user.user_id, command=command)
        return {
            "id": job.id,
            "job_type": job.job_type,
            "status": job.status.value,
            "workflow_version": job.workflow_version,
        }


@router.post("/jobs/{job_id}/start")
def start_job(request: Request, job_id: int, body: dict):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_application_service(session, request)
        job = service.start_job(
            user_id=user.user_id,
            command=StartJobCommand(job_id=job_id, triggered_by=body.get("triggered_by", "user")),
        )
        if job is None:
            raise HTTPException(status_code=404, detail="job not found")
        return {"id": job.id, "status": job.status.value}


@router.post("/jobs/{job_id}/cancel")
def cancel_job(request: Request, job_id: int, body: dict):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_application_service(session, request)
        ok = service.cancel_job(
            user_id=user.user_id,
            command=CancelJobCommand(job_id=job_id, reason=body.get("reason", "")),
        )
        return {"ok": ok}


@router.get("/jobs")
def list_jobs(request: Request):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_query_service(session, request)
        return [item.model_dump() for item in service.list_jobs(user.user_id)]


@router.get("/jobs/{job_id}")
def get_job_detail(request: Request, job_id: int):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_query_service(session, request)
        detail = service.get_job_detail(user.user_id, job_id)
        if detail is None:
            raise HTTPException(status_code=404, detail="job not found")
        return detail.model_dump()


@router.get("/jobs/{job_id}/items")
def list_job_items(request: Request, job_id: int):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_query_service(session, request)
        return [item.model_dump() for item in service.list_job_items(user.user_id, job_id)]


@router.get("/jobs/{job_id}/events")
def list_job_events(request: Request, job_id: int, after_id: int | None = None, limit: int = 50):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_query_service(session, request)
        return [item.model_dump() for item in service.list_events(user.user_id, job_id, after_id=after_id, limit=limit)]


@router.get("/quota")
def get_quota(request: Request):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_query_service(session, request)
        view = service.get_quota_view(user.user_id, datetime.now(UTC))
        if view is None:
            raise HTTPException(status_code=404, detail="quota not found")
        return view.model_dump()
