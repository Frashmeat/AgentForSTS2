from __future__ import annotations

import json
import os
import secrets
import subprocess
import sys
import time
import urllib.error
import urllib.request
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import urlparse

from app.shared.infra.config.settings import Settings

_CONFIG_PATH_ENV = "SPIREFORGE_CONFIG_PATH"


@dataclass(slots=True)
class WorkstationRuntimeStatus:
    available: bool
    auto_start: bool
    managed: bool
    running: bool
    workstation_url: str
    control_token_env: str
    pid: int | None = None
    last_error: str = ""
    capabilities: dict[str, object] | None = None
    stdout_log_path: str = ""
    stderr_log_path: str = ""

    def model_dump(self) -> dict[str, object]:
        return {
            "available": self.available,
            "auto_start": self.auto_start,
            "managed": self.managed,
            "running": self.running,
            "workstation_url": self.workstation_url,
            "control_token_env": self.control_token_env,
            "pid": self.pid,
            "last_error": self.last_error,
            "capabilities": self.capabilities,
            "stdout_log_path": self.stdout_log_path,
            "stderr_log_path": self.stderr_log_path,
        }


@dataclass(slots=True)
class WorkstationRuntimeLogTail:
    stream: str
    path: str
    exists: bool
    size_bytes: int
    tail_bytes: int
    truncated: bool
    content: str

    def model_dump(self) -> dict[str, object]:
        return {
            "stream": self.stream,
            "path": self.path,
            "exists": self.exists,
            "size_bytes": self.size_bytes,
            "tail_bytes": self.tail_bytes,
            "truncated": self.truncated,
            "content": self.content,
        }


