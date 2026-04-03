from __future__ import annotations

from contextlib import contextmanager

from fastapi import APIRouter, HTTPException, Request

from app.modules.platform.application.services import JobApplicationService, JobQueryService
from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand

router = APIRouter(prefix="/platform")


def _container(request: Request):
    return request.app.state.container


@contextmanager
def _session_scope(request: Request):
    session_factory = _container(request).resolve_optional_singleton("platform.db_session_factory")
    if session_factory is None:
        raise HTTPException(status_code=503, detail="platform database session is not configured")
    session = session_factory()
    try:
        yield session
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()


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


@router.post("/jobs")
def create_job(request: Request, user_id: int, body: dict):
    command = CreateJobCommand.model_validate(body)
    with _session_scope(request) as session:
        service = _build_job_application_service(session, request)
        job = service.create_job(user_id=user_id, command=command)
        return {
            "id": job.id,
            "job_type": job.job_type,
            "status": job.status.value,
            "workflow_version": job.workflow_version,
        }


@router.post("/jobs/{job_id}/start")
def start_job(request: Request, job_id: int, user_id: int, body: dict):
    with _session_scope(request) as session:
        service = _build_job_application_service(session, request)
        job = service.start_job(user_id=user_id, command=StartJobCommand(job_id=job_id, triggered_by=body.get("triggered_by", "user")))
        if job is None:
            raise HTTPException(status_code=404, detail="job not found")
        return {"id": job.id, "status": job.status.value}


@router.post("/jobs/{job_id}/cancel")
def cancel_job(request: Request, job_id: int, user_id: int, body: dict):
    with _session_scope(request) as session:
        service = _build_job_application_service(session, request)
        ok = service.cancel_job(user_id=user_id, command=CancelJobCommand(job_id=job_id, reason=body.get("reason", "")))
        return {"ok": ok}


@router.get("/jobs")
def list_jobs(request: Request, user_id: int):
    with _session_scope(request) as session:
        service = _build_job_query_service(session, request)
        return [item.model_dump() for item in service.list_jobs(user_id)]


@router.get("/jobs/{job_id}")
def get_job_detail(request: Request, job_id: int, user_id: int):
    with _session_scope(request) as session:
        service = _build_job_query_service(session, request)
        detail = service.get_job_detail(user_id, job_id)
        if detail is None:
            raise HTTPException(status_code=404, detail="job not found")
        return detail.model_dump()


@router.get("/jobs/{job_id}/items")
def list_job_items(request: Request, job_id: int, user_id: int):
    with _session_scope(request) as session:
        service = _build_job_query_service(session, request)
        return [item.model_dump() for item in service.list_job_items(user_id, job_id)]


@router.get("/jobs/{job_id}/events")
def list_job_events(request: Request, job_id: int, user_id: int, after_id: int | None = None, limit: int = 50):
    with _session_scope(request) as session:
        service = _build_job_query_service(session, request)
        return [item.model_dump() for item in service.list_events(user_id, job_id, after_id=after_id, limit=limit)]


@router.get("/quota")
def get_quota(request: Request, user_id: int):
    from datetime import UTC, datetime

    with _session_scope(request) as session:
        service = _build_job_query_service(session, request)
        view = service.get_quota_view(user_id, datetime.now(UTC))
        if view is None:
            raise HTTPException(status_code=404, detail="quota not found")
        return view.model_dump()
