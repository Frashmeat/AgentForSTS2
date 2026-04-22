from __future__ import annotations

from pathlib import Path

from app.modules.platform.application.services.server_deploy_registry_service import ServerDeployRegistryService


def test_server_deploy_registry_service_can_write_and_read_registration(tmp_path):
    service = ServerDeployRegistryService()
    target_dir = tmp_path / "Mods" / "DarkMod"

    metadata_path = service.write_registration(
        target_dir=target_dir,
        project_name="DarkMod",
        job_id=11,
        job_item_id=22,
        user_id=1001,
        server_project_ref="server-workspace:abc123",
        source_workspace_root="I:/runtime/workspaces/abc123",
        deployed_to=str(target_dir),
        entrypoint="platform.build.project",
        file_names=["DarkMod.dll", "DarkMod.pck"],
    )

    registration = service.read_registration(target_dir)

    assert metadata_path == Path(target_dir) / ".server-deploy.json"
    assert registration is not None
    assert registration.schema_version == "v1"
    assert registration.project_name == "DarkMod"
    assert registration.job_id == 11
    assert registration.job_item_id == 22
    assert registration.user_id == 1001
    assert registration.server_project_ref == "server-workspace:abc123"
    assert registration.source_workspace_root == "I:/runtime/workspaces/abc123"
    assert registration.deployed_to == str(target_dir)
    assert registration.entrypoint == "platform.build.project"
    assert registration.file_names == ["DarkMod.dll", "DarkMod.pck"]


def test_server_deploy_registry_service_can_build_registration_payload(tmp_path):
    service = ServerDeployRegistryService()
    target_dir = tmp_path / "Mods" / "DarkMod"
    service.write_registration(
        target_dir=target_dir,
        project_name="DarkMod",
        job_id=11,
        job_item_id=22,
        user_id=1001,
        server_project_ref="server-workspace:abc123",
        source_workspace_root="I:/runtime/workspaces/abc123",
        deployed_to=str(target_dir),
        entrypoint="platform.build.project",
        file_names=["DarkMod.dll", "DarkMod.pck"],
    )

    payload = service.build_registration_payload(service.read_registration(target_dir))

    assert payload is not None
    assert payload["project_name"] == "DarkMod"
    assert payload["job_id"] == 11
    assert payload["entrypoint"] == "platform.build.project"
    assert payload["file_names"] == ["DarkMod.dll", "DarkMod.pck"]
