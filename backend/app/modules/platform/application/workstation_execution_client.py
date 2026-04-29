from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Callable
from urllib.parse import urljoin

from app.modules.platform.contracts.runner_contracts import StepExecutionResult
from app.modules.platform.contracts.workstation_execution import (
    WorkstationExecutionDispatchAccepted,
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionEvent,
    WorkstationExecutionPollResult,
)
from app.shared.infra.config.settings import Settings


class WorkstationExecutionClientError(RuntimeError):
    pass


WorkstationEventHandler = Callable[[list[WorkstationExecutionEvent]], None]


@dataclass(slots=True)
class WorkstationExecutionClient:
    settings: Settings
    urlopen: Callable[..., object] = urllib.request.urlopen
    sleep: Callable[[float], None] = time.sleep
    monotonic: Callable[[], float] = time.monotonic

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
                    f"workstation execution timed out: {workstation_execution_id}"
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
        request = urllib.request.Request(
            urljoin(str(self.config.get("workstation_url", "http://127.0.0.1:7860")), path),
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
            raise WorkstationExecutionClientError(f"workstation request failed: HTTP {exc.code}") from exc
        except OSError as exc:
            raise WorkstationExecutionClientError(f"workstation request failed: {exc}") from exc

        decoded = json.loads(raw.decode("utf-8") if raw else "{}")
        if not isinstance(decoded, dict):
            raise WorkstationExecutionClientError("workstation response must be a JSON object")
        return decoded

    def _control_token(self) -> str:
        token_env = str(self.config.get("control_token_env", "ATS_WORKSTATION_CONTROL_TOKEN")).strip()
        token = os.environ.get(token_env, "").strip()
        if not token:
            raise WorkstationExecutionClientError("workstation control token is not configured")
        return token
