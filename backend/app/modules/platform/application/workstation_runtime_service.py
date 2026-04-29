from __future__ import annotations

import os
import json
import secrets
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable
from urllib.parse import urlparse
import urllib.error
import urllib.request

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
        }


class WorkstationRuntimeManager:
    def __init__(
        self,
        *,
        settings: Settings,
        cwd: Path,
        popen_factory: Callable[..., subprocess.Popen] = subprocess.Popen,
        token_factory: Callable[[], str] | None = None,
        urlopen: Callable[..., object] = urllib.request.urlopen,
    ) -> None:
        self._settings = settings
        self._cwd = cwd
        self._popen_factory = popen_factory
        self._token_factory = token_factory or (lambda: secrets.token_urlsafe(32))
        self._urlopen = urlopen
        self._process: subprocess.Popen | None = None
        self._last_error = ""

    @property
    def config(self) -> dict[str, object]:
        return self._settings.platform_execution

    def ensure_started(self) -> WorkstationRuntimeStatus:
        if not bool(self.config.get("auto_start", True)):
            return self.get_runtime_status()
        if self._is_process_running():
            return self.get_runtime_status()

        try:
            env = os.environ.copy()
            token_env = self._control_token_env()
            env[token_env] = env.get(token_env, "").strip() or self._token_factory()
            env[_CONFIG_PATH_ENV] = str(self._workstation_config_path())
            os.environ[token_env] = env[token_env]
            self._process = self._popen_factory(
                self._build_command(),
                cwd=str(self._cwd),
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            self._last_error = ""
        except Exception as exc:
            self._last_error = str(exc)[:300]
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

    def get_runtime_status(self) -> WorkstationRuntimeStatus:
        running = self._is_process_running()
        return WorkstationRuntimeStatus(
            available=not bool(self._last_error),
            auto_start=bool(self.config.get("auto_start", True)),
            managed=self._process is not None,
            running=running,
            workstation_url=str(self.config.get("workstation_url", "")),
            control_token_env=self._control_token_env(),
            pid=getattr(self._process, "pid", None) if self._process is not None else None,
            last_error=self._last_error,
            capabilities=self._fetch_capabilities() if running else None,
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
        if self._cwd.name == "backend" and self._cwd.parent.name in {"web", "workstation"} and self._cwd.parent.parent.name == "services":
            return self._cwd.parent.parent.parent
        if self._cwd.name == "backend":
            return self._cwd.parent
        return self._cwd

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
