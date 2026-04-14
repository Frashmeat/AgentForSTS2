from __future__ import annotations

from abc import ABC, abstractmethod

from app.modules.platform.contracts.server_execution import ExecutionProfileView, UserServerPreferenceView


class ServerExecutionRepository(ABC):
    @abstractmethod
    def list_execution_profiles(self) -> list[ExecutionProfileView]:
        raise NotImplementedError

    @abstractmethod
    def get_user_server_preference(self, user_id: int) -> UserServerPreferenceView:
        raise NotImplementedError

    @abstractmethod
    def set_user_server_preference(
        self,
        user_id: int,
        execution_profile_id: int | None,
    ) -> UserServerPreferenceView:
        raise NotImplementedError
