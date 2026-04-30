from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass
class AssetCodegenRequest:
    design_description: str
    asset_type: str
    asset_name: str
    image_paths: list[Path]
    project_root: Path
    name_zhs: str = ""
    skip_build: bool = False


@dataclass
class CustomCodegenRequest:
    description: str
    implementation_notes: str
    name: str
    project_root: Path
    skip_build: bool = False


@dataclass
class AssetGroupRequest:
    assets: list[dict]
    project_root: Path


@dataclass
class ModProjectRequest:
    project_name: str
    target_dir: Path
