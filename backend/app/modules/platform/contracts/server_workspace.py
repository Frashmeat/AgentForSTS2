from __future__ import annotations

from dataclasses import dataclass

from ._model import ModelBase


@dataclass(slots=True)
class CreateServerWorkspaceCommand(ModelBase):
    project_name: str


@dataclass(slots=True)
class ServerWorkspaceView(ModelBase):
    server_project_ref: str
    project_name: str
    workspace_root: str
    created_at: str
