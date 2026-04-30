from __future__ import annotations

from dataclasses import dataclass, field

from ._model import ModelBase


@dataclass(slots=True)
class ExecutionProfileView(ModelBase):
    id: int
    display_name: str
    agent_backend: str
    model: str
    description: str
    recommended: bool
    available: bool


@dataclass(slots=True)
class ExecutionProfileAdminView(ModelBase):
    id: int
    code: str
    display_name: str
    agent_backend: str
    model: str
    description: str
    enabled: bool
    recommended: bool
    sort_order: int


@dataclass(slots=True)
class CreateExecutionProfileCommand(ModelBase):
    code: str
    display_name: str
    agent_backend: str
    model: str
    description: str = ""
    enabled: bool = True
    recommended: bool = False
    sort_order: int = 0


@dataclass(slots=True)
class UpdateExecutionProfileCommand(ModelBase):
    code: str
    display_name: str
    agent_backend: str
    model: str
    description: str = ""
    enabled: bool = True
    recommended: bool = False
    sort_order: int = 0


@dataclass(slots=True)
class ExecutionProfileListView(ModelBase):
    items: list[ExecutionProfileView] = field(default_factory=list)


@dataclass(slots=True)
class UserServerPreferenceView(ModelBase):
    default_execution_profile_id: int | None
    display_name: str
    agent_backend: str
    model: str
    available: bool
    updated_at: str | None


@dataclass(slots=True)
class UpdateServerPreferenceCommand(ModelBase):
    default_execution_profile_id: int | None
