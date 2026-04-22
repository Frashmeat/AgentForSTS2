from __future__ import annotations

from datetime import UTC, datetime

from fastapi import APIRouter, HTTPException, Request

from app.modules.platform.application.services import (
    JobApplicationService,
    JobQueryService,
    PlatformRequestRateLimitExceededError,
    PlatformRequestRateLimiter,
    ServerQueuedJobClaimService,
    ServerExecutionService,
)
from app.modules.platform.application.services.server_workspace_service import ServerWorkspaceService
from app.modules.platform.application.services.uploaded_asset_service import UploadedAssetService
from app.modules.platform.contracts.job_commands import CancelJobCommand, CreateJobCommand, StartJobCommand
from app.modules.platform.contracts.uploaded_asset import UploadAssetCommand

from ._auth_support import auth_session_scope, require_current_user
from ._platform_execution_support import build_execution_orchestrator_service

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
        execution_orchestrator_service=build_execution_orchestrator_service(session, request),
        server_queued_job_claim_service=_build_server_queued_job_claim_service(request),
        server_workspace_service=_build_server_workspace_service(request),
        uploaded_asset_service=_build_uploaded_asset_service(request),
    )


def _build_job_query_service(session, request: Request) -> JobQueryService:
    container = _container(request)
    job_query_repository = container.resolve_singleton("platform.job_query_repository_factory")(session)
    quota_query_repository = container.resolve_singleton("platform.quota_query_repository_factory")(session)
    return container.resolve_singleton("platform.job_query_service_factory")(
        job_query_repository=job_query_repository,
        quota_query_repository=quota_query_repository,
    )


def _build_server_execution_service(session, request: Request) -> ServerExecutionService:
    container = _container(request)
    repository = container.resolve_singleton("platform.server_execution_repository_factory")(session)
    return container.resolve_singleton("platform.server_execution_service_factory")(
        server_execution_repository=repository,
    )


def _build_uploaded_asset_service(request: Request) -> UploadedAssetService:
    container = _container(request)
    factory = container.resolve_singleton("platform.uploaded_asset_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_server_workspace_service(request: Request) -> ServerWorkspaceService:
    container = _container(request)
    factory = container.resolve_singleton("platform.server_workspace_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_server_queued_job_claim_service(request: Request) -> ServerQueuedJobClaimService:
    container = _container(request)
    factory = container.resolve_singleton("platform.server_queued_job_claim_service_factory")
    if callable(factory):
        return factory()
    return factory


def _build_platform_request_rate_limiter(request: Request) -> PlatformRequestRateLimiter:
    container = _container(request)
    return container.resolve_singleton("platform.request_rate_limiter")


def _enrich_job_command_with_default_server_profile(
    command: CreateJobCommand,
    *,
    user_id: int,
    server_execution_service: ServerExecutionService,
) -> CreateJobCommand:
    if command.selected_execution_profile_id is not None:
        return command

    preference = server_execution_service.get_user_server_preference(user_id)
    if preference.default_execution_profile_id is None or not preference.available:
        return command

    payload = command.model_dump()
    payload["selected_execution_profile_id"] = preference.default_execution_profile_id
    payload["selected_agent_backend"] = preference.agent_backend
    payload["selected_model"] = preference.model
    return CreateJobCommand.model_validate(payload)


def _require_platform_user(request: Request, session):
    user = require_current_user(request, session)
    if not user.can_use_platform():
        raise HTTPException(status_code=403, detail="email verification required")
    return user


@router.post("/jobs")
def create_job(request: Request, body: dict):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        server_execution_service = _build_server_execution_service(session, request)
        service = _build_job_application_service(session, request)
        rate_limiter = _build_platform_request_rate_limiter(request)
        try:
            rate_limiter.check_and_record(user_id=user.user_id, action="create_job")
            command = _enrich_job_command_with_default_server_profile(
                CreateJobCommand.model_validate(body),
                user_id=user.user_id,
                server_execution_service=server_execution_service,
            )
            job = service.create_job(user_id=user.user_id, command=command)
        except PlatformRequestRateLimitExceededError as error:
            raise HTTPException(status_code=429, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        return {
            "id": job.id,
            "job_type": job.job_type,
            "status": job.status.value,
            "workflow_version": job.workflow_version,
            "selected_execution_profile_id": job.selected_execution_profile_id,
            "selected_agent_backend": job.selected_agent_backend,
            "selected_model": job.selected_model,
        }


@router.post("/jobs/{job_id}/start")
def start_job(request: Request, job_id: int, body: dict):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        service = _build_job_application_service(session, request)
        rate_limiter = _build_platform_request_rate_limiter(request)
        try:
            rate_limiter.check_and_record(user_id=user.user_id, action="start_job")
            job = service.start_job(
                user_id=user.user_id,
                command=StartJobCommand(job_id=job_id, triggered_by=body.get("triggered_by", "user")),
            )
        except PlatformRequestRateLimitExceededError as error:
            raise HTTPException(status_code=429, detail=str(error)) from error
        except LookupError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
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


@router.get("/execution-profiles")
def list_execution_profiles(request: Request):
    with auth_session_scope(request) as session:
        _require_platform_user(request, session)
        service = _build_server_execution_service(session, request)
        return service.list_execution_profiles().model_dump()


@router.post("/upload-assets")
def upload_asset(request: Request, body: dict):
    with auth_session_scope(request) as session:
        user = _require_platform_user(request, session)
        command = UploadAssetCommand.model_validate(body)
        service = _build_uploaded_asset_service(request)
        try:
            uploaded = service.create_asset(
                user_id=user.user_id,
                file_name=command.file_name,
                content_base64=command.content_base64,
                mime_type=command.mime_type,
            )
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        return uploaded.model_dump()
