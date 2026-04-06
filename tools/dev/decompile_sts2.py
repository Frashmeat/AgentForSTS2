#!/usr/bin/env python3
"""
decompile_sts2.py — 将 sts2.dll 反编译到本地目录，供 AgentTheSpire code agent 使用。

用法:
    python tools/dev/decompile_sts2.py --game-path "C:/Steam/steamapps/common/Slay the Spire 2"
    python tools/dev/decompile_sts2.py --dll-path "C:/path/to/sts2.dll"
    python tools/dev/decompile_sts2.py --game-path "..." --output "E:/my_decompiled"
    python tools/dev/decompile_sts2.py  # 默认读取 config.json 中的 sts2_path

运行后将输出目录写入 config.json 的 decompiled_src_path 字段。

依赖: ilspycmd (dotnet tool)
    dotnet tool install -g ilspycmd
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

# 默认输出路径（与 config.json 同级，不在仓库内）
_DEFAULT_OUTPUT = Path(__file__).parent.parent.parent / "sts2_decompiled"
_CONFIG_PATH = Path(__file__).parent.parent.parent / "config.json"

_STS2_DLL_RELATIVE = "data_sts2_windows_x86_64/sts2.dll"


def load_config() -> dict:
    if not _CONFIG_PATH.exists():
        return {}
    with open(_CONFIG_PATH, "r", encoding="utf-8-sig") as f:
        return json.load(f)


def find_dll(game_path: str | None, dll_path: str | None) -> Path:
    if dll_path:
        p = Path(dll_path)
        if not p.exists():
            sys.exit(f"[ERROR] DLL not found: {p}")
        return p

    resolved_game_path = game_path
    if not resolved_game_path:
        resolved_game_path = str(load_config().get("sts2_path", "")).strip()

    if resolved_game_path:
        p = Path(resolved_game_path) / _STS2_DLL_RELATIVE
        if not p.exists():
            sys.exit(f"[ERROR] sts2.dll not found at {p}")
        return p

    if game_path:
        sys.exit(f"[ERROR] sts2.dll not found at {Path(game_path) / _STS2_DLL_RELATIVE}")
    sys.exit("[ERROR] Provide --game-path or --dll-path, or set config.json.sts2_path")


def run_decompile(dll: Path, output: Path) -> None:
    output.mkdir(parents=True, exist_ok=True)
    print(f"[INFO] Decompiling {dll} → {output} ...")
    print("[INFO] This may take 1-3 minutes for a 100MB+ DLL.")
    result = subprocess.run(
        ["ilspycmd", str(dll), "--outputdir", str(output)],
        capture_output=False,
    )
    if result.returncode != 0:
        sys.exit(f"[ERROR] ilspycmd failed (exit {result.returncode}). Is it installed?\n"
                 "  dotnet tool install -g ilspycmd")
    cs_files = list(output.rglob("*.cs"))
    print(f"[OK] Decompiled {len(cs_files)} .cs files to {output}")


def write_config(output: Path) -> None:
    cfg = load_config()
    cfg["decompiled_src_path"] = str(output)
    with open(_CONFIG_PATH, "w", encoding="utf-8") as f:
        json.dump(cfg, f, indent=2, ensure_ascii=False)
    print(f"[OK] config.json updated: decompiled_src_path = {output}")


def main() -> None:
    parser = argparse.ArgumentParser(description="Decompile sts2.dll for AgentTheSpire")
    parser.add_argument("--game-path", help="STS2 game root directory")
    parser.add_argument("--dll-path", help="Direct path to sts2.dll")
    parser.add_argument("--output", default=str(_DEFAULT_OUTPUT),
                        help=f"Output directory (default: {_DEFAULT_OUTPUT})")
    args = parser.parse_args()

    dll = find_dll(args.game_path, args.dll_path)
    output = Path(args.output)
    run_decompile(dll, output)
    write_config(output)
    print("\n[DONE] AgentTheSpire code agent will now use the decompiled source for API lookup.")
    print(f"       Path: {output}")
    print("       Restart the backend to apply.")


if __name__ == "__main__":
    main()

