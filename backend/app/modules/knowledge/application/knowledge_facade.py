from __future__ import annotations

from pathlib import Path
from typing import Any

from app.modules.knowledge.infra import knowledge_runtime

_ILSPY_EXAMPLE_DLL_PATH = "<sts2_path>/data_sts2_windows_x86_64/sts2.dll"


def _has_cs_files(path: Path) -> bool:
    return path.exists() and any(path.rglob("*.cs"))


def _runtime_game_reference_path() -> Path:
    seed_file_name = Path(getattr(knowledge_runtime, "GAME_KNOWLEDGE_SEED_FILE", "sts2_api_reference.md")).name
    return knowledge_runtime.active_game_knowledge_dir() / seed_file_name


def get_game_source_info() -> dict[str, Any]:
    knowledge_runtime.ensure_runtime_knowledge_seeded()
    runtime_dir = knowledge_runtime.active_game_knowledge_dir()
    if _has_cs_files(runtime_dir):
        return {
            "source_mode": "runtime_decompiled",
            "path": str(runtime_dir),
            "exists": True,
        }

    runtime_reference = _runtime_game_reference_path()
    if runtime_reference.exists():
        return {
            "source_mode": "reference_only",
            "path": str(runtime_reference),
            "exists": True,
        }

    return {
        "source_mode": "missing",
        "path": "",
        "exists": False,
    }


def get_baselib_source_info() -> dict[str, Any]:
    knowledge_runtime.ensure_runtime_knowledge_seeded()
    runtime_file = knowledge_runtime.active_baselib_knowledge_dir() / "BaseLib.decompiled.cs"
    if runtime_file.exists():
        return {
            "source_mode": "runtime_decompiled",
            "path": str(runtime_file),
            "exists": True,
        }

    return {
        "source_mode": "missing",
        "path": "",
        "exists": False,
    }


def build_lookup_context() -> dict[str, str]:
    game = get_game_source_info()
    baselib = get_baselib_source_info()

    return {
        "baselib_src_path": baselib["path"],
        "game_source_mode": game["source_mode"],
        "game_path": game["path"],
        "ilspy_example_dll_path": _ILSPY_EXAMPLE_DLL_PATH,
    }
