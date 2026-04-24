from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "tools" / "latest" / "deploy-docker.ps1"


def _write_fake_docker(bin_dir: Path) -> Path:
    docker_cmd = bin_dir / "docker.cmd"
    docker_cmd.write_text(
        "@echo off\r\n"
        "setlocal EnableDelayedExpansion\r\n"
        "set \"LOG=%MOCK_DOCKER_LOG%\"\r\n"
        "if \"%~1\"==\"image\" (\r\n"
        "  if \"%~2\"==\"inspect\" (\r\n"
        "    >>\"%LOG%\" echo image inspect %~3\r\n"
        "    if /I \"%MOCK_DOCKER_IMAGE_EXISTS%\"==\"1\" exit /b 0\r\n"
        "    exit /b 1\r\n"
        "  )\r\n"
        "  if \"%~2\"==\"rm\" (\r\n"
        "    >>\"%LOG%\" echo image rm %~4\r\n"
        "    exit /b 0\r\n"
        "  )\r\n"
        ")\r\n"
        "if \"%~1\"==\"compose\" (\r\n"
        "  set \"CMD=\"\r\n"
        "  :scan\r\n"
        "  if \"%~1\"==\"\" goto done\r\n"
        "  if /I \"%~1\"==\"build\" set \"CMD=build\"\r\n"
        "  if /I \"%~1\"==\"up\" set \"CMD=up\"\r\n"
        "  if /I \"%~1\"==\"down\" set \"CMD=down\"\r\n"
        "  shift\r\n"
        "  goto scan\r\n"
        "  :done\r\n"
        "  >>\"%LOG%\" echo compose !CMD! %*\r\n"
        "  exit /b 0\r\n"
        ")\r\n"
        ">>\"%LOG%\" echo unexpected %*\r\n"
        "exit /b 0\r\n",
        encoding="utf-8",
    )
    return docker_cmd


def _write_fake_python(bin_dir: Path) -> Path:
    python_cmd = bin_dir / "python.cmd"
    python_cmd.write_text(
        "@echo off\r\n"
        "setlocal EnableDelayedExpansion\r\n"
        "set \"LOG=%MOCK_PYTHON_LOG%\"\r\n"
        ">>\"%LOG%\" echo python %*\r\n"
        "echo MOCK-STDOUT %*\r\n"
        ">&2 echo MOCK-STDERR %*\r\n"
        "exit /b 0\r\n",
        encoding="utf-8",
    )
    return python_cmd


def _prepare_release_bundle(release_root: Path, target: str) -> tuple[Path, Path]:
    if target == "hybrid":
        (release_root / "services" / "workstation" / "backend").mkdir(parents=True, exist_ok=True)
        (release_root / "services" / "workstation" / "frontend" / "dist").mkdir(parents=True, exist_ok=True)
        (release_root / "services" / "frontend" / "frontend" / "dist").mkdir(parents=True, exist_ok=True)
        (release_root / "services" / "workstation" / "config.example.json").write_text("{}", encoding="utf-8")
        compose = "services:\n  workstation:\n    image: fake\n  frontend:\n    image: fake\n"
    elif target == "frontend":
        (release_root / "services" / "frontend" / "frontend" / "dist").mkdir(parents=True, exist_ok=True)
        compose = "services:\n  frontend:\n    image: fake\n"
    elif target == "web":
        (release_root / "services" / "web").mkdir(parents=True, exist_ok=True)
        (release_root / "services" / "web" / "config.example.json").write_text("{}", encoding="utf-8")
        compose = "services:\n  web:\n    image: fake\n"
    else:
        (release_root / "services" / "workstation" / "backend").mkdir(parents=True, exist_ok=True)
        (release_root / "services" / "workstation" / "frontend" / "dist").mkdir(parents=True, exist_ok=True)
        (release_root / "services" / "workstation" / "config.example.json").write_text("{}", encoding="utf-8")
        compose = "services:\n  workstation:\n    image: fake\n"

    (release_root / "docker-compose.yml").write_text(compose, encoding="utf-8")
    config_path = release_root / "input-config.json"
    config_path.write_text(json.dumps({"migration": {}, "database": {}}), encoding="utf-8")
    return release_root, config_path


