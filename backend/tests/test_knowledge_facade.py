from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.knowledge.application import knowledge_facade


def test_game_source_prefers_runtime_decompiled(monkeypatch, tmp_path: Path):
    runtime_dir = tmp_path / "runtime_game"
    runtime_dir.mkdir(parents=True)
    (runtime_dir / "Game.cs").write_text("// game", encoding="utf-8")
    repo_ref = tmp_path / "sts2_api_reference.md"
    repo_ref.write_text("reference", encoding="utf-8")

    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "GAME_DECOMPILED_DIR", runtime_dir)
    monkeypatch.setattr(knowledge_facade, "API_REF_PATH", repo_ref)

    result = knowledge_facade.get_game_source_info()

    assert result == {
        "source_mode": "runtime_decompiled",
        "path": str(runtime_dir),
        "exists": True,
    }


def test_game_source_falls_back_to_repo_reference(monkeypatch, tmp_path: Path):
    runtime_dir = tmp_path / "runtime_game"
    runtime_dir.mkdir(parents=True)
    repo_ref = tmp_path / "sts2_api_reference.md"
    repo_ref.write_text("reference", encoding="utf-8")

    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "GAME_DECOMPILED_DIR", runtime_dir)
    monkeypatch.setattr(knowledge_facade, "API_REF_PATH", repo_ref)

    result = knowledge_facade.get_game_source_info()

    assert result == {
        "source_mode": "repo_reference",
        "path": str(repo_ref),
        "exists": True,
    }


def test_baselib_source_prefers_runtime_decompiled(monkeypatch, tmp_path: Path):
    runtime_dir = tmp_path / "runtime_baselib"
    runtime_dir.mkdir(parents=True)
    runtime_file = runtime_dir / "BaseLib.decompiled.cs"
    runtime_file.write_text("// baselib", encoding="utf-8")
    repo_file = tmp_path / "BaseLib.decompiled.cs"
    repo_file.write_text("// repo baselib", encoding="utf-8")

    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "BASELIB_DECOMPILED_DIR", runtime_dir)
    monkeypatch.setattr(knowledge_facade, "BASELIB_SRC_PATH", repo_file)

    result = knowledge_facade.get_baselib_source_info()

    assert result == {
        "source_mode": "runtime_decompiled",
        "path": str(runtime_file),
        "exists": True,
    }


def test_build_api_lookup_context_uses_facade_resolution(monkeypatch):
    monkeypatch.setattr(
        knowledge_facade,
        "get_game_source_info",
        lambda: {"source_mode": "repo_reference", "path": "I:/repo/sts2_api_reference.md", "exists": True},
    )
    monkeypatch.setattr(
        knowledge_facade,
        "get_baselib_source_info",
        lambda: {"source_mode": "repo_fallback", "path": "I:/repo/BaseLib.decompiled.cs", "exists": True},
    )

    context = knowledge_facade.build_api_lookup_context()

    assert context["baselib_src_path"] == "I:/repo/BaseLib.decompiled.cs"
    assert context["game_source_mode"] == "repo_reference"
    assert context["game_path"] == "I:/repo/sts2_api_reference.md"
    assert context["ilspy_example_dll_path"] == "<STS2GamePath>/data_sts2_windows_x86_64/sts2.dll"
