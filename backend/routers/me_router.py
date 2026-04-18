from __future__ import annotations

from datetime import UTC, datetime

from fastapi import APIRouter, HTTPException, Request

from app.modules.platform.application.services import JobApplicationService, JobQueryService, ServerExecutionService, UserCenterService
from app.modules.platform.contracts.job_commands import CreateJobCommand, StartJobCommand
from app.modules.platform.contracts.server_execution import UpdateServerPreferenceCommand

from ._auth_support import auth_session_scope, build_auth_service, require_current_user
from ._platform_execution_support import build_execution_orchestrator_service

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


def _build_job_application_service(session, request: Request) -> JobApplicationService:
    container = request.app.state.container
    job_repository = container.resolve_singleton("platform.job_repository_factory")(session)
    job_event_repository = container.resolve_singleton("platform.job_event_repository_factory")(session)
    return container.resolve_singleton("platform.job_application_service_factory")(
        job_repository=job_repository,
        job_event_repository=job_event_repository,
        execution_orchestrator_service=build_execution_orchestrator_service(session, request),
    )


def _build_server_execution_service(session, request: Request) -> ServerExecutionService:
    container = request.app.state.container
    repository = container.resolve_singleton("platform.server_execution_repository_factory")(session)
    return container.resolve_singleton("platform.server_execution_service_factory")(
        server_execution_repository=repository,
    )


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


def _require_platform_access(user) -> None:
    if not user.can_use_platform():
        raise HTTPException(status_code=403, detail="email verification required")


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


@router.post("/jobs")
def create_job(request: Request, body: dict):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        _require_platform_access(user)
        server_execution_service = _build_server_execution_service(session, request)
        service = _build_job_application_service(session, request)
        try:
            command = _enrich_job_command_with_default_server_profile(
                CreateJobCommand.model_validate(body),
                user_id=user.user_id,
                server_execution_service=server_execution_service,
            )
            job = service.create_job(user.user_id, command)
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


@router.get("/jobs/{job_id}")
def get_job_detail(request: Request, job_id: int):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        service = _build_user_center_service(session, request)
        detail = service.get_job_detail(user.user_id, job_id)
        if detail is None:
            raise HTTPException(status_code=404, detail="job not found")
        return detail.model_dump(exclude_none=True)


@router.post("/jobs/{job_id}/start")
def start_job(request: Request, job_id: int, body: dict):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        _require_platform_access(user)
        service = _build_job_application_service(session, request)
        try:
            job = service.start_job(
                user.user_id,
                StartJobCommand(job_id=job_id, triggered_by=body.get("triggered_by", "user")),
            )
        except LookupError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        if job is None:
            raise HTTPException(status_code=404, detail="job not found")
        return {"id": job.id, "status": job.status.value}


@router.get("/jobs/{job_id}/items")
def list_job_items(request: Request, job_id: int):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        service = _build_user_center_service(session, request)
        return [item.model_dump() for item in service.list_job_items(user.user_id, job_id)]


@router.get("/jobs/{job_id}/events")
def list_job_events(request: Request, job_id: int, after_id: int | None = None, limit: int = 50):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        service = _build_user_center_service(session, request)
        return [item.as_user_payload() for item in service.list_events(user.user_id, job_id, after_id=after_id, limit=limit)]


@router.get("/server-preferences")
def get_server_preferences(request: Request):
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        _require_platform_access(user)
        service = _build_server_execution_service(session, request)
        return service.get_user_server_preference(user.user_id).model_dump()


@router.put("/server-preferences")
def update_server_preferences(request: Request, body: dict):
    command = UpdateServerPreferenceCommand.model_validate(body)
    with auth_session_scope(request) as session:
        user = require_current_user(request, session)
        _require_platform_access(user)
        service = _build_server_execution_service(session, request)
        try:
            view = service.set_user_server_preference(
                user.user_id,
                command.default_execution_profile_id,
            )
        except LookupError as error:
            raise HTTPException(status_code=404, detail=str(error)) from error
        except ValueError as error:
            raise HTTPException(status_code=400, detail=str(error)) from error
        return view.model_dump()
