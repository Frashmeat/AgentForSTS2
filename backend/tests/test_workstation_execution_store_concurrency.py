from __future__ import annotations

import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.platform.contracts.workstation_execution import (
    WorkstationExecutionDispatchRequest,
    WorkstationExecutionPollResult,
)
from app.shared.infra.config.settings import Settings
from routers.workstation_platform import WorkstationExecutionStore


class BlockingExecutor:
    def __init__(self) -> None:
        self.started = 0
        self.max_active = 0
        self._active = 0
        self._lock = threading.Lock()
        self.release = threading.Event()

    def execute(self, request, event_sink=None):
        with self._lock:
            self.started += 1
            self._active += 1
            self.max_active = max(self.max_active, self._active)
        self.release.wait(timeout=2)
        with self._lock:
            self._active -= 1
        return WorkstationExecutionPollResult(
            workstation_execution_id=f"ws-exec-{request.execution_id}",
            status="succeeded",
            step_id="done",
        )


def _settings(**platform_execution) -> Settings:
    return Settings.from_dict({"platform_execution": platform_execution})


def _request(execution_id: int, item_type: str = "relic", server_project_ref: str = ""):
    payload = {"description": "生成内容"}
    if server_project_ref:
        payload["server_project_ref"] = server_project_ref
    return WorkstationExecutionDispatchRequest.model_validate(
        {
            "execution_id": execution_id,
            "job_id": 1,
            "job_item_id": execution_id,
            "job_type": "single_generate",
            "item_type": item_type,
            "workflow_version": "2026.03.31",
            "step_protocol_version": "v1",
            "result_schema_version": "v1",
            "input_payload": payload,
        }
    )


def test_workstation_store_limits_text_concurrency_to_configured_value():
    store = WorkstationExecutionStore()
    executor = BlockingExecutor()
    settings = _settings(max_concurrent_text=1)

    store.submit(_request(1), executor, settings)
    store.submit(_request(2), executor, settings)
    time.sleep(0.1)

    assert executor.started == 1
    assert executor.max_active == 1
    executor.release.set()


def test_workstation_store_serializes_same_server_project_ref():
    store = WorkstationExecutionStore()
    executor = BlockingExecutor()
    settings = _settings(max_concurrent_code=2)

    store.submit(_request(1, item_type="custom_code", server_project_ref="server-workspace:a"), executor, settings)
    store.submit(_request(2, item_type="custom_code", server_project_ref="server-workspace:a"), executor, settings)
    time.sleep(0.1)

    assert executor.started == 1
    assert executor.max_active == 1
    executor.release.set()
