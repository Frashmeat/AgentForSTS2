from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra import knowledge_runtime


def test_load_manifest_returns_none_when_file_missing(monkeypatch, tmp_path: Path):
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_MANIFEST_PATH", tmp_path / "missing.json")

    assert knowledge_runtime.load_manifest() is None


def test_select_baselib_asset_prefers_baselib_dll():
    release = {
        "tag_name": "v0.2.8",
        "published_at": "2026-04-07T20:27:01Z",
        "assets": [
            {"name": "BaseLib.0.2.8.zip", "browser_download_url": "https://example.invalid/BaseLib.0.2.8.zip"},
            {"name": "BaseLib.dll", "browser_download_url": "https://example.invalid/BaseLib.dll"},
            {"name": "BaseLib.pck", "browser_download_url": "https://example.invalid/BaseLib.pck"},
        ],
    }

    asset = knowledge_runtime.select_baselib_asset(release)

    assert asset["name"] == "BaseLib.dll"
    assert asset["browser_download_url"].endswith("/BaseLib.dll")


def test_compute_status_marks_stale_when_versions_change(monkeypatch, tmp_path: Path):
    game_dir = tmp_path / "game_decompiled"
    game_dir.mkdir(parents=True)
    (game_dir / "Game.cs").write_text("// game", encoding="utf-8")
    baselib_dir = tmp_path / "baselib_decompiled"
    baselib_dir.mkdir(parents=True)
    (baselib_dir / "BaseLib.decompiled.cs").write_text("// baselib", encoding="utf-8")
    manifest = {
        "status": "fresh",
        "game": {
            "version": "0.2.14",
            "sts2_path": "C:/Steam/steamapps/common/Slay the Spire 2",
            "decompiled_src_path": str(game_dir),
        },
        "baselib": {"release_tag": "v0.2.7", "decompiled_src_path": str(baselib_dir)},
    }
    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: manifest)
    monkeypatch.setattr(
        knowledge_runtime,
        "read_current_game_version",
        lambda game_path: {"version": "0.2.15", "source": "steam_app_manifest"},
    )
    monkeypatch.setattr(
        knowledge_runtime,
        "fetch_latest_baselib_release",
        lambda: {"tag_name": "v0.2.8", "published_at": "2026-04-07T20:27:01Z", "assets": []},
    )

    status = knowledge_runtime.get_knowledge_status()

    assert status["status"] == "stale"
    assert status["game"]["current_version"] == "0.2.15"
    assert status["baselib"]["latest_release_tag"] == "v0.2.8"
    assert status["game"]["matches"] is False
    assert status["baselib"]["matches"] is False


def test_refresh_task_preserves_previous_manifest_when_update_fails(monkeypatch, tmp_path: Path):
    manifest_path = tmp_path / "knowledge-manifest.json"
    manifest_path.write_text(
        '{"status":"fresh","game":{"version":"0.2.14"},"baselib":{"release_tag":"v0.2.7"}}',
        encoding="utf-8",
    )
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_MANIFEST_PATH", manifest_path)
    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: {"game": {"version": "0.2.14"}, "baselib": {"release_tag": "v0.2.7"}})
    monkeypatch.setattr(
        knowledge_runtime,
        "_run_refresh_impl",
        lambda task: (_ for _ in ()).throw(RuntimeError("refresh failed")),
    )

    snapshot = knowledge_runtime.start_refresh_task()
    final_snapshot = knowledge_runtime.get_refresh_task(snapshot["task_id"])

    assert final_snapshot["status"] == "failed"
    assert "refresh failed" in (final_snapshot["error"] or "")
    assert '"0.2.14"' in manifest_path.read_text(encoding="utf-8")


def test_missing_status_surfaces_missing_runtime_requirements(monkeypatch):
    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: None)
    monkeypatch.setattr(knowledge_runtime, "_has_ilspycmd", lambda: False)
    monkeypatch.setattr(knowledge_runtime, "_directory_has_sources", lambda _path: False)
    monkeypatch.setattr(
        knowledge_runtime,
        "get_config",
        lambda: {"sts2_path": ""},
    )

    status = knowledge_runtime.get_knowledge_status()

    assert status["status"] == "missing"
    assert "未配置 STS2 游戏路径，无法更新知识库" in status["warnings"]
    assert "未检测到 ilspycmd，无法反编译游戏和 BaseLib（会先查项目目录，再查 PATH）" in status["warnings"]


def test_resolve_ilspycmd_command_prefers_project_copy(monkeypatch, tmp_path: Path):
    project_copy = tmp_path / "tools" / "ilspycmd.exe"
    project_copy.parent.mkdir(parents=True, exist_ok=True)
    project_copy.write_text("stub", encoding="utf-8")

    monkeypatch.setattr(knowledge_runtime, "_ILSPY_SEARCH_ROOTS", (tmp_path,))
    monkeypatch.setenv(knowledge_runtime._ILSPY_PATH_ENV, "")
    monkeypatch.setattr(knowledge_runtime.shutil, "which", lambda _name: None)

    command = knowledge_runtime.resolve_ilspycmd_command()

    assert command == [str(project_copy.resolve())]
