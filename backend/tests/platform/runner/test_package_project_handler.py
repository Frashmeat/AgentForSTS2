from __future__ import annotations

import asyncio
import sys
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.contracts.runner_contracts import StepExecutionRequest
from app.modules.platform.application.services.platform_file_storage import normalize_object_key
from app.modules.platform.runner.package_project_handler import execute_package_project_step


def test_execute_package_project_step_zips_source_project_without_build_outputs(tmp_path):
    project_root = tmp_path / "DarkMod"
    project_root.mkdir()
    (project_root / "DarkMod.csproj").write_text("<Project />\n", encoding="utf-8")
    (project_root / "src").mkdir()
    (project_root / "src" / "Generated.cs").write_text("// generated\n", encoding="utf-8")
    (project_root / "bin").mkdir()
    (project_root / "bin" / "ignored.dll").write_text("binary\n", encoding="utf-8")
    (project_root / "obj").mkdir()
    (project_root / "obj" / "ignored.cache").write_text("cache\n", encoding="utf-8")

    result = asyncio.run(
        execute_package_project_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="package.project",
                step_id="single.relic.package",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "FangedGrimoire",
                    "server_workspace_root": str(project_root),
                },
            )
        )
    )

    assert result["text"] == "已打包 FangedGrimoire 的服务器项目源码"
    artifact = result["artifacts"][0]
    assert artifact["artifact_type"] == "source_project"
    assert artifact["file_name"] == "DarkMod.source.zip"
    with zipfile.ZipFile(str(artifact["object_key"])) as archive:
        names = archive.namelist()
    assert "DarkMod.csproj" in names
    assert "src/Generated.cs" in names
    assert "bin/ignored.dll" not in names
    assert "obj/ignored.cache" not in names


def test_execute_package_project_step_uses_platform_object_key_for_shared_workspace(tmp_path, monkeypatch):
    platform_root = tmp_path / "platform"
    project_root = platform_root / "workspaces" / "1001" / "abc123" / "DarkMod"
    project_root.mkdir(parents=True)
    (project_root / "DarkMod.csproj").write_text("<Project />\n", encoding="utf-8")
    monkeypatch.setattr(
        "app.modules.platform.application.services.platform_file_storage.default_platform_storage_root",
        lambda: platform_root,
    )

    result = asyncio.run(
        execute_package_project_step(
            StepExecutionRequest(
                workflow_version="2026.03.31",
                step_protocol_version="v1",
                step_type="package.project",
                step_id="single.relic.package",
                job_id=1,
                job_item_id=2,
                result_schema_version="v1",
                input_payload={
                    "item_name": "FangedGrimoire",
                    "server_workspace_object_key": normalize_object_key("workspaces", 1001, "abc123", "DarkMod"),
                },
            )
        )
    )

    artifact = result["artifacts"][0]
    assert artifact["storage_provider"] == "platform_fs"
    assert artifact["object_key"] == "workspaces/1001/abc123/_source_artifacts/DarkMod.source.zip"
