from __future__ import annotations

import io
import sys
import threading
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.knowledge.infra import knowledge_runtime


def _write_required_resource_docs_to_zip(archive: zipfile.ZipFile, text_prefix: str = "resource") -> None:
    for resource_path in knowledge_runtime.REQUIRED_RESOURCE_FILES:
        archive.writestr(resource_path, f"{text_prefix} {resource_path}\n")


def _write_required_resource_docs_to_dir(resource_dir: Path, text_prefix: str = "resource") -> None:
    resource_dir.mkdir(parents=True, exist_ok=True)
    for resource_path in knowledge_runtime.REQUIRED_RESOURCE_FILES:
        target = resource_dir / Path(resource_path).name
        target.write_text(f"{text_prefix} {resource_path}\n", encoding="utf-8", newline="\n")


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
        def do_GET(self) -> None:
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

        def log_message(self, format: str, *args) -> None:
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
    game_dir = tmp_path / "game"
    game_dir.mkdir(parents=True)
    (game_dir / "Game.cs").write_text("// game", encoding="utf-8")
    baselib_dir = tmp_path / "baselib"
    baselib_dir.mkdir(parents=True)
    (baselib_dir / "BaseLib.decompiled.cs").write_text("// baselib", encoding="utf-8")
    legacy_game_dir = tmp_path / "legacy" / "game"
    legacy_baselib_dir = tmp_path / "legacy" / "baselib"
    manifest = {
        "status": "fresh",
        "game": {
            "version": "0.2.14",
            "sts2_path": "C:/Steam/steamapps/common/Slay the Spire 2",
            "knowledge_path": str(legacy_game_dir),
            "decompiled_src_path": str(legacy_game_dir),
        },
        "baselib": {
            "release_tag": "v0.2.7",
            "knowledge_path": str(legacy_baselib_dir),
            "decompiled_src_path": str(legacy_baselib_dir),
        },
    }
    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: manifest)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_dir)
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
    assert status["game"]["knowledge_path"] == str(game_dir)
    assert status["baselib"]["knowledge_path"] == str(baselib_dir)


def test_refresh_task_preserves_previous_manifest_when_update_fails(monkeypatch, tmp_path: Path):
    manifest_path = tmp_path / "knowledge-manifest.json"
    manifest_path.write_text(
        '{"status":"fresh","game":{"version":"0.2.14"},"baselib":{"release_tag":"v0.2.7"}}',
        encoding="utf-8",
    )
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_MANIFEST_PATH", manifest_path)
    monkeypatch.setattr(
        knowledge_runtime,
        "load_manifest",
        lambda: {"game": {"version": "0.2.14"}, "baselib": {"release_tag": "v0.2.7"}},
    )
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


def test_runtime_knowledge_initialization_creates_empty_runtime_dirs(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    cache_dir = knowledge_root / "cache"
    packs_dir = knowledge_root / "packs"

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_CACHE_DIR", cache_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)

    knowledge_runtime.ensure_runtime_knowledge_seeded()

    assert game_runtime_dir.is_dir()
    assert baselib_runtime_dir.is_dir()
    assert resource_runtime_dir.is_dir()
    assert cache_dir.is_dir()
    assert packs_dir.is_dir()
    assert list(game_runtime_dir.iterdir()) == []
    assert list(baselib_runtime_dir.iterdir()) == []
    assert list(resource_runtime_dir.iterdir()) == []


def test_runtime_knowledge_initialization_preserves_existing_files(monkeypatch, tmp_path: Path):
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

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_CACHE_DIR", cache_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_MANIFEST_PATH", manifest_path)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir, raising=False)

    knowledge_runtime.ensure_runtime_knowledge_seeded()

    assert (game_runtime_dir / "Game.cs").read_text(encoding="utf-8") == "// user modified game"
    assert (baselib_runtime_dir / "BaseLib.decompiled.cs").read_text(encoding="utf-8") == "// user modified baselib"
    assert (resource_runtime_dir / "common.md").read_text(encoding="utf-8") == "user modified common\n"


