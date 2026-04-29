from __future__ import annotations

import os
import secrets
import threading

from fastapi import APIRouter, Header, HTTPException, Request

from app.modules.platform.contracts.workstation_execution import (
    WorkstationExecutionDispatchAccepted,
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionPollResult,
)
from app.shared.infra.config.settings import Settings
from config import get_config

router = APIRouter(prefix="/workstation/platform")
_CONTROL_TOKEN_HEADER = "X-ATS-Workstation-Token"


class WorkstationExecutionStore:
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._results: dict[str, WorkstationExecutionPollResult] = {}

    def submit(self, request: WorkstationExecutionDispatchRequest) -> WorkstationExecutionDispatchAccepted:
        workstation_execution_id = f"ws-exec-{request.execution_id}"
        result = WorkstationExecutionPollResult(
            workstation_execution_id=workstation_execution_id,
            status="accepted",
            step_id="",
        )
        with self._lock:
            self._results[workstation_execution_id] = result
        return WorkstationExecutionDispatchAccepted(
            workstation_execution_id=workstation_execution_id,
            poll_url=f"/api/workstation/platform/executions/{workstation_execution_id}",
        )

    def get(self, workstation_execution_id: str) -> WorkstationExecutionPollResult | None:
        with self._lock:
            return self._results.get(workstation_execution_id)


def _settings(request: Request) -> Settings:
    container = getattr(request.app.state, "container", None)
    if container is not None:
        settings = container.resolve_optional_singleton("settings")
        if isinstance(settings, Settings):
            return settings
    return Settings.from_dict(get_config())


def _expected_control_token(request: Request) -> str:
    token_env = str(_settings(request).platform_execution.get("control_token_env", "")).strip()
    if not token_env:
        raise HTTPException(status_code=503, detail="workstation control token env is not configured")
    token = os.environ.get(token_env, "").strip()
    if not token:
        raise HTTPException(status_code=503, detail="workstation control token is not configured")
    return token


def _require_control_token(
    request: Request,
    x_ats_workstation_token: str = Header(default="", alias=_CONTROL_TOKEN_HEADER),
) -> None:
    if not x_ats_workstation_token:
        raise HTTPException(status_code=401, detail="workstation control token required")
    expected = _expected_control_token(request)
    if not secrets.compare_digest(x_ats_workstation_token, expected):
        raise HTTPException(status_code=403, detail="invalid workstation control token")


def _store(request: Request) -> WorkstationExecutionStore:
    existing = getattr(request.app.state, "workstation_execution_store", None)
    if isinstance(existing, WorkstationExecutionStore):
        return existing
    store = WorkstationExecutionStore()
    request.app.state.workstation_execution_store = store
    return store


@router.post("/executions")
def dispatch_execution(
    request: Request,
    body: dict,
    x_ats_workstation_token: str = Header(default="", alias=_CONTROL_TOKEN_HEADER),
):
    _require_control_token(request, x_ats_workstation_token)
    dispatch_request = WorkstationExecutionDispatchRequest.model_validate(body)
    return _store(request).submit(dispatch_request).model_dump()


@router.get("/executions/{workstation_execution_id}")
def poll_execution(
    request: Request,
    workstation_execution_id: str,
    x_ats_workstation_token: str = Header(default="", alias=_CONTROL_TOKEN_HEADER),
):
    _require_control_token(request, x_ats_workstation_token)
    result = _store(request).get(workstation_execution_id)
    if result is None:
        raise HTTPException(status_code=404, detail="workstation execution not found")
    return result.model_dump()
