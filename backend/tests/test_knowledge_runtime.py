from __future__ import annotations

import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
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


def test_select_baselib_asset_reports_friendly_error_when_no_matching_dll():
    release = {
        "tag_name": "v0.3.1",
        "assets": [
            {"name": "BaseLib-linux-x64.zip", "browser_download_url": "https://example.invalid/BaseLib-linux-x64.zip"},
            {"name": "BaseLib.pdb", "browser_download_url": "https://example.invalid/BaseLib.pdb"},
            {"name": "manifest.json", "browser_download_url": "https://example.invalid/manifest.json"},
        ],
    }

    try:
        knowledge_runtime.select_baselib_asset(release)
    except RuntimeError as exc:
        message = str(exc)
    else:
        raise AssertionError("expected RuntimeError")

    assert "v0.3.1" in message
    assert "BaseLib-linux-x64.zip" in message
    assert "BaseLib.pdb" in message
    assert "manifest.json" in message
    assert "只接受 .dll" in message
    assert "优先 BaseLib.dll" in message
    assert "其次文件名包含 baselib" in message


def test_download_file_follows_github_style_redirect(tmp_path: Path):
    target = tmp_path / "cache" / "BaseLib.dll"
    payload = b"baselib-binary"

    class RedirectHandler(BaseHTTPRequestHandler):
        def do_GET(self) -> None:  # noqa: N802
            if self.path == "/releases/download/BaseLib.dll":
                self.send_response(302)
                self.send_header("Location", "/github-asset/BaseLib.dll")
                self.end_headers()
                return

            if self.path == "/github-asset/BaseLib.dll":
                self.send_response(200)
                self.send_header("Content-Type", "application/octet-stream")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return

            self.send_response(404)
            self.end_headers()

        def log_message(self, format: str, *args) -> None:  # noqa: A003
            return

    server = ThreadingHTTPServer(("127.0.0.1", 0), RedirectHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()

    try:
        knowledge_runtime._download_file(
            f"http://127.0.0.1:{server.server_port}/releases/download/BaseLib.dll",
            target,
        )
    finally:
        server.shutdown()
        thread.join(timeout=3)
        server.server_close()

    assert target.read_bytes() == payload


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


def test_runtime_knowledge_seed_initialization_copies_repo_sources(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    cache_dir = knowledge_root / "cache"
    manifest_path = knowledge_root / "knowledge-manifest.json"

    game_seed_dir = tmp_path / "seed" / "game"
    game_seed_dir.mkdir(parents=True)
    (game_seed_dir / "Game.cs").write_text("// runtime game seed", encoding="utf-8")
    baselib_seed_file = tmp_path / "seed" / "BaseLib.decompiled.cs"
    baselib_seed_file.parent.mkdir(parents=True, exist_ok=True)
    baselib_seed_file.write_text("// runtime baselib seed", encoding="utf-8")
    resource_seed_dir = tmp_path / "seed" / "resources" / "sts2"
    resource_seed_dir.mkdir(parents=True)
    (resource_seed_dir / "common.md").write_text("runtime common seed\n", encoding="utf-8")

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "GAME_DECOMPILED_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_DECOMPILED_DIR", baselib_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_CACHE_DIR", cache_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_MANIFEST_PATH", manifest_path)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_SEED_DIR", game_seed_dir, raising=False)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_SEED_FILE", baselib_seed_file, raising=False)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir, raising=False)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_SEED_DIR", resource_seed_dir, raising=False)

    knowledge_runtime.ensure_runtime_knowledge_seeded()

    assert (game_runtime_dir / "Game.cs").read_text(encoding="utf-8") == "// runtime game seed"
    assert (baselib_runtime_dir / "BaseLib.decompiled.cs").read_text(encoding="utf-8") == "// runtime baselib seed"
    assert (resource_runtime_dir / "common.md").read_text(encoding="utf-8") == "runtime common seed\n"


def test_runtime_knowledge_seed_initialization_preserves_user_changes(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    cache_dir = knowledge_root / "cache"
    manifest_path = knowledge_root / "knowledge-manifest.json"

    game_runtime_dir.mkdir(parents=True)
    (game_runtime_dir / "Game.cs").write_text("// user modified game", encoding="utf-8")
    baselib_runtime_dir.mkdir(parents=True)
    (baselib_runtime_dir / "BaseLib.decompiled.cs").write_text("// user modified baselib", encoding="utf-8")
    resource_runtime_dir.mkdir(parents=True)
    (resource_runtime_dir / "common.md").write_text("user modified common\n", encoding="utf-8")

    game_seed_dir = tmp_path / "seed" / "game"
    game_seed_dir.mkdir(parents=True)
    (game_seed_dir / "Game.cs").write_text("// original game seed", encoding="utf-8")
    baselib_seed_file = tmp_path / "seed" / "BaseLib.decompiled.cs"
    baselib_seed_file.parent.mkdir(parents=True, exist_ok=True)
    baselib_seed_file.write_text("// original baselib seed", encoding="utf-8")
    resource_seed_dir = tmp_path / "seed" / "resources" / "sts2"
    resource_seed_dir.mkdir(parents=True)
    (resource_seed_dir / "common.md").write_text("original common seed\n", encoding="utf-8")

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "GAME_DECOMPILED_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_DECOMPILED_DIR", baselib_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_CACHE_DIR", cache_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_MANIFEST_PATH", manifest_path)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_SEED_DIR", game_seed_dir, raising=False)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_SEED_FILE", baselib_seed_file, raising=False)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir, raising=False)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_SEED_DIR", resource_seed_dir, raising=False)

    knowledge_runtime.ensure_runtime_knowledge_seeded()

    assert (game_runtime_dir / "Game.cs").read_text(encoding="utf-8") == "// user modified game"
    assert (baselib_runtime_dir / "BaseLib.decompiled.cs").read_text(encoding="utf-8") == "// user modified baselib"
    assert (resource_runtime_dir / "common.md").read_text(encoding="utf-8") == "user modified common\n"


