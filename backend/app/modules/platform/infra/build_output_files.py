from __future__ import annotations

from pathlib import Path


_SKIP_DIRS = {"obj", "ref", ".godot"}


def find_latest_output_files(project_root: Path) -> list[Path]:
    results: dict[str, Path] = {}
    bin_dir = project_root / "bin"
    if not bin_dir.exists():
        return []
    for suffix in (".dll", ".pck"):
        candidates = [
            file for file in bin_dir.rglob(f"*{suffix}")
            if not any(part in _SKIP_DIRS for part in file.relative_to(bin_dir).parts)
        ]
        if candidates:
            results[suffix] = max(candidates, key=lambda file: file.stat().st_mtime)
    return list(results.values())