def _run_deploy(
    tmp_path: Path,
    target: str = "workstation",
    *extra_args: str,
    prepare_default_web_release: bool = False,
    record_log_viewers: bool = False,
    pass_config_path: bool = True,
) -> subprocess.CompletedProcess[str]:
    release_root, config_path = _prepare_release_bundle(tmp_path / "release", target)
    if prepare_default_web_release:
        _prepare_release_bundle(tmp_path / "agentthespire-web-release", "web")
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    _write_fake_docker(bin_dir)
    python_cmd = _write_fake_python(bin_dir)

    log_path = tmp_path / "docker.log"
    python_log_path = tmp_path / "python.log"
    env = os.environ.copy()
    env["PATH"] = str(bin_dir) + os.pathsep + env["PATH"]
    env["MOCK_DOCKER_LOG"] = str(log_path)
    env["MOCK_DOCKER_IMAGE_EXISTS"] = "1"
    env["MOCK_PYTHON_LOG"] = str(python_log_path)
    env["ATS_SKIP_LOCAL_READY_CHECK"] = "1"
    env["ATS_PYTHON_COMMAND"] = str(python_cmd)
    env["ATS_DISABLE_LOG_WINDOWS"] = "1"
    if record_log_viewers:
        env["ATS_LOG_VIEWER_RECORD_FILE"] = str(tmp_path / "log-viewers.txt")

    command = [
        "pwsh",
        "-NoProfile",
        "-File",
        str(SCRIPT_PATH),
        target,
        "-ReleaseRoot",
        str(release_root),
        *extra_args,
    ]
    if pass_config_path:
        command.extend([
            "-ConfigPath",
            str(config_path),
        ])
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        env=env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    completed.docker_log = log_path.read_text(encoding="utf-8") if log_path.exists() else ""  # type: ignore[attr-defined]
    completed.python_log = python_log_path.read_text(encoding="utf-8") if python_log_path.exists() else ""  # type: ignore[attr-defined]
    record_path = tmp_path / "log-viewers.txt"
    completed.log_viewers = record_path.read_text(encoding="utf-8") if record_path.exists() else ""  # type: ignore[attr-defined]
    return completed


def test_deploy_docker_default_builds_even_when_local_image_exists(tmp_path: Path):
    result = _run_deploy(tmp_path)

    assert result.returncode == 0, result.stderr
    assert result.docker_log == ""
    assert "-m uvicorn main_workstation:app --host 127.0.0.1 --port 7860" in result.python_log


def test_deploy_docker_reuse_images_skips_build_when_local_image_exists(tmp_path: Path):
    result = _run_deploy(tmp_path, "-ReuseImages")

    assert result.returncode == 0, result.stderr
    assert result.docker_log == ""
    assert "-m uvicorn main_workstation:app --host 127.0.0.1 --port 7860" in result.python_log


