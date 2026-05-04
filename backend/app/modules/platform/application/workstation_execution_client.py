from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass
from typing import Protocol
from urllib.parse import urljoin

from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.contracts.workstation_execution import (
    WorkstationExecutionDispatchAccepted,
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionEvent,
    WorkstationExecutionPollResult,
)
from app.modules.platform.errors import PlatformExecutionError, build_platform_error
from app.shared.infra.config.settings import Settings


class WorkstationExecutionClientError(RuntimeError):
    def __init__(self, message: str, *, reason_code: str, category: str, retryable: bool = True) -> None:
        super().__init__(message)
        self.reason_code = reason_code
        self.category = category
        self.retryable = retryable


WorkstationEventHandler = Callable[[list[WorkstationExecutionEvent]], None]


class WorkstationRuntimeController(Protocol):
    def ensure_started(self): ...


@dataclass(slots=True)
class WorkstationExecutionClient:
    settings: Settings
    urlopen: Callable[..., object] = urllib.request.urlopen
    sleep: Callable[[float], None] = time.sleep
    monotonic: Callable[[], float] = time.monotonic
    runtime_controller: WorkstationRuntimeController | None = None

    def dispatch_and_poll(
        self,
        request: WorkstationExecutionDispatchRequest,
        on_events: WorkstationEventHandler | None = None,
    ) -> StepExecutionResult:
        accepted = self.dispatch(request)
        result = self.poll_until_finished(accepted.workstation_execution_id, on_events=on_events)
        return StepExecutionResult(
            step_id=result.step_id,
            status=result.status,
            output_payload=result.output_payload,
            error_summary=result.error_summary,
            error_payload=result.error_payload,
        )

    def dispatch(self, request: WorkstationExecutionDispatchRequest) -> WorkstationExecutionDispatchAccepted:
        self._ensure_runtime_ready()
        payload = self._request_json(
            "POST",
            "/api/workstation/platform/executions",
            body=request.model_dump(),
            timeout_seconds=int(self.config.get("dispatch_timeout_seconds", 10)),
        )
        return WorkstationExecutionDispatchAccepted.model_validate(payload)

    def poll_until_finished(
        self,
        workstation_execution_id: str,
        on_events: WorkstationEventHandler | None = None,
    ) -> WorkstationExecutionPollResult:
        timeout_seconds = float(self.config.get("execution_timeout_seconds", 180))
        poll_interval_seconds = float(self.config.get("poll_interval_seconds", 2))
        deadline = self.monotonic() + timeout_seconds
        last_event_sequence = 0

        while True:
            payload = self._request_json(
                "GET",
                f"/api/workstation/platform/executions/{workstation_execution_id}",
                timeout_seconds=int(self.config.get("dispatch_timeout_seconds", 10)),
            )
            result = WorkstationExecutionPollResult.model_validate(payload)
            new_events = [
                event
                for event in sorted(result.events, key=lambda item: item.sequence)
                if event.sequence > last_event_sequence
            ]
            if new_events:
                last_event_sequence = max(event.sequence for event in new_events)
                if on_events is not None:
                    on_events(new_events)
            if result.status not in {"accepted", "running"}:
                return result
            if self.monotonic() >= deadline:
                raise WorkstationExecutionClientError(
                    f"workstation execution timed out: {workstation_execution_id}",
                    reason_code="web_workstation_dispatch_timeout",
                    category="timeout",
                    retryable=True,
                )
            self.sleep(poll_interval_seconds)

    @property
    def config(self) -> dict[str, object]:
        return self.settings.platform_execution

    def _request_json(
        self,
        method: str,
        path: str,
        *,
        body: dict[str, object] | None = None,
        timeout_seconds: int,
    ) -> dict[str, object]:
        data = None if body is None else json.dumps(body).encode("utf-8")
        url = urljoin(str(self.config.get("workstation_url", "http://127.0.0.1:7860")), path)
        request = urllib.request.Request(
            url,
            data=data,
            method=method,
            headers={
                "Content-Type": "application/json",
                "X-ATS-Workstation-Token": self._control_token(),
            },
        )
        try:
            response = self.urlopen(request, timeout=timeout_seconds)
            with response:
                raw = response.read()
        except urllib.error.HTTPError as exc:
            if exc.code in {401, 403}:
                reason_code = "web_workstation_control_token_invalid"
                category = "auth_error"
                retryable = False
            else:
                reason_code = "web_workstation_request_failed"
                category = "network_error"
                retryable = True
            raise WorkstationExecutionClientError(
                f"workstation request failed: url={url} HTTP {exc.code}",
                reason_code=reason_code,
                category=category,
                retryable=retryable,
            ) from exc
        except OSError as exc:
            raise WorkstationExecutionClientError(
                f"workstation request failed: url={url} error={exc}",
                reason_code="web_workstation_unreachable",
                category="network_error",
                retryable=True,
            ) from exc

        try:
            decoded = json.loads(raw.decode("utf-8") if raw else "{}")
        except json.JSONDecodeError as exc:
            raise WorkstationExecutionClientError(
                f"workstation response must be JSON: url={url}",
                reason_code="web_workstation_invalid_response",
                category="invalid_response",
                retryable=True,
            ) from exc
        if not isinstance(decoded, dict):
            raise WorkstationExecutionClientError(
                "workstation response must be a JSON object",
                reason_code="web_workstation_invalid_response",
                category="invalid_response",
                retryable=True,
            )
        return decoded

    def _control_token(self) -> str:
        token_env = str(self.config.get("control_token_env", "ATS_WORKSTATION_CONTROL_TOKEN")).strip()
        token = os.environ.get(token_env, "").strip()
        if not token:
            raise WorkstationExecutionClientError(
                "workstation control token is not configured",
                reason_code="web_workstation_control_token_missing",
                category="config_error",
                retryable=False,
            )
        return token

    def _ensure_runtime_ready(self) -> None:
        if self.runtime_controller is None:
            return
        status = self.runtime_controller.ensure_started()
        status_payload = status.model_dump() if hasattr(status, "model_dump") else {}
        if status_payload.get("running") is True:
            capabilities = status_payload.get("capabilities")
            if isinstance(capabilities, dict) and capabilities.get("available") is True:
                return
        reason = str(status_payload.get("last_error") or "")
        capabilities = status_payload.get("capabilities")
        if not reason and isinstance(capabilities, dict):
            reason = str(capabilities.get("reason") or "")
        workstation_url = str(status_payload.get("workstation_url") or self.config.get("workstation_url", ""))
        raise WorkstationExecutionClientError(
            f"workstation runtime unavailable before dispatch: url={workstation_url} reason={reason or 'not running'}",
            reason_code="web_workstation_unreachable",
            category="network_error",
            retryable=True,
        )


def workstation_dispatch_error_to_platform_error(error: Exception) -> PlatformExecutionError:
    reason_code = getattr(error, "reason_code", "web_workstation_dispatch_failed")
    category = getattr(error, "category", "network_error")
    retryable = bool(getattr(error, "retryable", True))
    return PlatformExecutionError(
        build_platform_error(
            origin="web_workstation",
            runtime_surface="web",
            component="workstation_dispatch_client",
            operation="dispatch_and_poll",
            category=str(category),
            reason_code=str(reason_code),
            message="Web 托管 Workstation 不可用，请管理员检查托管工作站运行状态。",
            developer_message=str(error),
            retryable=retryable,
            log_hint={"primary": "web_backend_log", "secondary": "web_workstation_stderr"},
            diagnostic={"exception_type": type(error).__name__},
        )
    )
