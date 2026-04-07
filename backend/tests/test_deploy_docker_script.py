from __future__ import annotations

import json
import os
import subprocess
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


def _prepare_release_bundle(release_root: Path, target: str) -> tuple[Path, Path]:
    if target == "hybrid":
        (release_root / "services" / "workstation").mkdir(parents=True)
        (release_root / "services" / "frontend").mkdir(parents=True)
        compose = "services:\n  workstation:\n    image: fake\n  frontend:\n    image: fake\n"
    elif target == "web":
        (release_root / "services" / "web").mkdir(parents=True)
        compose = "services:\n  web:\n    image: fake\n"
    else:
        (release_root / "services" / "workstation").mkdir(parents=True)
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
) -> subprocess.CompletedProcess[str]:
    release_root, config_path = _prepare_release_bundle(tmp_path / "release", target)
    if prepare_default_web_release:
        _prepare_release_bundle(tmp_path / "agentthespire-web-release", "web")
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    _write_fake_docker(bin_dir)

    log_path = tmp_path / "docker.log"
    env = os.environ.copy()
    env["PATH"] = str(bin_dir) + os.pathsep + env["PATH"]
    env["MOCK_DOCKER_LOG"] = str(log_path)
    env["MOCK_DOCKER_IMAGE_EXISTS"] = "1"

    command = [
        "pwsh",
        "-NoProfile",
        "-File",
        str(SCRIPT_PATH),
        target,
        "-ReleaseRoot",
        str(release_root),
        "-ConfigPath",
        str(config_path),
        *extra_args,
    ]
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
    return completed


def test_deploy_docker_default_builds_even_when_local_image_exists(tmp_path: Path):
    result = _run_deploy(tmp_path)

    assert result.returncode == 0, result.stderr
    assert "compose build" in result.docker_log
    assert "compose up" in result.docker_log


def test_deploy_docker_reuse_images_skips_build_when_local_image_exists(tmp_path: Path):
    result = _run_deploy(tmp_path, "-ReuseImages")

    assert result.returncode == 0, result.stderr
    assert "compose build" not in result.docker_log
    assert "compose up" in result.docker_log


def test_deploy_docker_hybrid_defaults_to_local_web_base_url_and_deploys_web_release(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", prepare_default_web_release=True)
    runtime_config = tmp_path / "release" / "runtime" / "runtime-config.js"

    assert result.returncode == 0, result.stderr
    assert "compose build" in result.docker_log
    assert result.docker_log.count("compose up") == 2
    assert runtime_config.exists()
    assert 'web: "http://127.0.0.1:7870"' in runtime_config.read_text(encoding="utf-8")


def test_deploy_docker_hybrid_builds_frontend_and_workstation_without_postgres(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-WebBaseUrl", "https://platform.example.com")

    runtime_config = tmp_path / "release" / "runtime" / "runtime-config.js"

    assert result.returncode == 0, result.stderr
    assert "image inspect postgres:16-alpine" not in result.docker_log
    assert "compose build" in result.docker_log
    assert "compose up" in result.docker_log
    assert runtime_config.exists()
    assert 'web: "https://platform.example.com"' in runtime_config.read_text(encoding="utf-8")


def test_deploy_docker_hybrid_rejects_web_base_url_without_http_scheme(tmp_path: Path):
    result = _run_deploy(tmp_path, "hybrid", "-WebBaseUrl", "127.0.0.1")

    assert result.returncode != 0
    assert "http:// 或 https://" in result.stderr
