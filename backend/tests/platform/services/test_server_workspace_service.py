from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.platform.application.services.server_workspace_service import ServerWorkspaceService


def test_server_workspace_service_creates_workspace_reference(tmp_path):
    service = ServerWorkspaceService(storage_root=tmp_path / "workspaces")

    workspace = service.create_workspace(user_id=1001, project_name="DarkMod")

    assert workspace.server_project_ref.startswith("server-workspace:")
    assert workspace.project_name == "DarkMod"
    assert workspace.workspace_root.endswith("DarkMod")
    assert (Path(workspace.workspace_root) / "DarkMod.csproj").exists() is True


def test_server_workspace_service_validates_ref_belongs_to_user(tmp_path):
    service = ServerWorkspaceService(storage_root=tmp_path / "workspaces")
    workspace = service.create_workspace(user_id=1001, project_name="DarkMod")

    service.ensure_accessible(user_id=1001, server_project_ref=workspace.server_project_ref)

    try:
        service.ensure_accessible(user_id=1002, server_project_ref=workspace.server_project_ref)
    except ValueError as error:
        assert str(error) == f"server workspace ref not found for user: {workspace.server_project_ref}"
    else:
        raise AssertionError("expected ValueError when server workspace ref belongs to another user")
