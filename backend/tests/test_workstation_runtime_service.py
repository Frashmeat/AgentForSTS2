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


def _write_workstation_config(root: Path) -> None:
    config_path = root / "runtime" / "workstation.config.json"
    config_path.parent.mkdir(parents=True, exist_ok=True)
    config_path.write_text("{}", encoding="utf-8")


def test_workstation_runtime_manager_starts_with_generated_control_token(monkeypatch, tmp_path):
    calls = []
    process = FakeProcess()
    _write_workstation_config(tmp_path)

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
    assert status["stdout_log_path"].replace("\\", "/").endswith("runtime/logs/web-workstation.stdout.log")
    assert status["stderr_log_path"].replace("\\", "/").endswith("runtime/logs/web-workstation.stderr.log")


def test_workstation_runtime_manager_reuses_running_process(tmp_path):
    calls = []
    _write_workstation_config(tmp_path)

    def fake_popen(*args, **kwargs):
        calls.append((args, kwargs))
        return FakeProcess()

    manager = WorkstationRuntimeManager(
        settings=_settings(control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
        popen_factory=fake_popen,
        token_factory=lambda: "generated-token",
        urlopen=lambda *args, **kwargs: FakeResponse({"generation": {"text_generation_available": True}}),
    )

    manager.ensure_started()
    manager.ensure_started()

    assert len(calls) == 1


def test_workstation_runtime_manager_stops_managed_process(tmp_path):
    process = FakeProcess()
    _write_workstation_config(tmp_path)
    manager = WorkstationRuntimeManager(
        settings=_settings(control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
        popen_factory=lambda *args, **kwargs: process,
        token_factory=lambda: "generated-token",
        urlopen=lambda *args, **kwargs: FakeResponse({"generation": {"text_generation_available": True}}),
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
    _write_workstation_config(release_root)

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
        urlopen=lambda *args, **kwargs: FakeResponse({"generation": {"text_generation_available": True}}),
    )

    manager.ensure_started()

    assert calls[0][1]["env"]["SPIREFORGE_CONFIG_PATH"] == str(
        release_root / "runtime" / "workstation.config.json"
    )


def test_workstation_runtime_manager_fails_fast_when_config_is_missing(tmp_path):
    calls = []
    manager = WorkstationRuntimeManager(
        settings=_settings(control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
        popen_factory=lambda *args, **kwargs: calls.append((args, kwargs)),
        token_factory=lambda: "generated-token",
    )

    status = manager.ensure_started().model_dump()

    assert calls == []
    assert status["running"] is False
    assert "workstation config file not found" in status["last_error"]


def test_workstation_runtime_manager_reports_process_exit_with_log_tail(tmp_path):
    _write_workstation_config(tmp_path)
    process = FakeProcess()
    process.terminated = True

    def fake_popen(*args, **kwargs):
        kwargs["stderr"].write(b"fatal boot error\n")
        return process

    manager = WorkstationRuntimeManager(
        settings=_settings(
            control_token_env="TEST_WORKSTATION_TOKEN",
            startup_timeout_seconds=1,
        ),
        cwd=tmp_path,
        popen_factory=fake_popen,
        token_factory=lambda: "generated-token",
        sleep=lambda seconds: None,
    )

    status = manager.ensure_started().model_dump()

    assert status["running"] is False
    assert "workstation process exited with code 0" in status["last_error"]
    assert "fatal boot error" in status["last_error"]


def test_workstation_runtime_manager_clears_startup_timeout_after_capabilities_recover(tmp_path):
    _write_workstation_config(tmp_path)
    responses = iter(
        [
            Exception("connection refused"),
            {"generation": {"text_generation_available": True}},
        ]
    )

    def fake_urlopen(*args, **kwargs):
        response = next(responses)
        if isinstance(response, Exception):
            raise response
        return FakeResponse(response)

    manager = WorkstationRuntimeManager(
        settings=_settings(
            control_token_env="TEST_WORKSTATION_TOKEN",
            startup_timeout_seconds=-1,
        ),
        cwd=tmp_path,
        popen_factory=lambda *args, **kwargs: FakeProcess(),
        token_factory=lambda: "generated-token",
        sleep=lambda seconds: None,
        urlopen=fake_urlopen,
    )

    first_status = manager.ensure_started().model_dump()
    recovered_status = manager.get_runtime_status().model_dump()

    assert "workstation did not become ready before timeout" in first_status["last_error"]
    assert recovered_status["running"] is True
    assert recovered_status["available"] is True
    assert recovered_status["last_error"] == ""
    assert recovered_status["capabilities"]["available"] is True


def test_workstation_runtime_manager_reads_fixed_log_tail(tmp_path):
    manager = WorkstationRuntimeManager(
        settings=_settings(control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
    )
    log_dir = tmp_path / "runtime" / "logs"
    log_dir.mkdir(parents=True)
    log_content = "line-1\nline-2\nline-3"
    log_path = log_dir / "web-workstation.stderr.log"
    log_path.write_text(log_content, encoding="utf-8")

    result = manager.read_runtime_log_tail("stderr", tail_bytes=12).model_dump()

    assert result["stream"] == "stderr"
    assert result["exists"] is True
    assert result["size_bytes"] == log_path.stat().st_size
    assert result["tail_bytes"] == 12
    assert result["truncated"] is True
    assert str(result["content"]).endswith("line-3")
    assert str(result["path"]).replace("\\", "/").endswith("runtime/logs/web-workstation.stderr.log")


def test_workstation_runtime_manager_reports_missing_log_file(tmp_path):
    manager = WorkstationRuntimeManager(
        settings=_settings(control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
    )

    result = manager.read_runtime_log_tail("stdout").model_dump()

    assert result["stream"] == "stdout"
    assert result["exists"] is False
    assert result["size_bytes"] == 0
    assert result["truncated"] is False
    assert result["content"] == ""


def test_workstation_runtime_manager_rejects_unknown_log_stream(tmp_path):
    manager = WorkstationRuntimeManager(
        settings=_settings(control_token_env="TEST_WORKSTATION_TOKEN"),
        cwd=tmp_path,
    )

    try:
        manager.read_runtime_log_tail("../config")
    except ValueError as error:
        assert str(error) == "stream must be stdout or stderr"
    else:
        raise AssertionError("expected ValueError for invalid stream")
