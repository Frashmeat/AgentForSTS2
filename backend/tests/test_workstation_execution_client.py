from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.platform.application.workstation_execution_client import (
    WorkstationExecutionClient,
    WorkstationExecutionClientError,
)
from app.modules.platform.contracts.workstation_execution import WorkstationExecutionDispatchRequest
from app.shared.infra.config.settings import Settings


class FakeResponse:
    def __init__(self, payload: dict[str, object]) -> None:
        self._payload = payload

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def read(self) -> bytes:
        return json.dumps(self._payload).encode("utf-8")


class FakeRuntimeStatus:
    def __init__(self, payload: dict[str, object]) -> None:
        self._payload = payload

    def model_dump(self) -> dict[str, object]:
        return self._payload


class FakeRuntimeController:
    def __init__(self, payload: dict[str, object]) -> None:
        self.payload = payload
        self.calls = 0

    def ensure_started(self) -> FakeRuntimeStatus:
        self.calls += 1
        return FakeRuntimeStatus(self.payload)


def _request() -> WorkstationExecutionDispatchRequest:
    return WorkstationExecutionDispatchRequest.model_validate(
        {
            "execution_id": 2203,
            "job_id": 2002,
            "job_item_id": 2103,
            "job_type": "single_generate",
            "item_type": "relic",
            "workflow_version": "2026.03.31",
            "step_protocol_version": "v1",
            "result_schema_version": "v1",
            "input_payload": {"description": "生成遗物"},
            "execution_binding": {
                "provider": "openai",
                "model": "gpt-5.4",
                "credential_ref": "server-credential:1",
                "credential": "sk-live",
            },
        }
    )


def _settings() -> Settings:
    return Settings.from_dict(
        {
            "platform_execution": {
                "workstation_url": "http://127.0.0.1:7860",
                "control_token_env": "TEST_WORKSTATION_TOKEN",
                "poll_interval_seconds": 0,
                "execution_timeout_seconds": 1,
            }
        }
    )


def test_workstation_execution_client_dispatches_with_control_token_and_polls_result(monkeypatch):
    monkeypatch.setenv("TEST_WORKSTATION_TOKEN", "secret-token")
    calls = []
    responses = [
        {
            "workstation_execution_id": "ws-exec-2203",
            "status": "accepted",
            "poll_url": "/api/workstation/platform/executions/ws-exec-2203",
        },
        {
            "workstation_execution_id": "ws-exec-2203",
            "status": "running",
            "step_id": "single.relic.plan",
            "events": [
                {
                    "sequence": 1,
                    "event_type": "workstation.step.started",
                    "occurred_at": "2026-04-29T10:00:00+00:00",
                    "payload": {"step_id": "single.relic.plan"},
                }
            ],
        },
        {
            "workstation_execution_id": "ws-exec-2203",
            "status": "succeeded",
            "step_id": "single.relic.plan",
            "output_payload": {"text": "ok"},
            "events": [
                {
                    "sequence": 1,
                    "event_type": "workstation.step.started",
                    "occurred_at": "2026-04-29T10:00:00+00:00",
                    "payload": {"step_id": "single.relic.plan"},
                },
                {
                    "sequence": 2,
                    "event_type": "workstation.step.finished",
                    "occurred_at": "2026-04-29T10:00:01+00:00",
                    "payload": {"step_id": "single.relic.plan"},
                },
            ],
        },
    ]

    def fake_urlopen(request, timeout):
        calls.append((request, timeout))
        return FakeResponse(responses.pop(0))

    client = WorkstationExecutionClient(
        settings=_settings(),
        urlopen=fake_urlopen,
        sleep=lambda seconds: None,
    )
    seen_events = []

    result = client.dispatch_and_poll(_request(), on_events=lambda events: seen_events.extend(events))

    assert result.model_dump() == {
        "step_id": "single.relic.plan",
        "status": "succeeded",
        "output_payload": {"text": "ok"},
        "error_summary": "",
        "error_payload": {},
    }
    assert [call[0].method for call in calls] == ["POST", "GET", "GET"]
    assert calls[0][0].headers["X-ats-workstation-token"] == "secret-token"
    assert b"sk-live" in calls[0][0].data
    assert [event.sequence for event in seen_events] == [1, 2]


def test_workstation_execution_client_fails_when_control_token_missing(monkeypatch):
    monkeypatch.delenv("TEST_WORKSTATION_TOKEN", raising=False)
    client = WorkstationExecutionClient(settings=_settings())

    with pytest.raises(WorkstationExecutionClientError, match="control token"):
        client.dispatch(_request())


def test_workstation_execution_client_times_out_while_execution_is_running(monkeypatch):
    monkeypatch.setenv("TEST_WORKSTATION_TOKEN", "secret-token")
    ticks = iter([0.0, 2.0])

    def fake_urlopen(request, timeout):
        return FakeResponse(
            {
                "workstation_execution_id": "ws-exec-2203",
                "status": "running",
                "step_id": "single.relic.plan",
            }
        )

    client = WorkstationExecutionClient(
        settings=_settings(),
        urlopen=fake_urlopen,
        sleep=lambda seconds: None,
        monotonic=lambda: next(ticks),
    )

    with pytest.raises(WorkstationExecutionClientError, match="timed out"):
        client.poll_until_finished("ws-exec-2203")


def test_workstation_execution_client_checks_runtime_before_dispatch(monkeypatch):
    monkeypatch.setenv("TEST_WORKSTATION_TOKEN", "secret-token")
    runtime = FakeRuntimeController(
        {
            "running": False,
            "workstation_url": "http://127.0.0.1:7860",
            "last_error": "workstation config file not found",
        }
    )
    calls = []
    client = WorkstationExecutionClient(
        settings=_settings(),
        runtime_controller=runtime,
        urlopen=lambda *args, **kwargs: calls.append((args, kwargs)),
    )

    with pytest.raises(WorkstationExecutionClientError, match="workstation runtime unavailable before dispatch"):
        client.dispatch(_request())

    assert runtime.calls == 1
    assert calls == []


def test_workstation_execution_client_requires_capabilities_ready_before_dispatch(monkeypatch):
    monkeypatch.setenv("TEST_WORKSTATION_TOKEN", "secret-token")
    runtime = FakeRuntimeController(
        {
            "running": True,
            "workstation_url": "http://127.0.0.1:7860",
            "capabilities": {"available": False, "reason": "connection refused"},
        }
    )
    calls = []
    client = WorkstationExecutionClient(
        settings=_settings(),
        runtime_controller=runtime,
        urlopen=lambda *args, **kwargs: calls.append((args, kwargs)),
    )

    with pytest.raises(WorkstationExecutionClientError, match="connection refused"):
        client.dispatch(_request())

    assert runtime.calls == 1
    assert calls == []
