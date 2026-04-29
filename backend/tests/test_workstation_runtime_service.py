from __future__ import annotations

import sys
import json
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.platform.application.workstation_runtime_service import WorkstationRuntimeManager
from app.shared.infra.config.settings import Settings


class FakeProcess:
    def __init__(self, pid: int = 4321) -> None:
        self.pid = pid
        self.terminated = False
        self.killed = False

    def poll(self):
        return 0 if self.terminated or self.killed else None

    def terminate(self) -> None:
        self.terminated = True

    def kill(self) -> None:
        self.killed = True

    def wait(self, timeout=None):
        return 0


class FakeResponse:
    def __init__(self, payload: dict[str, object]) -> None:
        self._payload = payload

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False

    def read(self) -> bytes:
        return json.dumps(self._payload).encode("utf-8")


def _settings(**platform_execution) -> Settings:
    return Settings.from_dict({"platform_execution": platform_execution})


def test_workstation_runtime_manager_starts_with_generated_control_token(monkeypatch, tmp_path):
    calls = []
    process = FakeProcess()

    def fake_popen(*args, **kwargs):
        calls.append((args, kwargs))
        return process

    monkeypatch.delenv("TEST_WORKSTATION_TOKEN", raising=False)
    manager = WorkstationRuntimeManager(
        settings=_settings(
            workstation_url="http://127.0.0.1:7865",
            workstation_config_path="runtime/workstation.config.json",
            control_token_env="TEST_WORKSTATION_TOKEN",
        ),
        cwd=tmp_path,
        popen_factory=fake_popen,
        token_factory=lambda: "generated-token",
        urlopen=lambda *args, **kwargs: FakeResponse({"generation": {"text_generation_available": True}}),
    )

    status = manager.ensure_started().model_dump()

    assert len(calls) == 1
    command = calls[0][0][0]
    assert command[-4:] == ["--host", "127.0.0.1", "--port", "7865"]
    assert calls[0][1]["env"]["TEST_WORKSTATION_TOKEN"] == "generated-token"
    assert calls[0][1]["env"]["SPIREFORGE_CONFIG_PATH"] == str(tmp_path / "runtime" / "workstation.config.json")
    assert status["managed"] is True
    assert status["running"] is True
    assert status["pid"] == 4321
    assert status["capabilities"]["available"] is True
    assert status["capabilities"]["generation"]["text_generation_available"] is True


def test_workstation_runtime_manager_reuses_running_process(tmp_path):
    calls = []

    def fake_popen(*args, **kwargs):
        calls.append((args, kwargs))
        return FakeProcess()

    manager = WorkstationRuntimeManager(
        settings=_settings(control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
        popen_factory=fake_popen,
        token_factory=lambda: "generated-token",
    )

    manager.ensure_started()
    manager.ensure_started()

    assert len(calls) == 1


def test_workstation_runtime_manager_stops_managed_process(tmp_path):
    process = FakeProcess()
    manager = WorkstationRuntimeManager(
        settings=_settings(control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
        popen_factory=lambda *args, **kwargs: process,
        token_factory=lambda: "generated-token",
    )

    manager.ensure_started()
    manager.stop()

    assert process.terminated is True
    assert manager.get_runtime_status().running is False


def test_workstation_runtime_manager_respects_auto_start_false(tmp_path):
    calls = []
    manager = WorkstationRuntimeManager(
        settings=_settings(auto_start=False, control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
        popen_factory=lambda *args, **kwargs: calls.append((args, kwargs)),
        token_factory=lambda: "generated-token",
    )

    status = manager.ensure_started().model_dump()

    assert calls == []
    assert status["auto_start"] is False
    assert status["managed"] is False
    assert status["running"] is False


def test_workstation_runtime_manager_resolves_release_relative_config_path(tmp_path):
    calls = []
    process = FakeProcess()
    release_root = tmp_path / "release"
    cwd = release_root / "services" / "web" / "backend"
    cwd.mkdir(parents=True)

    def fake_popen(*args, **kwargs):
        calls.append((args, kwargs))
        return process

    manager = WorkstationRuntimeManager(
        settings=_settings(
            workstation_config_path="runtime/workstation.config.json",
            control_token_env="TEST_WORKSTATION_TOKEN",
        ),
        cwd=cwd,
        popen_factory=fake_popen,
        token_factory=lambda: "generated-token",
    )

    manager.ensure_started()

    assert calls[0][1]["env"]["SPIREFORGE_CONFIG_PATH"] == str(
        release_root / "runtime" / "workstation.config.json"
    )