def test_refresh_syncs_resource_templates_to_runtime(monkeypatch, tmp_path: Path):
    resource_runtime_dir = tmp_path / "runtime" / "knowledge" / "resources" / "sts2"
    template_dir = tmp_path / "templates" / "sts2"
    _write_required_resource_docs_to_dir(template_dir, "template")

    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_TEMPLATE_DIR", template_dir)

    knowledge_runtime._sync_resource_templates_to_runtime()

    assert sorted(path.name for path in resource_runtime_dir.glob("*.md")) == sorted(
        Path(resource_path).name for resource_path in knowledge_runtime.REQUIRED_RESOURCE_FILES
    )
    assert (resource_runtime_dir / "common.md").read_text(encoding="utf-8") == "template resources/sts2/common.md\n"


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


def test_missing_manifest_stays_missing_when_only_seed_files_exist(monkeypatch, tmp_path: Path):
    game_dir = tmp_path / "game"
    game_dir.mkdir(parents=True)
    baselib_dir = tmp_path / "baselib"
    baselib_dir.mkdir(parents=True)
    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: None)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_dir)
    monkeypatch.setattr(knowledge_runtime, "_has_ilspycmd", lambda: False)
    monkeypatch.setattr(
        knowledge_runtime, "get_config", lambda: {"sts2_path": "E:/steam/steamapps/common/Slay the Spire 2"}
    )

    status = knowledge_runtime.get_knowledge_status()

    assert status["status"] == "missing"
    assert status["game"]["source_mode"] == "missing"
    assert status["baselib"]["source_mode"] == "missing"
    assert status["game"]["knowledge_path"] == str(game_dir)
    assert status["baselib"]["knowledge_path"] == str(baselib_dir)


def test_manifest_runtime_missing_reports_missing_when_runtime_files_are_absent(monkeypatch, tmp_path: Path):
    game_dir = tmp_path / "game"
    game_dir.mkdir(parents=True)
    baselib_dir = tmp_path / "baselib"
    baselib_dir.mkdir(parents=True)
    manifest = {
        "generated_at": "2026-04-10T00:00:00+0800",
        "game": {
            "version": "22340209",
            "sts2_path": "E:/SteamLibrary/steamapps/common/Slay the Spire 2",
            "knowledge_path": str(game_dir),
            "decompiled_src_path": str(game_dir),
        },
        "baselib": {
            "release_tag": "v0.2.7",
            "knowledge_path": str(baselib_dir),
            "decompiled_src_path": str(baselib_dir),
        },
    }

    monkeypatch.setattr(knowledge_runtime, "load_manifest", lambda: manifest)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_dir)
    monkeypatch.setattr(
        knowledge_runtime,
        "read_current_game_version",
        lambda _path: {"version": "22340209", "source": "steam_app_manifest"},
    )
    monkeypatch.setattr(knowledge_runtime, "fetch_latest_baselib_release", lambda: {"tag_name": "v0.2.7"})

    status = knowledge_runtime.get_knowledge_status()

    assert status["status"] == "missing"
    assert status["game"]["source_mode"] == "missing"
    assert status["baselib"]["source_mode"] == "missing"
    assert status["game"]["knowledge_path"] == str(game_dir)
    assert status["baselib"]["knowledge_path"] == str(baselib_dir)


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
    assert knowledge_runtime.RUNTIME_ROOT / "tools" in knowledge_runtime._ILSPY_SEARCH_ROOTS


def test_game_decompile_uses_ilspy_project_output(monkeypatch, tmp_path: Path):
    dll_path = tmp_path / "sts2.dll"
    dll_path.write_bytes(b"dll")
    output_dir = tmp_path / "game"
    calls: list[list[str]] = []

    monkeypatch.setattr(knowledge_runtime, "resolve_ilspycmd_command", lambda: ["ilspycmd"])
    monkeypatch.setattr(knowledge_runtime, "_assert_complete_game_decompile_output", lambda _output_dir: None)

    def fake_run(command, **kwargs):
        calls.append(command)
        return type("Result", (), {"returncode": 0, "stdout": "", "stderr": ""})()

    monkeypatch.setattr(knowledge_runtime.subprocess, "run", fake_run)

    knowledge_runtime._run_ilspy_project_outputdir(dll_path, output_dir)

    assert calls == [["ilspycmd", "--nested-directories", "--project", "--outputdir", str(output_dir), str(dll_path)]]