def test_deploy_docker_hybrid_opens_dedicated_log_windows_for_frontend_and_workstation(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-WebBaseUrl", "https://platform.example.com", record_log_viewers=True)

    assert result.returncode == 0, result.stderr
    assert "workstation|" in result.log_viewers
    assert "frontend|" in result.log_viewers


def test_deploy_docker_hybrid_writes_local_process_state_file(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-WebBaseUrl", "https://platform.example.com")
    state_path = tmp_path / "release" / "runtime" / "local-deploy-state.json"

    assert result.returncode == 0, result.stderr
    assert state_path.exists()

    payload = json.loads(state_path.read_text(encoding="utf-8"))
    assert payload["target"] == "hybrid"
    assert payload["release_root"] == str(tmp_path / "release")
    assert {entry["service_name"] for entry in payload["processes"]} == {"workstation", "frontend"}


def test_deploy_docker_hybrid_requires_explicit_web_base_url_or_local_web_flag(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", prepare_default_web_release=True)

    assert result.returncode != 0
    assert "hybrid 默认不再自动联动本机 web-backend" in result.stderr
    assert result.docker_log == ""


def test_deploy_docker_hybrid_can_explicitly_deploy_local_web_release(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-DeployLocalWeb", prepare_default_web_release=True)
    runtime_config = tmp_path / "release" / "services" / "frontend" / "frontend" / "dist" / "runtime-config.js"

    assert result.returncode == 0, result.stderr
    assert result.docker_log.count("compose down") == 1
    assert result.docker_log.count("compose build") == 1
    assert result.docker_log.count("compose up") == 1
    assert result.docker_log.index("compose down") < result.docker_log.index("compose build") < result.docker_log.index("compose up")
    assert "-m uvicorn main_workstation:app --host 127.0.0.1 --port 7860" in result.python_log
    assert "-m http.server 8080 --bind 127.0.0.1 --directory" in result.python_log
    assert runtime_config.exists()
    assert 'web: "http://127.0.0.1:7870"' in runtime_config.read_text(encoding="utf-8")


def test_deploy_docker_hybrid_custom_frontend_port_updates_runtime_cors_origins(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-DeployLocalWeb", "-FrontendPort", "4173", prepare_default_web_release=True)
    workstation_config_path = tmp_path / "release" / "runtime" / "workstation.config.json"
    web_config_path = tmp_path / "agentthespire-web-release" / "runtime" / "web.config.json"

    assert result.returncode == 0, result.stderr
    workstation_config = json.loads(workstation_config_path.read_text(encoding="utf-8"))
    web_config = json.loads(web_config_path.read_text(encoding="utf-8"))
    assert "http://localhost:4173" in workstation_config["runtime"]["workstation"]["cors_origins"]
    assert "http://127.0.0.1:4173" in workstation_config["runtime"]["workstation"]["cors_origins"]
    assert workstation_config["runtime"]["workstation"]["allow_loopback_origins"] is True
    assert "http://localhost:4173" in web_config["runtime"]["web"]["cors_origins"]
    assert "http://127.0.0.1:4173" in web_config["runtime"]["web"]["cors_origins"]
    assert web_config["runtime"]["web"]["allow_loopback_origins"] is True


def test_deploy_docker_hybrid_local_web_can_omit_explicit_config_path(tmp_path: Path):
    result = _run_deploy(
        tmp_path,
        "hybrid",
        "-DeployLocalWeb",
        prepare_default_web_release=True,
        pass_config_path=False,
    )
    runtime_config = tmp_path / "release" / "services" / "frontend" / "frontend" / "dist" / "runtime-config.js"

    assert result.returncode == 0, result.stderr
    assert result.docker_log.count("compose down") == 1
    assert result.docker_log.count("compose build") == 1
    assert result.docker_log.count("compose up") == 1
    assert runtime_config.exists()
    assert 'web: "http://127.0.0.1:7870"' in runtime_config.read_text(encoding="utf-8")


def test_deploy_docker_hybrid_falls_back_when_runtime_workstation_config_is_invalid(tmp_path: Path):
    release_root, _ = _prepare_release_bundle(tmp_path / "release", "hybrid")
    runtime_config_path = release_root / "runtime" / "workstation.config.json"
    runtime_config_path.parent.mkdir(parents=True, exist_ok=True)
    runtime_config_path.write_text('{"runtime":{"bad" a}}\n', encoding="utf-8")

    _prepare_release_bundle(tmp_path / "agentthespire-web-release", "web")
    result = _run_deploy(
        tmp_path,
        "hybrid",
        "-DeployLocalWeb",
        prepare_default_web_release=False,
        pass_config_path=False,
    )
    frontend_runtime_config = release_root / "services" / "frontend" / "frontend" / "dist" / "runtime-config.js"

    assert result.returncode == 0, result.stderr
    assert "检测到损坏的 runtime 配置，已回退到模板配置" in result.stdout
    assert frontend_runtime_config.exists()
    rewritten_config = json.loads(runtime_config_path.read_text(encoding="utf-8"))
    assert rewritten_config["migration"]["platform_jobs_api_enabled"] is False
    assert rewritten_config["migration"]["platform_service_split_enabled"] is False


def test_deploy_docker_hybrid_accepts_explicit_web_release_root(tmp_path: Path):
    custom_web_release_root = tmp_path / "custom-web-release"
    _prepare_release_bundle(custom_web_release_root, "web")

    result = _run_deploy(tmp_path, "hybrid", "-DeployLocalWeb", "-WebReleaseRoot", str(custom_web_release_root))
    runtime_config = tmp_path / "release" / "services" / "frontend" / "frontend" / "dist" / "runtime-config.js"

    assert result.returncode == 0, result.stderr
    assert result.docker_log.count("compose build") == 1
    assert result.docker_log.count("compose up") == 1
    assert runtime_config.exists()
    assert 'web: "http://127.0.0.1:7870"' in runtime_config.read_text(encoding="utf-8")


def test_deploy_docker_web_writes_python_base_image_to_env_file(tmp_path: Path):
    result = _run_deploy(tmp_path, "web")
    env_file = tmp_path / "release" / "runtime" / ".env"

    assert result.returncode == 0, result.stderr
    assert env_file.exists()
    assert "ATS_PYTHON_BASE_IMAGE=" in env_file.read_text(encoding="utf-8")


def test_deploy_docker_hybrid_runs_locally_without_current_release_docker_when_web_base_is_remote(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-WebBaseUrl", "https://platform.example.com")

    runtime_config = tmp_path / "release" / "services" / "frontend" / "frontend" / "dist" / "runtime-config.js"

    assert result.returncode == 0, result.stderr
    assert "image inspect postgres:16-alpine" not in result.docker_log
    assert result.docker_log == ""
    assert "-m uvicorn main_workstation:app --host 127.0.0.1 --port 7860" in result.python_log
    assert "-m http.server 8080 --bind 127.0.0.1 --directory" in result.python_log
    assert runtime_config.exists()
    assert 'web: "https://platform.example.com"' in runtime_config.read_text(encoding="utf-8")


def test_deploy_docker_workstation_falls_back_when_stdout_log_is_locked(tmp_path: Path):
    release_root, _ = _prepare_release_bundle(tmp_path / "release", "workstation")
    locked_log = release_root / "runtime" / "logs" / "workstation.stdout.log"
    locked_log.parent.mkdir(parents=True, exist_ok=True)
    locked_log.write_text("still in use\n", encoding="utf-8")

    locked_log_ps = str(locked_log).replace("'", "''")
    locker = subprocess.Popen(
        [
            "pwsh",
            "-NoProfile",
            "-Command",
            (
                f"$fs = [System.IO.File]::Open('{locked_log_ps}', [System.IO.FileMode]::Open, "
                "[System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None); "
                "Start-Sleep -Seconds 20; "
                "$fs.Dispose()"
            ),
        ],
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        time.sleep(1.5)
        result = _run_deploy(tmp_path)
    finally:
        locker.terminate()
        try:
            locker.wait(timeout=5)
        except subprocess.TimeoutExpired:
            locker.kill()
            locker.wait(timeout=5)

    log_dir = release_root / "runtime" / "logs"
    rotated_logs = sorted(log_dir.glob("workstation.stdout.*.log"))

    assert result.returncode == 0, result.stderr
    assert "日志文件被占用" in result.stdout
    assert locked_log.read_text(encoding="utf-8") == "still in use\n"
    assert rotated_logs, "expected deploy-docker to fall back to a rotated stdout log file"


def test_deploy_docker_hybrid_rejects_web_base_url_without_http_scheme(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-WebBaseUrl", "127.0.0.1")

    assert result.returncode != 0
    assert "http:// 或 https://" in result.stderr


def test_deploy_docker_hybrid_rejects_web_base_url_with_local_web_flag(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-WebBaseUrl", "https://platform.example.com", "-DeployLocalWeb")

    assert result.returncode != 0
    assert "-WebBaseUrl 与 -DeployLocalWeb / -WebReleaseRoot 不能同时使用" in result.stderr


def test_deploy_docker_frontend_runs_local_static_server_without_docker(tmp_path: Path):
    result = _run_deploy(tmp_path, "frontend", "-WebBaseUrl", "https://platform.example.com")

    runtime_config = tmp_path / "release" / "services" / "frontend" / "frontend" / "dist" / "runtime-config.js"

    assert result.returncode == 0, result.stderr
    assert result.docker_log == ""
    assert "-m http.server 8080 --bind 127.0.0.1 --directory" in result.python_log
    assert runtime_config.exists()
    assert 'web: "https://platform.example.com"' in runtime_config.read_text(encoding="utf-8")


