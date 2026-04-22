from __future__ import annotations

from contextlib import contextmanager

from fastapi import APIRouter, HTTPException, Request

from app.modules.platform.application.services import AdminQueryService, PlatformRuntimeAuditService, ServerCredentialAdminService
from app.modules.platform.contracts import CreateServerCredentialCommand, UpdateServerCredentialCommand
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
        session.commit()
    except Exception:
        session.rollback()
        raise
    finally:
        session.close()


def _build_admin_query_service(session, request: Request) -> AdminQueryService:
    container = _container(request)
    repositories = container.resolve_singleton("platform.admin_query_repositories_factory")(session)
    runtime_audit_service = _build_runtime_audit_service(request)
    return container.resolve_singleton("platform.admin_query_service_factory")(
        admin_query_repositories=repositories,
        runtime_audit_service=runtime_audit_service,
    )


def _build_runtime_audit_service(request: Request) -> PlatformRuntimeAuditService:
    container = _container(request)
    factory = container.resolve_singleton("platform.runtime_audit_service_factory")
    session_factory = container.resolve_optional_singleton("platform.db_session_factory")
    if callable(factory):
        return factory(session_factory=session_factory)
    return factory


def _build_server_credential_admin_service(session, request: Request) -> ServerCredentialAdminService:
    container = _container(request)
    repository = container.resolve_singleton("platform.server_credential_admin_repository_factory")(session)
    cipher_factory = container.resolve_singleton("platform.server_credential_cipher_factory")
    health_checker_factory = container.resolve_singleton("platform.server_credential_health_checker_factory")
    settings = container.resolve_singleton("settings")
    try:
        cipher = cipher_factory.from_settings(settings)
    except (RuntimeError, ValueError) as error:
        raise HTTPException(status_code=503, detail=str(error)) from error
    return container.resolve_singleton("platform.server_credential_admin_service_factory")(
        server_credential_admin_repository=repository,
        server_credential_cipher=cipher,
        server_credential_health_checker=health_checker_factory(),
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
def list_audit_events(
    request: Request,
    job_id: int | None = None,
    after_id: int | None = None,
    limit: int = 50,
    event_type_prefix: str | None = None,
):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        return [
            item.model_dump()
            for item in service.list_audit_events(
                job_id=job_id,
                after_id=after_id,
                limit=limit,
                event_type_prefix=event_type_prefix,
            )
        ]


@router.get("/platform/server-credentials")
def list_server_credentials(request: Request, execution_profile_id: int | None = None):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        return {"items": [item.model_dump() for item in service.list_server_credentials(execution_profile_id=execution_profile_id)]}


@router.post("/platform/server-credentials")
def create_server_credential(request: Request, body: dict):
    try:
        command = CreateServerCredentialCommand.model_validate(body)
    except KeyError as error:
        message = error.args[0] if error.args else "invalid request body"
        raise HTTPException(status_code=400, detail=str(message)) from error

    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_server_credential_admin_service(session, request)
        try:
            item = service.create_server_credential(command)
        except LookupError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        return item.model_dump()


@router.put("/platform/server-credentials/{credential_id}")
def update_server_credential(request: Request, credential_id: int, body: dict):
    try:
        command = UpdateServerCredentialCommand.model_validate(body)
    except KeyError as error:
        message = error.args[0] if error.args else "invalid request body"
        raise HTTPException(status_code=400, detail=str(message)) from error

    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_server_credential_admin_service(session, request)
        try:
            item = service.update_server_credential(credential_id, command)
        except LookupError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        return item.model_dump()


@router.post("/platform/server-credentials/{credential_id}/enable")
def enable_server_credential(request: Request, credential_id: int):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_server_credential_admin_service(session, request)
        try:
            item = service.set_server_credential_enabled(credential_id, True)
        except LookupError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        return item.model_dump()


@router.post("/platform/server-credentials/{credential_id}/disable")
def disable_server_credential(request: Request, credential_id: int):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_server_credential_admin_service(session, request)
        try:
            item = service.set_server_credential_enabled(credential_id, False)
        except LookupError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        return item.model_dump()


@router.post("/platform/server-credentials/{credential_id}/health-check")
def run_server_credential_health_check(request: Request, credential_id: int):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_server_credential_admin_service(session, request)
        try:
            item = service.run_health_check(credential_id)
        except LookupError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        return item.model_dump()


@router.get("/platform/execution-profiles")
def list_execution_profiles(request: Request):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        return {"items": [item.model_dump() for item in service.list_execution_profiles()]}