def test_game_decompile_rejects_single_file_output(tmp_path: Path):
    output_dir = tmp_path / "game"
    output_dir.mkdir(parents=True)
    (output_dir / "sts2.decompiled.cs").write_text("// single file output\n", encoding="utf-8")

    try:
        knowledge_runtime._assert_complete_game_decompile_output(output_dir)
    except RuntimeError as exc:
        message = str(exc)
    else:
        raise AssertionError("expected RuntimeError")

    assert "游戏反编译结果不完整" in message
    assert "sts2.decompiled.cs" in message
    assert "--project --outputdir" in message


def test_knowledge_pack_upload_activate_and_rollback(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    packs_dir = knowledge_root / "packs"
    active_path = knowledge_root / "active-knowledge-pack.json"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"
    cache_dir = knowledge_root / "cache"
    archive_path = tmp_path / "pack.zip"

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)
    monkeypatch.setattr(knowledge_runtime, "ACTIVE_KNOWLEDGE_PACK_PATH", active_path)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_CACHE_DIR", cache_dir)

    with zipfile.ZipFile(archive_path, "w") as archive:
        _write_required_resource_docs_to_zip(archive, "active")
        for index in range(knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT):
            archive.writestr(f"game/Game{index}.cs", "// active game\n")
        archive.writestr("baselib/BaseLib.decompiled.cs", "// active baselib\n")

    pack = knowledge_runtime.upload_knowledge_pack_zip(
        archive_path.read_bytes(), file_name="sts2-pack.zip", label="STS2 Pack"
    )
    activated = knowledge_runtime.activate_knowledge_pack(pack["pack_id"])

    assert pack["file_count"] == knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT + len(knowledge_runtime.REQUIRED_RESOURCE_FILES) + 1
    assert pack["resource_md_count"] == len(knowledge_runtime.REQUIRED_RESOURCE_FILES)
    assert pack["game_cs_count"] == knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT
    assert pack["baselib_cs_count"] == 1
    assert pack["has_required_resources"] is True
    assert pack["has_baselib_file"] is True
    assert pack["missing_resource_files"] == []
    assert "baselib/BaseLib.decompiled.cs" in pack["files"]
    assert "game/Game0.cs" in pack["files"]
    assert "resources/sts2/common.md" in pack["files"]
    assert activated["active"] is True
    assert activated["files"] == pack["files"]
    assert knowledge_runtime.active_resource_knowledge_dir() == resource_runtime_dir
    assert knowledge_runtime.active_game_knowledge_dir() == game_runtime_dir
    assert knowledge_runtime.active_baselib_knowledge_dir() == baselib_runtime_dir
    assert (resource_runtime_dir / "common.md").read_text(encoding="utf-8") == "active resources/sts2/common.md\n"
    assert (game_runtime_dir / "Game0.cs").exists()
    assert (baselib_runtime_dir / "BaseLib.decompiled.cs").exists()

    listed = knowledge_runtime.list_knowledge_packs()
    assert listed["active_pack_id"] == pack["pack_id"]
    assert listed["items"][0]["active"] is True
    assert listed["items"][0]["files"] == pack["files"]
    assert listed["active_pack"]["files"] == pack["files"]

    rolled_back = knowledge_runtime.rollback_knowledge_pack()
    assert rolled_back["active_pack"] is None
    assert knowledge_runtime.get_active_knowledge_pack() is None
    assert list(game_runtime_dir.iterdir()) == []
    assert list(baselib_runtime_dir.iterdir()) == []
    assert list(resource_runtime_dir.iterdir()) == []


