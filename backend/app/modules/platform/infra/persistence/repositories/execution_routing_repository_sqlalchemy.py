from __future__ import annotations

from app.modules.platform.domain.repositories import (
    ExecutionProfileRoutingRecord,
    ExecutionRoutingRepository,
    ExecutionRoutingTargetRecord,
)
from app.modules.platform.infra.persistence.models import ExecutionProfileRecord, ServerCredentialRecord


class ExecutionRoutingRepositorySqlAlchemy(ExecutionRoutingRepository):
    def __init__(self, session) -> None:
        self.session = session

    def get_execution_profile(self, execution_profile_id: int) -> ExecutionProfileRoutingRecord | None:
        row = (
            self.session.query(ExecutionProfileRecord)
            .filter(ExecutionProfileRecord.id == execution_profile_id)
            .one_or_none()
        )
        if row is None:
            return None
        return ExecutionProfileRoutingRecord(
            id=row.id,
            runner_type=row.runner_type,
            model=row.model,
            enabled=row.enabled,
        )

    def find_routable_execution_target(
        self,
        execution_profile_id: int,
        *,
        excluded_credential_ids: set[int] | None = None,
    ) -> ExecutionRoutingTargetRecord | None:
        query = (
            self.session.query(ExecutionProfileRecord, ServerCredentialRecord)
            .join(
                ServerCredentialRecord,
                ServerCredentialRecord.execution_profile_id == ExecutionProfileRecord.id,
            )
            .filter(
                ExecutionProfileRecord.id == execution_profile_id,
                ServerCredentialRecord.enabled.is_(True),
                ServerCredentialRecord.health_status == "healthy",
            )
        )
        if excluded_credential_ids:
            query = query.filter(ServerCredentialRecord.id.notin_(excluded_credential_ids))
        row = query.order_by(
            ServerCredentialRecord.priority.asc(),
            ServerCredentialRecord.id.asc(),
        ).first()
        if row is None:
            return None
        profile, credential = row
        return ExecutionRoutingTargetRecord(
            execution_profile_id=profile.id,
            runner_type=profile.runner_type,
            model=profile.model,
            api_protocol=credential.api_protocol,
            credential_id=credential.id,
            auth_type=credential.auth_type,
            credential_ciphertext=credential.credential_ciphertext,
            secret_ciphertext=credential.secret_ciphertext,
            api_base_url=credential.api_base_url,
        )
