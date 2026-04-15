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
            agent_backend=row.agent_backend,
            model=row.model,
            enabled=row.enabled,
        )

    def find_routable_execution_target(self, execution_profile_id: int) -> ExecutionRoutingTargetRecord | None:
        row = (
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
            .order_by(
                ServerCredentialRecord.priority.asc(),
                ServerCredentialRecord.id.asc(),
            )
            .first()
        )
        if row is None:
            return None
        profile, credential = row
        return ExecutionRoutingTargetRecord(
            execution_profile_id=profile.id,
            agent_backend=profile.agent_backend,
            model=profile.model,
            provider=credential.provider,
            credential_id=credential.id,
            auth_type=credential.auth_type,
            credential_ciphertext=credential.credential_ciphertext,
            secret_ciphertext=credential.secret_ciphertext,
            base_url=credential.base_url,
        )
