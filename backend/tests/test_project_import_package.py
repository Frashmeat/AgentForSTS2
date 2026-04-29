from __future__ import annotations

import base64
import sys
import zipfile
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from routers.workflow import _api_import_project_package


def _zip_base64(path: Path, members: dict[str, str]) -> str:
    with zipfile.ZipFile(path, "w") as archive:
        for name, content in members.items():
            archive.writestr(name, content)
    return base64.b64encode(path.read_bytes()).decode("ascii")


@pytest.mark.asyncio
async def test_import_project_package_extracts_zip_to_target_project(tmp_path: Path):
    package_base64 = _zip_base64(
        tmp_path / "GeneratedMod.source.zip",
        {
            "GeneratedMod.csproj": "<Project />\n",
            "localization/eng/cards.json": "{}\n",
        },
    )

    result = await _api_import_project_package(
        {
            "package_base64": package_base64,
            "file_name": "GeneratedMod.source.zip",
            "target_dir": str(tmp_path / "projects"),
            "project_name": "GeneratedMod",
        }
    )

    project_root = Path(result["project_path"])
    assert (project_root / "GeneratedMod.csproj").read_text(encoding="utf-8") == "<Project />\n"
    assert (project_root / "localization" / "eng" / "cards.json").read_text(encoding="utf-8") == "{}\n"


@pytest.mark.asyncio
async def test_import_project_package_rejects_unsafe_zip_paths(tmp_path: Path):
    package_base64 = _zip_base64(tmp_path / "unsafe.zip", {"../escape.txt": "bad"})

    result = await _api_import_project_package(
        {
            "package_base64": package_base64,
            "target_dir": str(tmp_path / "projects"),
            "project_name": "GeneratedMod",
        }
    )

    assert result["error"] == "unsafe package path: ../escape.txt"
    assert not (tmp_path / "projects" / "GeneratedMod").exists()
