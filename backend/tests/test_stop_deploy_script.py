from __future__ import annotations

import json
import subprocess
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "tools" / "latest" / "stop-deploy.ps1"


def _wait_for_exit(pid: int, timeout_seconds: float = 5.0) -> bool:
    deadline = time.time() + timeout_seconds
    while time.time() < deadline:
        probe = subprocess.run(
            [
                "pwsh",
                "-NoProfile",
                "-Command",
                f"if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ exit 1 }} else {{ exit 0 }}",
            ],
            cwd=REPO_ROOT,
            check=False,
        )
        if probe.returncode == 0:
            return True
        time.sleep(0.2)
    return False


def test_stop_deploy_stops_recorded_local_processes(tmp_path: Path):
    release_root = tmp_path / "release"
    runtime_dir = release_root / "runtime"
    runtime_dir.mkdir(parents=True)

    sleeper = subprocess.Popen(
        ["pwsh", "-NoProfile", "-Command", "Start-Sleep -Seconds 120"],
        cwd=REPO_ROOT,
    )

    try:
        state_path = runtime_dir / "local-deploy-state.json"
        state_path.write_text(
            json.dumps(
                {
                    "target": "hybrid",
                    "release_root": str(release_root),
                    "processes": [
                        {
                            "service_name": "frontend",
                            "pid": sleeper.pid,
                            "port": 8080,
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )

        completed = subprocess.run(
            [
                "pwsh",
                "-NoProfile",
                "-File",
                str(SCRIPT_PATH),
                "hybrid",
                "-ReleaseRoot",
                str(release_root),
            ],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )

        assert completed.returncode == 0, completed.stderr
        assert _wait_for_exit(sleeper.pid)
        assert not state_path.exists()
    finally:
        if sleeper.poll() is None:
            sleeper.terminate()
            sleeper.wait(timeout=5)
