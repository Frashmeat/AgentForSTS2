from __future__ import annotations

import zipfile
from contextlib import contextmanager
from datetime import UTC, datetime

from fastapi import APIRouter, File, Form, HTTPException, Query, Request, UploadFile

from app.modules.knowledge.infra import knowledge_runtime
from app.modules.platform.application.services import (
    AdminQueryService,
    AdminQuotaCommandService,
    PlatformRuntimeAuditService,
    ServerCredentialAdminService,
)
from app.modules.platform.contracts import AdjustUserQuotaCommand, CreateServerCredentialCommand, UpdateServerCredentialCommand
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


def _build_admin_quota_command_service(session, request: Request) -> AdminQuotaCommandService:
    container = _container(request)
    return container.resolve_singleton("platform.admin_quota_command_service_factory")(
        quota_account_repository=container.resolve_singleton("platform.quota_account_repository_factory")(session),
        quota_balance_repository=container.resolve_singleton("platform.quota_balance_repository_factory")(session),
        usage_ledger_repository=container.resolve_singleton("platform.usage_ledger_repository_factory")(session),
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


@router.get("/users")
def list_users(
    request: Request,
    query: str = "",
    email_verified: bool | None = None,
    is_admin: bool | None = None,
    quota_status: str = "",
    anomaly: str = "",
    limit: int = 50,
    offset: int = 0,
):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        return service.list_users(
            query=query,
            email_verified=email_verified,
            is_admin=is_admin,
            quota_status=quota_status,
            anomaly=anomaly,
            limit=limit,
            offset=offset,
        ).model_dump()


@router.get("/users/{user_id}")
def get_user_detail(request: Request, user_id: int):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        detail = service.get_user_detail(user_id)
        if detail is None:
            raise HTTPException(status_code=404, detail="user not found")
        return detail.model_dump()


@router.get("/users/{user_id}/quota")
def get_user_quota(request: Request, user_id: int):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        detail = service.get_user_detail(user_id)
        if detail is None:
            raise HTTPException(status_code=404, detail="user not found")
        return detail.quota.model_dump()


@router.get("/users/{user_id}/quota/ledger")
def list_user_quota_ledger(request: Request, user_id: int, after_id: int | None = None, limit: int = 50):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        service = _build_admin_query_service(session, request)
        if service.get_user_detail(user_id) is None:
            raise HTTPException(status_code=404, detail="user not found")
        return service.list_user_quota_ledgers(user_id, after_id=after_id, limit=limit).model_dump()


@router.post("/users/{user_id}/quota/adjust")
def adjust_user_quota(request: Request, user_id: int, body: dict):
    try:
        command = AdjustUserQuotaCommand.model_validate(body)
    except KeyError as error:
        message = error.args[0] if error.args else "invalid request body"
        raise HTTPException(status_code=400, detail=str(message)) from error

    with auth_session_scope(request) as auth_session:
        admin_user = require_admin_user(request, auth_session)
    with _session_scope(request) as session:
        query_service = _build_admin_query_service(session, request)
        if query_service.get_user_detail(user_id) is None:
            raise HTTPException(status_code=404, detail="user not found")
        service = _build_admin_quota_command_service(session, request)
        try:
            view = service.adjust_user_quota(user_id, command, admin_user_id=admin_user.user_id, now=datetime.now(UTC))
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        _build_runtime_audit_service(request).append_event(
            event_type="admin.quota.adjusted",
            payload={
                "admin_user_id": admin_user.user_id,
                "target_user_id": user_id,
                "direction": command.direction,
                "amount": command.amount,
                "reason": command.reason,
            },
        )
        return view.model_dump()


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


@router.get("/platform/workstation-runtime-status")
def get_workstation_runtime_status(request: Request):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    manager = getattr(request.app.state, "workstation_runtime_manager", None)
    if manager is None:
        return {"available": False, "reason": "workstation_runtime_manager_not_registered"}
    return manager.get_runtime_status().model_dump()


@router.get("/platform/workstation-runtime-logs")
def get_workstation_runtime_logs(
    request: Request,
    stream: str = Query(default="stderr"),
    tail_bytes: int = Query(default=65_536, ge=1, le=262_144),
):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    manager = getattr(request.app.state, "workstation_runtime_manager", None)
    if manager is None:
        raise HTTPException(status_code=503, detail="workstation runtime manager is not registered")
    try:
        return manager.read_runtime_log_tail(stream=stream, tail_bytes=tail_bytes).model_dump()
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error


@router.get("/platform/knowledge-packs")
def list_knowledge_packs(request: Request):
    with auth_session_scope(request) as auth_session:
        require_admin_user(request, auth_session)
    return knowledge_runtime.list_knowledge_packs()


@router.post("/platform/knowledge-packs")
async def upload_knowledge_pack(
    request: Request,
    file: UploadFile = File(...),
    label: str = Form(default=""),
):
    with auth_session_scope(request) as auth_session:
        admin_user = require_admin_user(request, auth_session)
    content = await file.read()
    if not content:
        raise HTTPException(status_code=400, detail="knowledge pack file is empty")
    try:
        pack = knowledge_runtime.upload_knowledge_pack_zip(
            content,
            file_name=file.filename or "",
            label=label,
        )
    except zipfile.BadZipFile as error:
        raise HTTPException(status_code=400, detail="knowledge pack must be a zip file") from error
    except ValueError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    _build_runtime_audit_service(request).append_event(
        event_type="admin.knowledge_pack.uploaded",
        payload={
            "admin_user_id": admin_user.user_id,
            "pack_id": pack["pack_id"],
            "label": pack["label"],
            "file_count": pack["file_count"],
        },
    )
    return pack


@router.post("/platform/knowledge-packs/rollback")
def rollback_knowledge_pack(request: Request):
    with auth_session_scope(request) as auth_session:
        admin_user = require_admin_user(request, auth_session)
    result = knowledge_runtime.rollback_knowledge_pack()
    _build_runtime_audit_service(request).append_event(
        event_type="admin.knowledge_pack.rolled_back",
        payload={"admin_user_id": admin_user.user_id, "active_pack_id": result.get("active_pack_id", "")},
    )
    return result


@router.post("/platform/knowledge-packs/{pack_id}/activate")
def activate_knowledge_pack(request: Request, pack_id: str):
    with auth_session_scope(request) as auth_session:
        admin_user = require_admin_user(request, auth_session)
    try:
        pack = knowledge_runtime.activate_knowledge_pack(pack_id)
    except (KeyError, ValueError) as error:
        raise HTTPException(status_code=404, detail="knowledge pack not found") from error
    _build_runtime_audit_service(request).append_event(
        event_type="admin.knowledge_pack.activated",
        payload={"admin_user_id": admin_user.user_id, "pack_id": pack["pack_id"], "label": pack.get("label", "")},
    )
    return pack


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
