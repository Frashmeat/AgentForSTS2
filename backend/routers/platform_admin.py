from __future__ import annotations

from contextlib import contextmanager

from fastapi import APIRouter, HTTPException, Request

from app.modules.platform.application.services import AdminQueryService
from ._auth_support import auth_session_scope, require_admin_user

router = APIRouter(prefix="/admin")


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
    finally:
        session.close()


def _build_admin_query_service(session, request: Request) -> AdminQueryService:
    container = _container(request)
    repositories = container.resolve_singleton("platform.admin_query_repositories_factory")(session)
    return container.resolve_singleton("platform.admin_query_service_factory")(
        admin_query_repositories=repositories,
    )


@router.get("/jobs/{job_id}/executions")
def list_job_executions(request: Request, job_id: int):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        return [item.model_dump() for item in service.list_executions(job_id=job_id)]


@router.get("/executions/{execution_id}")
def get_execution_detail(request: Request, execution_id: int):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        detail = service.get_execution_detail(execution_id)
        if detail is None:
            raise HTTPException(status_code=404, detail="execution not found")
        return detail.model_dump()


@router.get("/quota/refunds")
def list_refunds(request: Request, user_id: int | None = None):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        return [item.model_dump() for item in service.list_refunds(user_id=user_id)]


@router.get("/audit/events")
def list_audit_events(request: Request, job_id: int | None = None):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        return [item.model_dump() for item in service.list_audit_events(job_id=job_id)]


@router.get("/platform/server-credentials")
def list_server_credentials(request: Request, execution_profile_id: int | None = None):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        return {"items": [item.model_dump() for item in service.list_server_credentials(execution_profile_id=execution_profile_id)]}