def test_missing_status_surfaces_missing_runtime_requirements(monkeypatch):
    missing_repo_ref = Path("Z:/missing/sts2_api_reference.md")
    missing_baselib = Path("Z:/missing/BaseLib.decompiled.cs")
    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: None)
    monkeypatch.setattr(knowledge_runtime, "_has_ilspycmd", lambda: False)
    monkeypatch.setattr(knowledge_runtime, "_directory_has_sources", lambda _path: False)
    monkeypatch.setattr(knowledge_runtime, "API_REF_PATH", missing_repo_ref)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_FALLBACK_PATH", missing_baselib)
    monkeypatch.setattr(
        knowledge_runtime,
        "get_config",
        lambda: {"sts2_path": ""},
    )

    status = knowledge_runtime.get_knowledge_status()

    assert status["status"] == "missing"
    assert "未配置 STS2 游戏路径，无法更新知识库" in status["warnings"]
    assert "未检测到 ilspycmd，无法反编译游戏和 BaseLib（会先查项目目录，再查 PATH）" in status["warnings"]


def test_missing_manifest_stays_missing_when_only_seed_files_exist(monkeypatch, tmp_path: Path):
    game_dir = tmp_path / "game_decompiled"
    game_dir.mkdir(parents=True)
    baselib_dir = tmp_path / "baselib_decompiled"
    baselib_dir.mkdir(parents=True)
    repo_ref = tmp_path / "sts2_api_reference.md"
    repo_ref.write_text("reference", encoding="utf-8")
    repo_baselib = tmp_path / "BaseLib.decompiled.cs"
    repo_baselib.write_text("// baselib", encoding="utf-8")

    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: None)
    monkeypatch.setattr(knowledge_runtime, "GAME_DECOMPILED_DIR", game_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_DECOMPILED_DIR", baselib_dir)
    monkeypatch.setattr(knowledge_runtime, "API_REF_PATH", repo_ref)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_FALLBACK_PATH", repo_baselib)
    monkeypatch.setattr(knowledge_runtime, "_has_ilspycmd", lambda: False)
    monkeypatch.setattr(knowledge_runtime, "get_config", lambda: {"sts2_path": "E:/steam/steamapps/common/Slay the Spire 2"})

    status = knowledge_runtime.get_knowledge_status()

    assert status["status"] == "missing"
    assert status["game"]["source_mode"] == "missing"
    assert status["baselib"]["source_mode"] == "missing"


def test_manifest_runtime_missing_reports_missing_when_runtime_files_are_absent(monkeypatch, tmp_path: Path):
    game_dir = tmp_path / "game_decompiled"
    game_dir.mkdir(parents=True)
    baselib_dir = tmp_path / "baselib_decompiled"
    baselib_dir.mkdir(parents=True)
    repo_ref = tmp_path / "sts2_api_reference.md"
    repo_ref.write_text("reference", encoding="utf-8")
    repo_baselib = tmp_path / "BaseLib.decompiled.cs"
    repo_baselib.write_text("// baselib", encoding="utf-8")
    manifest = {
        "generated_at": "2026-04-10T00:00:00+0800",
        "game": {
            "version": "22340209",
            "sts2_path": "E:/SteamLibrary/steamapps/common/Slay the Spire 2",
            "decompiled_src_path": str(game_dir),
        },
        "baselib": {
            "release_tag": "v0.2.7",
            "decompiled_src_path": str(baselib_dir),
        },
    }

    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: manifest)
    monkeypatch.setattr(knowledge_runtime, "GAME_DECOMPILED_DIR", game_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_DECOMPILED_DIR", baselib_dir)
    monkeypatch.setattr(knowledge_runtime, "API_REF_PATH", repo_ref)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_FALLBACK_PATH", repo_baselib)
    monkeypatch.setattr(knowledge_runtime, "read_current_game_version", lambda _path: {"version": "22340209", "source": "steam_app_manifest"})
    monkeypatch.setattr(knowledge_runtime, "fetch_latest_baselib_release", lambda: {"tag_name": "v0.2.7"})

    status = knowledge_runtime.get_knowledge_status()

    assert status["status"] == "missing"
    assert status["game"]["source_mode"] == "missing"
    assert status["baselib"]["source_mode"] == "missing"


def test_resolve_ilspycmd_command_prefers_project_copy(monkeypatch, tmp_path: Path):
    project_copy = tmp_path / "tools" / "ilspycmd.exe"
    project_copy.parent.mkdir(parents=True, exist_ok=True)
    project_copy.write_text("stub", encoding="utf-8")

    monkeypatch.setattr(knowledge_runtime, "_ILSPY_SEARCH_ROOTS", (tmp_path,))
    monkeypatch.setenv(knowledge_runtime._ILSPY_PATH_ENV, "")
    monkeypatch.setattr(knowledge_runtime.shutil, "which", lambda _name: None)

    command = knowledge_runtime.resolve_ilspycmd_command()

    assert command == [str(project_copy.resolve())]


def test_ilspy_search_roots_include_runtime_tools_next_to_runtime_config():
    assert knowledge_runtime.settings_module.RUNTIME_CONFIG_PATH.parent / "tools" in knowledge_runtime._ILSPY_SEARCH_ROOTS
