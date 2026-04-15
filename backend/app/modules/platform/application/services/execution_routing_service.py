from __future__ import annotations

from dataclasses import dataclass

from app.modules.platform.domain.repositories import ExecutionRoutingRepository
from app.modules.platform.infra.persistence.models import JobRecord


@dataclass(slots=True)
class ResolvedExecutionRoute:
    execution_profile_id: int
    agent_backend: str
    model: str
    provider: str
    credential_ref: str
    retry_attempt: int
    switched_credential: bool
    auth_type: str
    credential_ciphertext: str
    secret_ciphertext: str | None
    base_url: str


class ExecutionRoutingService:
    def __init__(self, execution_routing_repository: ExecutionRoutingRepository) -> None:
        self.execution_routing_repository = execution_routing_repository

    def resolve_for_job(self, job: JobRecord) -> ResolvedExecutionRoute:
        execution_profile_id = job.selected_execution_profile_id
        if execution_profile_id is None:
            raise ValueError("job selected_execution_profile_id is required")

        profile = self.execution_routing_repository.get_execution_profile(execution_profile_id)
        if profile is None:
            raise LookupError(f"execution profile not found: {execution_profile_id}")
        if not profile.enabled:
            raise ValueError(f"execution profile is disabled: {execution_profile_id}")

        target = self.execution_routing_repository.find_routable_execution_target(execution_profile_id)
        if target is None:
            raise LookupError(f"no enabled healthy server credential for execution profile: {execution_profile_id}")

        return ResolvedExecutionRoute(
            execution_profile_id=execution_profile_id,
            agent_backend=target.agent_backend,
            model=target.model,
            provider=target.provider,
            credential_ref=f"server-credential:{target.credential_id}",
            retry_attempt=0,
            switched_credential=False,
            auth_type=target.auth_type,
            credential_ciphertext=target.credential_ciphertext,
            secret_ciphertext=target.secret_ciphertext,
            base_url=target.base_url,
        )
