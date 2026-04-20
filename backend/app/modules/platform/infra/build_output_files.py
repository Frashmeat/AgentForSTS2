from __future__ import annotations

import shutil
from dataclasses import dataclass
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


@dataclass(slots=True)
class DeployOutputsResult:
    deployed_to: str | None
    file_names: list[str]
    file_paths: list[Path]


def deploy_latest_output_files(project_root: Path, mods_root: Path, *, project_name: str | None = None) -> DeployOutputsResult:
    target_name = str(project_name).strip() or project_root.name
    target_dir = mods_root / target_name
    output_files = find_latest_output_files(project_root)
    if not output_files:
        return DeployOutputsResult(
            deployed_to=None,
            file_names=[],
            file_paths=[],
        )

    target_dir.mkdir(parents=True, exist_ok=True)
    copied_paths: list[Path] = []
    for file in output_files:
        destination = target_dir / file.name
        shutil.copy2(file, destination)
        copied_paths.append(destination)
    return DeployOutputsResult(
        deployed_to=str(target_dir),
        file_names=[file.name for file in copied_paths],
        file_paths=copied_paths,
    )
