from pathlib import Path
import sys


sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.windows_installer.builder import (  # type: ignore[attr-defined]
    BuildPaths,
    _write_iexpress_sed,
    normalize_runtime_requirements,
    render_iexpress_sed,
    render_install_bat,
    render_start_bat,
)


def test_normalize_runtime_requirements_replaces_gpu_and_drops_dev_only_lines():
    raw = [
        "fastapi==0.115.0",
        "rembg[gpu]>=2.0.62",
        "pytest>=8.0",
        "pytest-asyncio>=0.23",
        "",
        "# keep comments",
        "uvicorn[standard]==0.30.6",
    ]

    assert normalize_runtime_requirements(raw) == [
        "fastapi==0.115.0",
        "rembg>=2.0.62",
        "# keep comments",
        "uvicorn[standard]==0.30.6",
    ]


def test_render_scripts_and_iexpress_manifest_include_expected_paths():
    start_bat = render_start_bat(port=7860, app_subdir="app")
    install_bat = render_install_bat(app_dir_name="AgentTheSpireWorkstation")
    sed = render_iexpress_sed(
        package_name="AgentTheSpire Workstation",
        output_exe=Path("dist/AgentTheSpire-Workstation-Setup.exe"),
        install_command="install-workstation.bat",
        source_files=["install-workstation.bat", "start-workstation.bat", "app\\backend\\main_workstation.py"],
    )

    assert "http://127.0.0.1:7860" in start_bat
    assert 'set "APP_ROOT=%~dp0app"' in start_bat
    assert "%LocalAppData%\\AgentTheSpireWorkstation" in install_bat
    assert "Expand-Archive" in install_bat
    assert "app-payload.zip" in install_bat
    assert "TargetName=dist\\AgentTheSpire-Workstation-Setup.exe" in sed
    assert "AppLaunched=cmd /c install-workstation.bat" in sed
    assert "FILE0=install-workstation.bat" in sed
    assert "FILE2=app\\backend\\main_workstation.py" in sed


def test_write_iexpress_sed_only_references_archived_payload(tmp_path: Path):
    artifacts_root = tmp_path / "artifacts"
    payload_root = artifacts_root / "payload"
    dist_root = artifacts_root / "dist"
    cache_root = artifacts_root / "cache"
    release_root = tmp_path / "release"
    payload_root.mkdir(parents=True)
    dist_root.mkdir(parents=True)
    cache_root.mkdir(parents=True)
    release_root.mkdir(parents=True)

    (payload_root / "install-workstation.bat").write_text("@echo off\n", encoding="utf-8")
    (payload_root / "start-workstation.bat").write_text("@echo off\n", encoding="utf-8")
    (payload_root / "README.txt").write_text("readme\n", encoding="utf-8")
    (payload_root / "app-payload.zip").write_text("zip\n", encoding="utf-8")
    (payload_root / "app" / "backend").mkdir(parents=True)
    (payload_root / "app" / "backend" / "main_workstation.py").write_text("print('x')\n", encoding="utf-8")

    paths = BuildPaths(
        repo_root=tmp_path,
        release_root=release_root,
        artifacts_root=artifacts_root,
        cache_root=cache_root,
        payload_root=payload_root,
        dist_root=dist_root,
        output_exe=dist_root / "AgentTheSpire-Workstation-Setup.exe",
        sed_path=artifacts_root / "workstation-installer.sed",
        requirements_path=artifacts_root / "runtime-requirements.txt",
    )

    _write_iexpress_sed(paths)

    sed = paths.sed_path.read_text(encoding="utf-8")
    assert "app-payload.zip" in sed
    assert "app\\backend\\main_workstation.py" not in sed
