from __future__ import annotations

from pathlib import Path
from typing import Any

from agents.sts2_docs import API_REF_PATH, BASELIB_SRC_PATH
from app.modules.knowledge.infra import knowledge_runtime

_ILSPY_EXAMPLE_DLL_PATH = "<sts2_path>/data_sts2_windows_x86_64/sts2.dll"


def _has_cs_files(path: Path) -> bool:
    return path.exists() and any(path.rglob("*.cs"))


def get_game_source_info() -> dict[str, Any]:
    runtime_dir = knowledge_runtime.GAME_DECOMPILED_DIR
    if _has_cs_files(runtime_dir):
        return {
            "source_mode": "runtime_decompiled",
            "path": str(runtime_dir),
            "exists": True,
        }

    if API_REF_PATH.exists():
        return {
            "source_mode": "repo_reference",
            "path": str(API_REF_PATH),
            "exists": True,
        }

    return {
        "source_mode": "missing",
        "path": "",
        "exists": False,
    }


def get_baselib_source_info() -> dict[str, Any]:
    runtime_file = knowledge_runtime.BASELIB_DECOMPILED_DIR / "BaseLib.decompiled.cs"
    if runtime_file.exists():
        return {
            "source_mode": "runtime_decompiled",
            "path": str(runtime_file),
            "exists": True,
        }

    if Path(BASELIB_SRC_PATH).exists():
        return {
            "source_mode": "repo_fallback",
            "path": str(BASELIB_SRC_PATH),
            "exists": True,
        }

    return {
        "source_mode": "missing",
        "path": "",
        "exists": False,
    }


def build_api_lookup_context() -> dict[str, str]:
    game = get_game_source_info()
    baselib = get_baselib_source_info()

    return {
        "baselib_src_path": baselib["path"],
        "game_source_mode": game["source_mode"],
        "game_path": game["path"],
        "ilspy_example_dll_path": _ILSPY_EXAMPLE_DLL_PATH,
    }
