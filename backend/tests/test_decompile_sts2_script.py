from __future__ import annotations

import importlib.util
import json
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT_PATH = REPO_ROOT / "tools" / "dev" / "decompile_sts2.py"


def _load_module():
    spec = importlib.util.spec_from_file_location("decompile_sts2_test", SCRIPT_PATH)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def test_find_dll_uses_sts2_path_from_config_when_args_are_missing(tmp_path: Path):
    module = _load_module()
    game_root = tmp_path / "sts2"
    dll_path = game_root / "data_sts2_windows_x86_64" / "sts2.dll"
    dll_path.parent.mkdir(parents=True)
    dll_path.write_text("fake dll", encoding="utf-8")

    config_path = tmp_path / "config.json"
    config_path.write_text(json.dumps({"sts2_path": str(game_root)}), encoding="utf-8-sig")
    module._CONFIG_PATH = config_path

    resolved = module.find_dll(None, None)

    assert resolved == dll_path


def test_find_dll_reports_config_hint_when_no_path_is_available(tmp_path: Path):
    module = _load_module()
    module._CONFIG_PATH = tmp_path / "missing-config.json"

    try:
        module.find_dll(None, None)
    except SystemExit as exc:
        assert "config.json.sts2_path" in str(exc)
    else:
        raise AssertionError("expected SystemExit when dll path cannot be resolved")
