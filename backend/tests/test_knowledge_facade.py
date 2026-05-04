from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.modules.knowledge.application import knowledge_facade


def test_game_source_prefers_runtime_decompiled(monkeypatch, tmp_path: Path):
    runtime_dir = tmp_path / "runtime_game"
    runtime_dir.mkdir(parents=True)
    (runtime_dir / "Game.cs").write_text("// game", encoding="utf-8")

    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "GAME_KNOWLEDGE_DIR", runtime_dir)
    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "ensure_runtime_knowledge_seeded", lambda: None)

    result = knowledge_facade.get_game_source_info()

    assert result == {
        "source_mode": "runtime_decompiled",
        "path": str(runtime_dir),
        "exists": True,
    }


def test_game_source_reports_missing_when_only_runtime_reference_exists(monkeypatch, tmp_path: Path):
    runtime_dir = tmp_path / "runtime" / "knowledge" / "game"
    runtime_dir.mkdir(parents=True)
    (runtime_dir / "sts2_api_reference.md").write_text("reference", encoding="utf-8")

    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "GAME_KNOWLEDGE_DIR", runtime_dir)
    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "ensure_runtime_knowledge_seeded", lambda: None)

    result = knowledge_facade.get_game_source_info()

    assert result == {
        "source_mode": "missing",
        "path": "",
        "exists": False,
    }


def test_baselib_source_prefers_runtime_decompiled(monkeypatch, tmp_path: Path):
    runtime_dir = tmp_path / "runtime_baselib"
    runtime_dir.mkdir(parents=True)
    runtime_file = runtime_dir / "BaseLib.decompiled.cs"
    runtime_file.write_text("// baselib", encoding="utf-8")

    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", runtime_dir)
    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "ensure_runtime_knowledge_seeded", lambda: None)

    result = knowledge_facade.get_baselib_source_info()

    assert result == {
        "source_mode": "runtime_decompiled",
        "path": str(runtime_file),
        "exists": True,
    }


def test_baselib_source_stays_missing_without_runtime_file(monkeypatch, tmp_path: Path):
    runtime_dir = tmp_path / "runtime" / "knowledge" / "baselib"

    monkeypatch.setattr(knowledge_facade.knowledge_runtime, "BASELIB_KNOWLEDGE_DIR", runtime_dir)
    monkeypatch.setattr(
        knowledge_facade.knowledge_runtime,
        "ensure_runtime_knowledge_seeded",
        lambda: runtime_dir.mkdir(parents=True, exist_ok=True),
    )

    result = knowledge_facade.get_baselib_source_info()

    assert result == {
        "source_mode": "missing",
        "path": "",
        "exists": False,
    }


def test_build_lookup_context_uses_facade_resolution(monkeypatch):
    monkeypatch.setattr(
        knowledge_facade,
        "get_game_source_info",
        lambda: {
            "source_mode": "runtime_decompiled",
            "path": "I:/release/runtime/knowledge/game/sts2_api_reference.md",
            "exists": True,
        },
    )
    monkeypatch.setattr(
        knowledge_facade,
        "get_baselib_source_info",
        lambda: {
            "source_mode": "runtime_decompiled",
            "path": "I:/release/runtime/knowledge/baselib/BaseLib.decompiled.cs",
            "exists": True,
        },
    )

    context = knowledge_facade.build_lookup_context()

    assert context["baselib_src_path"] == "I:/release/runtime/knowledge/baselib/BaseLib.decompiled.cs"
    assert context["game_source_mode"] == "runtime_decompiled"
    assert context["game_path"] == "I:/release/runtime/knowledge/game/sts2_api_reference.md"
    assert context["ilspy_example_dll_path"] == "<sts2_path>/data_sts2_windows_x86_64/sts2.dll"
