from __future__ import annotations

import glob
import importlib
import subprocess
import shutil
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

project_utils = importlib.import_module("project_utils")


def test_find_godot_detects_repo_local_install(monkeypatch, tmp_path):
    repo_root = tmp_path
    exe_path = (
        repo_root
        / "godot"
        / "Godot_v4.5.1-stable_mono_win64"
        / "Godot_v4.5.1-stable_mono_win64.exe"
    )
    exe_path.parent.mkdir(parents=True)
    exe_path.write_text("", encoding="utf-8")

    monkeypatch.setattr(project_utils, "_REPO_ROOT", repo_root, raising=False)
    monkeypatch.setattr(project_utils.Path, "home", lambda: repo_root / "home")

    original_glob = glob.glob

    def fake_glob(pattern, recursive=False):
        normalized = pattern.replace("\\", "/")
        if normalized.startswith(str((repo_root / "godot").as_posix())):
            return [str(exe_path)]
        return []

    monkeypatch.setattr(glob, "glob", fake_glob)
    monkeypatch.setattr(shutil, "which", lambda _: None)

    result, note = project_utils._find_godot()

    assert result == str(exe_path)
    assert "Godot" in note


def test_pick_path_windows_does_not_force_timeout(monkeypatch):
    captured: dict[str, object] = {}

    def fake_run(*args, **kwargs):
        captured["args"] = args
        captured["kwargs"] = kwargs
        return subprocess.CompletedProcess(args[0], 0, stdout="C:\\mods\r\n", stderr="")

    monkeypatch.setattr(project_utils.subprocess, "run", fake_run)

    result = project_utils._pick_path_windows(
        kind="directory",
        title="选择默认 Mod 项目目录",
        initial_path="",
        filters=[],
    )

    assert result == "C:/mods"
    assert "timeout" not in captured["kwargs"]


def test_detect_paths_task_reports_progress_and_completion(monkeypatch):
    progress_gate = threading.Event()

    def fake_detect_paths_impl(reporter):
        reporter.set_step("扫描 Steam 注册表")
        reporter.add_note("开始扫描")
        progress_gate.set()
        reporter.set_sts2_path("E:/steam/steamapps/common/Slay the Spire 2")
        reporter.set_godot_exe_path("C:/tools/Godot.exe")
        reporter.add_note("检测完成")

    monkeypatch.setattr(project_utils, "_run_detect_paths_impl", fake_detect_paths_impl)

    task = project_utils.start_detect_paths_task()
    assert task["status"] in {"pending", "running", "completed"}

    assert progress_gate.wait(1.0)
    snapshot = project_utils.get_detect_paths_task(task["task_id"])
    assert snapshot["current_step"] in {"扫描 Steam 注册表", "检测完成"}
    assert "开始扫描" in snapshot["notes"]

    for _ in range(20):
        snapshot = project_utils.get_detect_paths_task(task["task_id"])
        if snapshot["status"] == "completed":
            break
        time.sleep(0.05)

    assert snapshot["status"] == "completed"
    assert snapshot["sts2_path"] == "E:/steam/steamapps/common/Slay the Spire 2"
    assert snapshot["godot_exe_path"] == "C:/tools/Godot.exe"
    assert snapshot["can_cancel"] is False


def test_detect_paths_task_can_be_cancelled(monkeypatch):
    entered = threading.Event()

    def fake_detect_paths_impl(reporter):
        reporter.set_step("长时间扫描")
        entered.set()
        for _ in range(50):
            if reporter.is_cancelled():
                reporter.add_note("检测已中断")
                return
            time.sleep(0.01)

    monkeypatch.setattr(project_utils, "_run_detect_paths_impl", fake_detect_paths_impl)

    task = project_utils.start_detect_paths_task()
    assert entered.wait(1.0)

    cancelled = project_utils.cancel_detect_paths_task(task["task_id"])
    assert cancelled["status"] in {"running", "cancelled"}

    for _ in range(20):
        snapshot = project_utils.get_detect_paths_task(task["task_id"])
        if snapshot["status"] == "cancelled":
            break
        time.sleep(0.05)

    assert snapshot["status"] == "cancelled"
    assert snapshot["can_cancel"] is False


def test_ensure_local_props_creates_managed_fields_from_config(monkeypatch, tmp_path):
    monkeypatch.setattr(
        "config.get_config",
        lambda: {
            "sts2_path": "E:/SteamLibrary/steamapps/common/Slay the Spire 2",
            "godot_exe_path": "C:/tools/Godot.exe",
        },
    )

    project_root = tmp_path / "MyMod"
    project_root.mkdir()

    assert project_utils.ensure_local_props(project_root) is True

    props_text = (project_root / "local.props").read_text(encoding="utf-8")
    assert "<SteamLibraryPath>E:/SteamLibrary/steamapps</SteamLibraryPath>" in props_text
    assert "<GodotPath>C:/tools/Godot.exe</GodotPath>" in props_text
    assert "STS2GamePath" not in props_text
    assert "GodotExePath" not in props_text


def test_ensure_local_props_overwrites_existing_project_with_managed_fields(monkeypatch, tmp_path):
    monkeypatch.setattr(
        "config.get_config",
        lambda: {
            "sts2_path": "E:/SteamLibrary/steamapps/common/Slay the Spire 2",
            "godot_exe_path": "F:/tools/Godot.exe",
        },
    )

    project_root = tmp_path / "MyMod"
    project_root.mkdir()
    props_path = project_root / "local.props"
    props_path.write_text(
        """<Project>
  <PropertyGroup>
    <SteamLibraryPath>C:/Program Files (x86)/Steam/steamapps</SteamLibraryPath>
    <GodotPath>C:/tools/OldGodot.exe</GodotPath>
    <STS2GamePath>legacy-path</STS2GamePath>
    <CustomFlag>true</CustomFlag>
  </PropertyGroup>
</Project>
""",
        encoding="utf-8",
    )

    assert project_utils.ensure_local_props(project_root) is True

    props_text = props_path.read_text(encoding="utf-8")
    assert "<SteamLibraryPath>E:/SteamLibrary/steamapps</SteamLibraryPath>" in props_text
    assert "<GodotPath>F:/tools/Godot.exe</GodotPath>" in props_text
    assert "STS2GamePath" not in props_text
    assert "<CustomFlag>true</CustomFlag>" not in props_text
