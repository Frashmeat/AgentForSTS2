from __future__ import annotations

from app.modules.platform.contracts.server_execution import ExecutionProfileListView, UserServerPreferenceView
from app.modules.platform.domain.repositories import ServerExecutionRepository


class ServerExecutionService:
    def __init__(self, server_execution_repository: ServerExecutionRepository) -> None:
        self.server_execution_repository = server_execution_repository

    def list_execution_profiles(self) -> ExecutionProfileListView:
        return ExecutionProfileListView(items=self.server_execution_repository.list_execution_profiles())

    def get_user_server_preference(self, user_id: int) -> UserServerPreferenceView:
        return self.server_execution_repository.get_user_server_preference(user_id)

    def set_user_server_preference(
        self,
        user_id: int,
        execution_profile_id: int | None,
    ) -> UserServerPreferenceView:
        return self.server_execution_repository.set_user_server_preference(user_id, execution_profile_id)
