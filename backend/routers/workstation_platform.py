from __future__ import annotations

import os
import secrets
import threading

from fastapi import APIRouter, Header, HTTPException, Request

from app.modules.platform.application.workstation_platform_executor import (
    WorkstationPlatformExecutor,
    build_default_workstation_platform_executor,
)
from app.modules.platform.contracts.workstation_execution import (
    WorkstationExecutionDispatchAccepted,
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionEvent,
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
        self._text_semaphore: threading.Semaphore | None = None
        self._code_semaphore: threading.Semaphore | None = None
        self._workspace_locks: dict[str, threading.Lock] = {}

    def submit(
        self,
        request: WorkstationExecutionDispatchRequest,
        executor: WorkstationPlatformExecutor,
        settings: Settings,
    ) -> WorkstationExecutionDispatchAccepted:
        workstation_execution_id = f"ws-exec-{request.execution_id}"
        result = WorkstationExecutionPollResult(
            workstation_execution_id=workstation_execution_id,
            status="accepted",
            step_id="",
        )
        with self._lock:
            self._results[workstation_execution_id] = result
        thread = threading.Thread(
            target=self._execute,
            args=(request, executor, settings),
            name=f"workstation-platform-{workstation_execution_id}",
            daemon=True,
        )
        thread.start()
        return WorkstationExecutionDispatchAccepted(
            workstation_execution_id=workstation_execution_id,
            poll_url=f"/api/workstation/platform/executions/{workstation_execution_id}",
        )

    def get(self, workstation_execution_id: str) -> WorkstationExecutionPollResult | None:
        with self._lock:
            return self._results.get(workstation_execution_id)

    def _execute(
        self,
        request: WorkstationExecutionDispatchRequest,
        executor: WorkstationPlatformExecutor,
        settings: Settings,
    ) -> None:
        workstation_execution_id = f"ws-exec-{request.execution_id}"
        with self._lock:
            self._results[workstation_execution_id] = WorkstationExecutionPollResult(
                workstation_execution_id=workstation_execution_id,
                status="running",
                step_id="workflow.dispatch",
            )
        event_sink = self._event_sink(workstation_execution_id)
        semaphore = self._semaphore_for(request, settings)
        workspace_lock = self._workspace_lock_for(request)
        with semaphore:
            if workspace_lock is None:
                result = executor.execute(request, event_sink=event_sink)
            else:
                with workspace_lock:
                    result = executor.execute(request, event_sink=event_sink)
        with self._lock:
            current = self._results.get(workstation_execution_id)
            if current is not None and current.events:
                merged_events = _merge_events_by_sequence(current.events, result.events)
                result = WorkstationExecutionPollResult(
                    workstation_execution_id=result.workstation_execution_id,
                    status=result.status,
                    step_id=result.step_id,
                    output_payload=result.output_payload,
                    error_summary=result.error_summary,
                    error_payload=result.error_payload,
                    events=merged_events,
                )
            self._results[workstation_execution_id] = result

    def _event_sink(self, workstation_execution_id: str):
        def sink(event: WorkstationExecutionEvent) -> None:
            with self._lock:
                current = self._results.get(workstation_execution_id)
                if current is None:
                    return
                events = [*current.events, event]
                current_step_id = str(event.payload.get("step_id") or current.step_id)
                self._results[workstation_execution_id] = WorkstationExecutionPollResult(
                    workstation_execution_id=current.workstation_execution_id,
                    status=current.status,
                    step_id=current_step_id,
                    output_payload=current.output_payload,
                    error_summary=current.error_summary,
                    error_payload=current.error_payload,
                    events=events,
                )

        return sink

    def _semaphore_for(
        self,
        request: WorkstationExecutionDispatchRequest,
        settings: Settings,
    ) -> threading.Semaphore:
        platform_execution = settings.platform_execution
        if _is_code_execution(request):
            with self._lock:
                if self._code_semaphore is None:
                    self._code_semaphore = threading.Semaphore(
                        max(1, int(platform_execution.get("max_concurrent_code", 2)))
                    )
                return self._code_semaphore
        with self._lock:
            if self._text_semaphore is None:
                self._text_semaphore = threading.Semaphore(
                    max(1, int(platform_execution.get("max_concurrent_text", 2)))
                )
            return self._text_semaphore

    def _workspace_lock_for(self, request: WorkstationExecutionDispatchRequest) -> threading.Lock | None:
        server_project_ref = str(request.input_payload.get("server_project_ref", "")).strip()
        if not server_project_ref:
            return None
        with self._lock:
            if server_project_ref not in self._workspace_locks:
                self._workspace_locks[server_project_ref] = threading.Lock()
            return self._workspace_locks[server_project_ref]


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


def _executor(request: Request) -> WorkstationPlatformExecutor:
    existing = getattr(request.app.state, "workstation_platform_executor", None)
    if existing is not None and callable(getattr(existing, "execute", None)):
        return existing
    container = getattr(request.app.state, "container", None)
    if container is None:
        raise HTTPException(status_code=503, detail="workstation container is not configured")
    executor = build_default_workstation_platform_executor(container)
    request.app.state.workstation_platform_executor = executor
    return executor


def _is_code_execution(request: WorkstationExecutionDispatchRequest) -> bool:
    if request.item_type == "custom_code":
        return True
    return any(
        str(request.input_payload.get(key, "")).strip()
        for key in ("server_project_ref", "server_workspace_root", "deploy_target")
    )


def _merge_events_by_sequence(
    left: list[WorkstationExecutionEvent],
    right: list[WorkstationExecutionEvent],
) -> list[WorkstationExecutionEvent]:
    merged: dict[int, WorkstationExecutionEvent] = {}
    for event in [*left, *right]:
        merged[event.sequence] = event
    return [merged[sequence] for sequence in sorted(merged)]


@router.post("/executions")
def dispatch_execution(
    request: Request,
    body: dict,
    x_ats_workstation_token: str = Header(default="", alias=_CONTROL_TOKEN_HEADER),
):
    _require_control_token(request, x_ats_workstation_token)
    dispatch_request = WorkstationExecutionDispatchRequest.model_validate(body)
    return _store(request).submit(dispatch_request, _executor(request), _settings(request)).model_dump()


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
