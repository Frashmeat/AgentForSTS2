from __future__ import annotations

import os
import subprocess
import sys
import time
import zipfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_SOURCE = REPO_ROOT / "tools" / "latest" / "package-release.ps1"


def _write_fake_git(bin_dir: Path) -> None:
    (bin_dir / "git.cmd").write_text(
        "@echo off\r\n"
        "if \"%~1\"==\"-C\" shift\r\n"
        "if \"%~1\"==\"%CD%\" shift\r\n"
        "if \"%~1\"==\"rev-parse\" (\r\n"
        "  echo abc1234\r\n"
        "  exit /b 0\r\n"
        ")\r\n"
        "echo unsupported git args %*\r\n"
        "exit /b 1\r\n",
        encoding="utf-8",
    )


def _write_fake_robocopy(bin_dir: Path) -> None:
    (bin_dir / "robocopy.cmd").write_text(
        "@echo off\r\n"
        "exit /b 1\r\n",
        encoding="utf-8",
    )


def _prepare_common_layout(repo_root: Path) -> None:
    (repo_root / "tools" / "latest" / "templates").mkdir(parents=True, exist_ok=True)
    (repo_root / "tools" / "split-local").mkdir(parents=True, exist_ok=True)
    (repo_root / "tools" / "latest" / "package-release.ps1").write_text(
        SCRIPT_SOURCE.read_text(encoding="utf-8"),
        encoding="utf-8",
    )
    for name in (
        "start_split_local.ps1",
        "start_split_local.bat",
        "stop_split_local.ps1",
        "stop_split_local.bat",
    ):
        (repo_root / "tools" / "split-local" / name).write_text("echo launcher\n", encoding="utf-8")
    (repo_root / "README.md").write_text("repo readme\n", encoding="utf-8")


def _write_template_files(repo_root: Path, target: str) -> None:
    templates_root = repo_root / "tools" / "latest" / "templates"
    (templates_root / f"compose.{target}.yml").write_text("services:\n", encoding="utf-8")

    if target == "frontend":
        template_root = templates_root / "frontend"
        template_root.mkdir(parents=True, exist_ok=True)
        (template_root / "Dockerfile").write_text("FROM nginx:alpine\n", encoding="utf-8")
        (template_root / ".dockerignore").write_text("node_modules\n", encoding="utf-8")
        (template_root / "nginx.conf").write_text("events {}\nhttp {}\n", encoding="utf-8")
    elif target == "web":
        template_root = templates_root / "web"
        template_root.mkdir(parents=True, exist_ok=True)
        (template_root / "Dockerfile").write_text("FROM python:3.11-alpine\n", encoding="utf-8")
        (template_root / ".dockerignore").write_text("__pycache__\n", encoding="utf-8")
    elif target == "workstation":
        template_root = templates_root / "workstation"
        template_root.mkdir(parents=True, exist_ok=True)
        (template_root / "Dockerfile").write_text("FROM python:3.11-alpine\n", encoding="utf-8")
        (template_root / ".dockerignore").write_text("__pycache__\n", encoding="utf-8")
    else:
        raise AssertionError(f"unexpected target: {target}")


def _run_package_release(temp_repo: Path, target: str, *extra_args: str) -> subprocess.CompletedProcess[str]:
    bin_dir = temp_repo / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    _write_fake_git(bin_dir)
    _write_fake_robocopy(bin_dir)

    env = os.environ.copy()
    env["PATH"] = str(bin_dir) + os.pathsep + env["PATH"]

    return subprocess.run(
        [
            "pwsh",
            "-NoProfile",
            "-File",
            str(temp_repo / "tools" / "latest" / "package-release.ps1"),
            target,
            *extra_args,
        ],
        cwd=temp_repo,
        env=env,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )


