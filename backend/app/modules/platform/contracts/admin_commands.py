from __future__ import annotations

from dataclasses import dataclass

from ._model import ModelBase


@dataclass(slots=True)
class CreateServerCredentialCommand(ModelBase):
    execution_profile_id: int
    provider: str
    auth_type: str
    credential: str
    secret: str = ""
    base_url: str = ""
    label: str = ""
    priority: int = 0
    enabled: bool = True


@dataclass(slots=True)
class UpdateServerCredentialCommand(ModelBase):
    execution_profile_id: int
    provider: str
    auth_type: str
    credential: str = ""
    secret: str = ""
    base_url: str = ""
    label: str = ""
    priority: int = 0
    enabled: bool = True


@dataclass(slots=True)
class AdjustUserQuotaCommand(ModelBase):
    direction: str
    amount: int
    reason: str