def test_export_current_knowledge_pack_zip_uses_effective_runtime_dirs(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    packs_dir = knowledge_root / "packs"
    active_path = knowledge_root / "active-knowledge-pack.json"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"
    cache_dir = knowledge_root / "cache"

    _write_required_resource_docs_to_dir(resource_runtime_dir, "current")
    game_runtime_dir.mkdir(parents=True)
    baselib_runtime_dir.mkdir(parents=True)
    for index in range(knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT):
        (game_runtime_dir / f"Game{index}.cs").write_text("// current game\n", encoding="utf-8", newline="\n")
    (baselib_runtime_dir / "BaseLib.decompiled.cs").write_text("// current baselib\n", encoding="utf-8", newline="\n")

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)
    monkeypatch.setattr(knowledge_runtime, "ACTIVE_KNOWLEDGE_PACK_PATH", active_path)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_CACHE_DIR", cache_dir)

    package = knowledge_runtime.export_current_knowledge_pack_zip()

    assert package["file_count"] == knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT + len(knowledge_runtime.REQUIRED_RESOURCE_FILES) + 1
    assert package["resource_md_count"] == len(knowledge_runtime.REQUIRED_RESOURCE_FILES)
    assert package["game_cs_count"] == knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT
    assert package["baselib_cs_count"] == 1
    assert package["has_required_resources"] is True
    assert package["has_baselib_file"] is True
    assert package["missing_resource_files"] == []
    assert "baselib/BaseLib.decompiled.cs" in package["files"]
    assert "game/Game0.cs" in package["files"]
    assert "resources/sts2/common.md" in package["files"]
    with zipfile.ZipFile(io.BytesIO(package["content"])) as archive:
        assert sorted(archive.namelist()) == package["files"]
        assert archive.read("resources/sts2/common.md").decode("utf-8") == "current resources/sts2/common.md\n"


def test_knowledge_pack_upload_rejects_incomplete_game_sources(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    packs_dir = knowledge_root / "packs"
    archive_path = tmp_path / "incomplete-pack.zip"

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)

    with zipfile.ZipFile(archive_path, "w") as archive:
        _write_required_resource_docs_to_zip(archive, "active")
        archive.writestr("game/Game.cs", "// incomplete game\n")
        archive.writestr("baselib/BaseLib.decompiled.cs", "// active baselib\n")

    try:
        knowledge_runtime.upload_knowledge_pack_zip(archive_path.read_bytes(), file_name="incomplete.zip")
    except ValueError as exc:
        message = str(exc)
    else:
        raise AssertionError("expected ValueError")

    assert "缺少完整游戏反编译源码" in message
    assert "game/**/*.cs=1" in message
    assert not packs_dir.exists() or not any(packs_dir.iterdir())


def test_knowledge_pack_upload_rejects_missing_baselib(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    packs_dir = knowledge_root / "packs"
    archive_path = tmp_path / "missing-baselib.zip"

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)

    with zipfile.ZipFile(archive_path, "w") as archive:
        _write_required_resource_docs_to_zip(archive, "active")
        for index in range(knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT):
            archive.writestr(f"game/Game{index}.cs", "// active game\n")

    try:
        knowledge_runtime.upload_knowledge_pack_zip(archive_path.read_bytes(), file_name="missing-baselib.zip")
    except ValueError as exc:
        message = str(exc)
    else:
        raise AssertionError("expected ValueError")

    assert "缺少 BaseLib 反编译源码" in message
    assert "baselib/BaseLib.decompiled.cs" in message
    assert not packs_dir.exists() or not any(packs_dir.iterdir())


def test_knowledge_pack_upload_rejects_missing_required_resources(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    packs_dir = knowledge_root / "packs"
    archive_path = tmp_path / "missing-resources.zip"

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)

    with zipfile.ZipFile(archive_path, "w") as archive:
        archive.writestr("resources/sts2/common.md", "common\n")
        for index in range(knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT):
            archive.writestr(f"game/Game{index}.cs", "// active game\n")
        archive.writestr("baselib/BaseLib.decompiled.cs", "// active baselib\n")

    try:
        knowledge_runtime.upload_knowledge_pack_zip(archive_path.read_bytes(), file_name="missing-resources.zip")
    except ValueError as exc:
        message = str(exc)
    else:
        raise AssertionError("expected ValueError")

    assert "缺少规则/摘要文档" in message
    assert "resources/sts2/card.md" in message
    assert not packs_dir.exists() or not any(packs_dir.iterdir())


def test_export_current_knowledge_pack_zip_rejects_missing_required_resources(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"

    resource_runtime_dir.mkdir(parents=True)
    game_runtime_dir.mkdir(parents=True)
    baselib_runtime_dir.mkdir(parents=True)
    (resource_runtime_dir / "common.md").write_text("common\n", encoding="utf-8", newline="\n")
    for index in range(knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT):
        (game_runtime_dir / f"Game{index}.cs").write_text("// current game\n", encoding="utf-8", newline="\n")
    (baselib_runtime_dir / "BaseLib.decompiled.cs").write_text("// current baselib\n", encoding="utf-8", newline="\n")

    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_runtime_dir)

    try:
        knowledge_runtime.export_current_knowledge_pack_zip()
    except ValueError as exc:
        message = str(exc)
    else:
        raise AssertionError("expected ValueError")

    assert "缺少规则/摘要文档" in message
    assert "resources/sts2/card.md" in message