class WorkstationRuntimeManager:
    _MAX_LOG_TAIL_BYTES = 262_144

    def __init__(
        self,
        *,
        settings: Settings,
        cwd: Path,
        popen_factory: Callable[..., subprocess.Popen] = subprocess.Popen,
        token_factory: Callable[[], str] | None = None,
        urlopen: Callable[..., object] = urllib.request.urlopen,
        sleep: Callable[[float], None] = time.sleep,
    ) -> None:
        self._settings = settings
        self._cwd = cwd
        self._popen_factory = popen_factory
        self._token_factory = token_factory or (lambda: secrets.token_urlsafe(32))
        self._urlopen = urlopen
        self._sleep = sleep
        self._process: subprocess.Popen | None = None
        self._last_error = ""
        self._log_handles: list[object] = []

    @property
    def config(self) -> dict[str, object]:
        return self._settings.platform_execution

    def ensure_started(self) -> WorkstationRuntimeStatus:
        if not bool(self.config.get("auto_start", True)):
            return self.get_runtime_status()
        if self._is_process_running():
            return self.get_runtime_status()

        try:
            workstation_config_path = self._workstation_config_path()
            if not workstation_config_path.exists():
                self._last_error = f"workstation config file not found: {workstation_config_path}"
                return self.get_runtime_status()
            env = os.environ.copy()
            token_env = self._control_token_env()
            env[token_env] = env.get(token_env, "").strip() or self._token_factory()
            env[_CONFIG_PATH_ENV] = str(self._workstation_config_path())
            os.environ[token_env] = env[token_env]
            stdout_handle, stderr_handle = self._open_log_handles()
            self._close_log_handles()
            self._log_handles = [stdout_handle, stderr_handle]
            self._process = self._popen_factory(
                self._build_command(),
                cwd=str(self._cwd),
                env=env,
                stdout=stdout_handle,
                stderr=stderr_handle,
            )
            self._last_error = ""
        except Exception as exc:
            self._last_error = str(exc)[:300]
            self._close_log_handles()
        if self._process is not None:
            self._wait_until_ready()
        return self.get_runtime_status()

    def stop(self) -> None:
        if self._process is None or self._process.poll() is not None:
            return
        self._process.terminate()
        try:
            self._process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self._process.kill()
            self._process.wait(timeout=10)
        finally:
            self._close_log_handles()

    def get_runtime_status(self) -> WorkstationRuntimeStatus:
        process_running = self._is_process_running()
        managed = self._process is not None
        capabilities = self._fetch_capabilities() if process_running or not managed else None
        reachable = isinstance(capabilities, dict) and capabilities.get("available") is True
        running = process_running or (not managed and reachable)
        if reachable and self._last_error.startswith("workstation did not become ready before timeout"):
            self._last_error = ""
        return WorkstationRuntimeStatus(
            available=not bool(self._last_error),
            auto_start=bool(self.config.get("auto_start", True)),
            managed=managed,
            running=running,
            workstation_url=str(self.config.get("workstation_url", "")),
            control_token_env=self._control_token_env(),
            pid=getattr(self._process, "pid", None) if self._process is not None else None,
            last_error=self._last_error,
            capabilities=capabilities,
            stdout_log_path=str(self._stdout_log_path()),
            stderr_log_path=str(self._stderr_log_path()),
        )

    def read_runtime_log_tail(self, stream: str, tail_bytes: int = 65_536) -> WorkstationRuntimeLogTail:
        normalized_stream = str(stream or "").strip().lower()
        if normalized_stream == "stdout":
            path = self._stdout_log_path()
        elif normalized_stream == "stderr":
            path = self._stderr_log_path()
        else:
            raise ValueError("stream must be stdout or stderr")

        effective_tail_bytes = max(1, min(int(tail_bytes or 65_536), self._MAX_LOG_TAIL_BYTES))
        if not path.exists():
            return WorkstationRuntimeLogTail(
                stream=normalized_stream,
                path=str(path),
                exists=False,
                size_bytes=0,
                tail_bytes=effective_tail_bytes,
                truncated=False,
                content="",
            )

        size_bytes = path.stat().st_size
        truncated = size_bytes > effective_tail_bytes
        with path.open("rb") as handle:
            if truncated:
                handle.seek(-effective_tail_bytes, os.SEEK_END)
            data = handle.read()
        return WorkstationRuntimeLogTail(
            stream=normalized_stream,
            path=str(path),
            exists=True,
            size_bytes=size_bytes,
            tail_bytes=effective_tail_bytes,
            truncated=truncated,
            content=data.decode("utf-8", errors="replace"),
        )

    def _is_process_running(self) -> bool:
        return self._process is not None and self._process.poll() is None

    def _control_token_env(self) -> str:
        return str(self.config.get("control_token_env", "ATS_WORKSTATION_CONTROL_TOKEN")).strip()

    def _workstation_config_path(self) -> Path:
        configured = str(self.config.get("workstation_config_path", "runtime/workstation.config.json")).strip()
        path = Path(configured or "runtime/workstation.config.json")
        if path.is_absolute():
            return path
        return self._runtime_root() / path

    def _runtime_root(self) -> Path:
        if (
            self._cwd.name == "backend"
            and self._cwd.parent.name in {"web", "workstation"}
            and self._cwd.parent.parent.name == "services"
        ):
            return self._cwd.parent.parent.parent
        if self._cwd.name == "backend":
            return self._cwd.parent
        return self._cwd

    def _log_dir(self) -> Path:
        return self._runtime_root() / "runtime" / "logs"

    def _stdout_log_path(self) -> Path:
        return self._log_dir() / "web-workstation.stdout.log"

    def _stderr_log_path(self) -> Path:
        return self._log_dir() / "web-workstation.stderr.log"

    def _open_log_handles(self) -> tuple[object, object]:
        self._log_dir().mkdir(parents=True, exist_ok=True)
        return (
            self._stdout_log_path().open("ab", buffering=0),
            self._stderr_log_path().open("ab", buffering=0),
        )

    def _close_log_handles(self) -> None:
        while self._log_handles:
            handle = self._log_handles.pop()
            close = getattr(handle, "close", None)
            if callable(close):
                close()

    def _build_command(self) -> list[str]:
        url = str(self.config.get("workstation_url", "http://127.0.0.1:7860"))
        parsed = urlparse(url)
        host = parsed.hostname or "127.0.0.1"
        port = parsed.port or 7860
        return [
            sys.executable,
            "-m",
            "uvicorn",
            "main_workstation:app",
            "--host",
            host,
            "--port",
            str(port),
        ]

    def _wait_until_ready(self) -> None:
        deadline = time.monotonic() + float(self.config.get("startup_timeout_seconds", 10))
        last_reason = ""
        while time.monotonic() < deadline:
            if self._process is not None and self._process.poll() is not None:
                self._last_error = self._build_process_exit_error()
                return
            capabilities = self._fetch_capabilities()
            if capabilities.get("available") is True:
                return
            last_reason = str(capabilities.get("reason", "workstation not ready"))
            self._sleep(0.25)
        self._last_error = f"workstation did not become ready before timeout: {last_reason}"

    def _build_process_exit_error(self) -> str:
        code = self._process.poll() if self._process is not None else "unknown"
        stderr_tail = self._read_log_tail(self._stderr_log_path())
        stdout_tail = self._read_log_tail(self._stdout_log_path())
        detail = stderr_tail or stdout_tail
        if detail:
            return f"workstation process exited with code {code}: {detail}"[:300]
        return f"workstation process exited with code {code}"

    def _read_log_tail(self, path: Path, limit: int = 500) -> str:
        try:
            if not path.exists():
                return ""
            data = path.read_text(encoding="utf-8", errors="replace")
            return data[-limit:].strip()
        except Exception:
            return ""

    def _fetch_capabilities(self) -> dict[str, object]:
        token = os.environ.get(self._control_token_env(), "").strip()
        if not token:
            return {"available": False, "reason": "workstation_control_token_missing"}
        request = urllib.request.Request(
            url=f"{str(self.config.get('workstation_url', 'http://127.0.0.1:7860')).rstrip('/')}/api/workstation/capabilities",
            method="GET",
            headers={"X-ATS-Workstation-Token": token},
        )
        try:
            response = self._urlopen(
                request,
                timeout=int(self.config.get("dispatch_timeout_seconds", 10)),
            )
            with response:
                raw = response.read()
            payload = json.loads(raw.decode("utf-8") if raw else "{}")
            if isinstance(payload, dict):
                return {"available": True, **payload}
            return {"available": False, "reason": "invalid_capabilities_payload"}
        except urllib.error.HTTPError as exc:
            return {"available": False, "reason": f"http_{exc.code}"}
        except Exception as exc:
            return {"available": False, "reason": str(exc)[:120]}