def test_package_release_frontend_does_not_require_backend_directory(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "frontend")

    frontend_dist = temp_repo / "frontend" / "dist"
    frontend_dist.mkdir(parents=True, exist_ok=True)
    (frontend_dist / "index.html").write_text("<html></html>\n", encoding="utf-8")

    completed = _run_package_release(temp_repo, "frontend", "-NoFrontend", "-NoZip")

    assert completed.returncode == 0, completed.stderr
    assert "缺少backend 目录" not in completed.stderr
    assert (temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-frontend-release").exists()


def test_package_release_web_does_not_require_frontend_directory(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "web")

    backend_dir = temp_repo / "backend"
    backend_dir.mkdir(parents=True, exist_ok=True)
    (backend_dir / "main.py").write_text("print('ok')\n", encoding="utf-8")
    (temp_repo / "config.example.json").write_text("{}\n", encoding="utf-8")

    completed = _run_package_release(temp_repo, "web", "-NoZip")

    assert completed.returncode == 0, completed.stderr
    assert "缺少frontend 目录" not in completed.stderr
    assert (temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-web-release").exists()


def test_package_release_debug_reuses_previous_workstation_config(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "workstation")

    backend_dir = temp_repo / "backend"
    backend_dir.mkdir(parents=True, exist_ok=True)
    (backend_dir / "main_workstation.py").write_text("print('ok')\n", encoding="utf-8")
    frontend_dist = temp_repo / "frontend" / "dist"
    frontend_dist.mkdir(parents=True, exist_ok=True)
    (frontend_dist / "index.html").write_text("<html></html>\n", encoding="utf-8")
    mod_template = temp_repo / "mod_template"
    mod_template.mkdir(parents=True, exist_ok=True)
    (mod_template / "README.md").write_text("template\n", encoding="utf-8")
    (temp_repo / "config.example.json").write_text('{"mode":"example"}\n', encoding="utf-8")

    previous_config = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release" / "runtime" / "workstation.config.json"
    previous_config.parent.mkdir(parents=True, exist_ok=True)
    previous_config.write_text('{"mode":"debug"}\n', encoding="utf-8")

    completed = _run_package_release(temp_repo, "workstation", "-NoFrontend", "-NoZip", "-Debug")

    assert completed.returncode == 0, completed.stderr
    generated_config = previous_config
    assert generated_config.read_text(encoding="utf-8").strip() == '{"mode":"debug"}'


def test_package_release_without_debug_resets_workstation_config_to_example(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "workstation")

    backend_dir = temp_repo / "backend"
    backend_dir.mkdir(parents=True, exist_ok=True)
    (backend_dir / "main_workstation.py").write_text("print('ok')\n", encoding="utf-8")
    frontend_dist = temp_repo / "frontend" / "dist"
    frontend_dist.mkdir(parents=True, exist_ok=True)
    (frontend_dist / "index.html").write_text("<html></html>\n", encoding="utf-8")
    mod_template = temp_repo / "mod_template"
    mod_template.mkdir(parents=True, exist_ok=True)
    (mod_template / "README.md").write_text("template\n", encoding="utf-8")
    (temp_repo / "config.example.json").write_text('{"mode":"example"}\n', encoding="utf-8")

    previous_config = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release" / "runtime" / "workstation.config.json"
    previous_config.parent.mkdir(parents=True, exist_ok=True)
    previous_config.write_text('{"mode":"debug"}\n', encoding="utf-8")

    completed = _run_package_release(temp_repo, "workstation", "-NoFrontend", "-NoZip")

    assert completed.returncode == 0, completed.stderr
    generated_config = previous_config
    assert generated_config.read_text(encoding="utf-8").strip() == '{"mode":"example"}'


def test_package_release_no_longer_writes_service_level_workstation_config(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "workstation")

    backend_dir = temp_repo / "backend"
    backend_dir.mkdir(parents=True, exist_ok=True)
    (backend_dir / "main_workstation.py").write_text("print('ok')\n", encoding="utf-8")
    frontend_dist = temp_repo / "frontend" / "dist"
    frontend_dist.mkdir(parents=True, exist_ok=True)
    (frontend_dist / "index.html").write_text("<html></html>\n", encoding="utf-8")
    mod_template = temp_repo / "mod_template"
    mod_template.mkdir(parents=True, exist_ok=True)
    (mod_template / "README.md").write_text("template\n", encoding="utf-8")
    (temp_repo / "config.example.json").write_text('{"mode":"example"}\n', encoding="utf-8")

    completed = _run_package_release(temp_repo, "workstation", "-NoFrontend", "-NoZip")

    runtime_config = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release" / "runtime" / "workstation.config.json"
    service_config = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release" / "services" / "workstation" / "config.json"

    assert completed.returncode == 0, completed.stderr
    assert runtime_config.exists()
    assert not service_config.exists()


def test_package_release_seeds_runtime_knowledge_directory(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "workstation")

    backend_dir = temp_repo / "backend"
    agents_dir = backend_dir / "agents"
    resources_dir = backend_dir / "app" / "modules" / "knowledge" / "resources" / "sts2"
    backend_dir.mkdir(parents=True, exist_ok=True)
    agents_dir.mkdir(parents=True, exist_ok=True)
    resources_dir.mkdir(parents=True, exist_ok=True)
    (backend_dir / "main_workstation.py").write_text("print('ok')\n", encoding="utf-8")
    (agents_dir / "sts2_api_reference.md").write_text("api reference\n", encoding="utf-8")
    (agents_dir / "baselib_src").mkdir(parents=True, exist_ok=True)
    (agents_dir / "baselib_src" / "BaseLib.decompiled.cs").write_text("// baselib\n", encoding="utf-8")
    (resources_dir / "common.md").write_text("runtime common\n", encoding="utf-8")
    (resources_dir / "card.md").write_text("card\n", encoding="utf-8")
    (resources_dir / "relic.md").write_text("relic\n", encoding="utf-8")
    (resources_dir / "power.md").write_text("power\n", encoding="utf-8")
    (resources_dir / "potion.md").write_text("potion\n", encoding="utf-8")
    (resources_dir / "character.md").write_text("character\n", encoding="utf-8")
    (resources_dir / "custom_code.md").write_text("custom\n", encoding="utf-8")
    (resources_dir / "planner_hints.md").write_text("planner\n", encoding="utf-8")

    frontend_dist = temp_repo / "frontend" / "dist"
    frontend_dist.mkdir(parents=True, exist_ok=True)
    (frontend_dist / "index.html").write_text("<html></html>\n", encoding="utf-8")
    mod_template = temp_repo / "mod_template"
    mod_template.mkdir(parents=True, exist_ok=True)
    (mod_template / "README.md").write_text("template\n", encoding="utf-8")
    (temp_repo / "config.example.json").write_text('{"mode":"example"}\n', encoding="utf-8")

    completed = _run_package_release(temp_repo, "workstation", "-NoFrontend", "-NoZip")

    release_dir = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release"
    runtime_knowledge_dir = release_dir / "runtime" / "knowledge"

    assert completed.returncode == 0, completed.stderr
    assert (runtime_knowledge_dir / "game" / "sts2_api_reference.md").exists()
    assert (runtime_knowledge_dir / "baselib" / "BaseLib.decompiled.cs").exists()
    assert (runtime_knowledge_dir / "resources" / "sts2" / "common.md").exists()


def test_package_release_preserves_locked_runtime_logs(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "workstation")

    backend_dir = temp_repo / "backend"
    backend_dir.mkdir(parents=True, exist_ok=True)
    (backend_dir / "main_workstation.py").write_text("print('ok')\n", encoding="utf-8")
    frontend_dist = temp_repo / "frontend" / "dist"
    frontend_dist.mkdir(parents=True, exist_ok=True)
    (frontend_dist / "index.html").write_text("<html></html>\n", encoding="utf-8")
    mod_template = temp_repo / "mod_template"
    mod_template.mkdir(parents=True, exist_ok=True)
    (mod_template / "README.md").write_text("template\n", encoding="utf-8")
    (temp_repo / "config.example.json").write_text('{"mode":"example"}\n', encoding="utf-8")

    release_dir = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release"
    locked_log = release_dir / "runtime" / "logs" / "workstation.stderr.log"
    locked_log.parent.mkdir(parents=True, exist_ok=True)
    locked_log.write_text("still in use\n", encoding="utf-8")
    stale_file = release_dir / "stale.txt"
    stale_file.write_text("stale\n", encoding="utf-8")

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
        cwd=temp_repo,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        time.sleep(1.5)
        completed = _run_package_release(temp_repo, "workstation", "-NoFrontend", "-NoZip")
    finally:
        locker.terminate()
        try:
            locker.wait(timeout=5)
        except subprocess.TimeoutExpired:
            locker.kill()
            locker.wait(timeout=5)

    assert completed.returncode == 0, completed.stderr
    assert locked_log.exists()
    assert not stale_file.exists()


def test_package_release_preserves_runtime_python_cache(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "workstation")

    backend_dir = temp_repo / "backend"
    backend_dir.mkdir(parents=True, exist_ok=True)
    (backend_dir / "main_workstation.py").write_text("print('ok')\n", encoding="utf-8")
    frontend_dist = temp_repo / "frontend" / "dist"
    frontend_dist.mkdir(parents=True, exist_ok=True)
    (frontend_dist / "index.html").write_text("<html></html>\n", encoding="utf-8")
    mod_template = temp_repo / "mod_template"
    mod_template.mkdir(parents=True, exist_ok=True)
    (mod_template / "README.md").write_text("template\n", encoding="utf-8")
    (temp_repo / "config.example.json").write_text('{"mode":"example"}\n', encoding="utf-8")

    release_dir = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release"
    runtime_cache_marker = release_dir / "runtime" / "python-runtime" / "workstation" / "cache.marker"
    runtime_cache_marker.parent.mkdir(parents=True, exist_ok=True)
    runtime_cache_marker.write_text("cached\n", encoding="utf-8")
    stale_file = release_dir / "stale.txt"
    stale_file.write_text("stale\n", encoding="utf-8")

    completed = _run_package_release(temp_repo, "workstation", "-NoFrontend", "-NoZip")

    assert completed.returncode == 0, completed.stderr
    assert runtime_cache_marker.exists()
    assert not stale_file.exists()


def test_package_release_zip_excludes_runtime_python_cache(tmp_path: Path):
    temp_repo = tmp_path / "repo"
    _prepare_common_layout(temp_repo)
    _write_template_files(temp_repo, "workstation")

    backend_dir = temp_repo / "backend"
    backend_dir.mkdir(parents=True, exist_ok=True)
    (backend_dir / "main_workstation.py").write_text("print('ok')\n", encoding="utf-8")
    frontend_dist = temp_repo / "frontend" / "dist"
    frontend_dist.mkdir(parents=True, exist_ok=True)
    (frontend_dist / "index.html").write_text("<html></html>\n", encoding="utf-8")
    mod_template = temp_repo / "mod_template"
    mod_template.mkdir(parents=True, exist_ok=True)
    (mod_template / "README.md").write_text("template\n", encoding="utf-8")
    (temp_repo / "config.example.json").write_text('{"mode":"example"}\n', encoding="utf-8")

    release_dir = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release"
    runtime_cache_marker = release_dir / "runtime" / "python-runtime" / "workstation" / "cache.marker"
    runtime_cache_marker.parent.mkdir(parents=True, exist_ok=True)
    runtime_cache_marker.write_text("cached\n", encoding="utf-8")

    completed = _run_package_release(temp_repo, "workstation", "-NoFrontend")

    zip_path = temp_repo / "tools" / "latest" / "artifacts" / "agentthespire-workstation-release.zip"

    assert completed.returncode == 0, completed.stderr
    assert runtime_cache_marker.exists()
    assert zip_path.exists()

    with zipfile.ZipFile(zip_path) as archive:
        names = archive.namelist()

    assert "agentthespire-workstation-release/runtime/python-runtime/workstation/cache.marker" not in names