def test_delete_active_knowledge_pack_rolls_back_to_previous(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    packs_dir = knowledge_root / "packs"
    active_path = knowledge_root / "active-knowledge-pack.json"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)
    monkeypatch.setattr(knowledge_runtime, "ACTIVE_KNOWLEDGE_PACK_PATH", active_path)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_runtime_dir)

    def make_pack(label: str) -> dict:
        archive_path = tmp_path / f"{label}.zip"
        with zipfile.ZipFile(archive_path, "w") as archive:
            _write_required_resource_docs_to_zip(archive, label)
            for index in range(knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT):
                archive.writestr(f"game/Game{index}.cs", f"// {label} game\n")
            archive.writestr("baselib/BaseLib.decompiled.cs", f"// {label} baselib\n")
        return knowledge_runtime.upload_knowledge_pack_zip(archive_path.read_bytes(), file_name=f"{label}.zip")

    first = make_pack("first")
    second = make_pack("second")
    knowledge_runtime.activate_knowledge_pack(first["pack_id"])
    knowledge_runtime.activate_knowledge_pack(second["pack_id"])

    result = knowledge_runtime.delete_knowledge_pack(second["pack_id"])

    assert result["deleted"] is True
    assert result["was_active"] is True
    assert result["active_pack_id"] == first["pack_id"]
    assert not (packs_dir / second["pack_id"]).exists()
    assert knowledge_runtime.get_active_knowledge_pack()["pack_id"] == first["pack_id"]
    assert (knowledge_root / "resources" / "sts2" / "common.md").read_text(encoding="utf-8") == "first resources/sts2/common.md\n"
    assert (knowledge_root / "game" / "Game0.cs").read_text(encoding="utf-8") == "// first game\n"


def test_delete_inactive_knowledge_pack_clears_previous_pointer(monkeypatch, tmp_path: Path):
    knowledge_root = tmp_path / "runtime" / "knowledge"
    packs_dir = knowledge_root / "packs"
    active_path = knowledge_root / "active-knowledge-pack.json"
    resource_runtime_dir = knowledge_root / "resources" / "sts2"
    game_runtime_dir = knowledge_root / "game"
    baselib_runtime_dir = knowledge_root / "baselib"

    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_ROOT", knowledge_root)
    monkeypatch.setattr(knowledge_runtime, "KNOWLEDGE_PACKS_DIR", packs_dir)
    monkeypatch.setattr(knowledge_runtime, "ACTIVE_KNOWLEDGE_PACK_PATH", active_path)
    monkeypatch.setattr(knowledge_runtime, "RESOURCE_KNOWLEDGE_DIR", resource_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "GAME_KNOWLEDGE_DIR", game_runtime_dir)
    monkeypatch.setattr(knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", baselib_runtime_dir)

    def make_pack(label: str) -> dict:
        archive_path = tmp_path / f"{label}.zip"
        with zipfile.ZipFile(archive_path, "w") as archive:
            _write_required_resource_docs_to_zip(archive, label)
            for index in range(knowledge_runtime.MIN_COMPLETE_GAME_CS_COUNT):
                archive.writestr(f"game/Game{index}.cs", f"// {label} game\n")
            archive.writestr("baselib/BaseLib.decompiled.cs", f"// {label} baselib\n")
        return knowledge_runtime.upload_knowledge_pack_zip(archive_path.read_bytes(), file_name=f"{label}.zip")

    first = make_pack("first")
    second = make_pack("second")
    knowledge_runtime.activate_knowledge_pack(first["pack_id"])
    knowledge_runtime.activate_knowledge_pack(second["pack_id"])

    result = knowledge_runtime.delete_knowledge_pack(first["pack_id"])

    assert result["deleted"] is True
    assert result["was_active"] is False
    assert result["active_pack_id"] == second["pack_id"]
    assert knowledge_runtime.get_active_knowledge_pack()["previous_pack_id"] == ""
    assert (knowledge_root / "resources" / "sts2" / "common.md").read_text(encoding="utf-8") == "second resources/sts2/common.md\n"
