from __future__ import annotations

import glob
import importlib
import shutil
import sys
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
