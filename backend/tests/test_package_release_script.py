from __future__ import annotations

import os
import subprocess
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
    (repo_root / "tools" / "latest" / "package-release.ps1").write_text(
        SCRIPT_SOURCE.read_text(encoding="utf-8"),
        encoding="utf-8",
    )
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
