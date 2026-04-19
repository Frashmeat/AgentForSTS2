from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass


@dataclass(slots=True)
class ExecutionProfileRoutingRecord:
    id: int
    agent_backend: str
    model: str
    enabled: bool


@dataclass(slots=True)
class ExecutionRoutingTargetRecord:
    execution_profile_id: int
    agent_backend: str
    model: str
    provider: str
    credential_id: int
    auth_type: str
    credential_ciphertext: str
    secret_ciphertext: str | None
    base_url: str


class ExecutionRoutingRepository(ABC):
    @abstractmethod
    def get_execution_profile(self, execution_profile_id: int) -> ExecutionProfileRoutingRecord | None:
        raise NotImplementedError

    @abstractmethod
    def find_routable_execution_target(
        self,
        execution_profile_id: int,
        *,
        excluded_credential_ids: set[int] | None = None,
    ) -> ExecutionRoutingTargetRecord | None:
        raise NotImplementedError
