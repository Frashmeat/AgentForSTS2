from __future__ import annotations

from datetime import UTC, datetime

from sqlalchemy import inspect
from sqlalchemy.orm import Session

from app.modules.platform.contracts.server_execution import ExecutionProfileView, UserServerPreferenceView
from app.modules.platform.domain.repositories import ServerExecutionRepository
from app.modules.platform.infra.persistence.models import (
    ExecutionProfileRecord,
    ServerCredentialRecord,
    UserPlatformPreferenceRecord,
)


def _to_iso(value: object | None) -> str | None:
    if value is None:
        return None
    if isinstance(value, datetime):
        if value.tzinfo is None:
            value = value.replace(tzinfo=UTC)
        else:
            value = value.astimezone(UTC)
    return value.isoformat()


class ServerExecutionRepositorySqlAlchemy(ServerExecutionRepository):
    DEFAULT_EXECUTION_PROFILES = (
        {
            "code": "codex-gpt-5-4",
            "display_name": "Codex CLI / gpt-5.4",
            "agent_backend": "codex",
            "model": "gpt-5.4",
            "description": "适合复杂代码修改，默认推荐。",
            "enabled": True,
            "recommended": True,
            "sort_order": 10,
        },
        {
            "code": "claude-cli-claude-sonnet-4-6",
            "display_name": "Claude CLI / claude-sonnet-4-6",
            "agent_backend": "claude",
            "model": "claude-sonnet-4-6",
            "description": "适合通用代码生成与分析。",
            "enabled": True,
            "recommended": False,
            "sort_order": 20,
        },
    )

    def __init__(self, session: Session) -> None:
        self.session = session

    def ensure_default_execution_profiles_seeded(self) -> None:
        bind = self.session.get_bind()
        if bind is None:
            return
        if not inspect(bind).has_table("execution_profiles"):
            return
        existing_codes = {
            row.code
            for row in self.session.query(ExecutionProfileRecord.code).all()
        }
        for payload in self.DEFAULT_EXECUTION_PROFILES:
            if payload["code"] in existing_codes:
                continue
            self.session.add(ExecutionProfileRecord(**payload))
        self.session.flush()

    def list_execution_profiles(self) -> list[ExecutionProfileView]:
        rows = (
            self.session.query(ExecutionProfileRecord)
            .filter(ExecutionProfileRecord.enabled.is_(True))
            .order_by(
                ExecutionProfileRecord.recommended.desc(),
                ExecutionProfileRecord.sort_order.asc(),
                ExecutionProfileRecord.id.asc(),
            )
            .all()
        )
        return [self._to_execution_profile_view(row) for row in rows]

    def get_user_server_preference(self, user_id: int) -> UserServerPreferenceView:
        preference = (
            self.session.query(UserPlatformPreferenceRecord)
            .filter(UserPlatformPreferenceRecord.user_id == user_id)
            .one_or_none()
        )
        if preference is None or preference.default_execution_profile_id is None:
            return UserServerPreferenceView(
                default_execution_profile_id=None,
                display_name="",
                agent_backend="",
                model="",
                available=False,
                updated_at=None,
            )

        profile = (
            self.session.query(ExecutionProfileRecord)
            .filter(ExecutionProfileRecord.id == preference.default_execution_profile_id)
            .one_or_none()
        )
        if profile is None:
            return UserServerPreferenceView(
                default_execution_profile_id=preference.default_execution_profile_id,
                display_name="",
                agent_backend="",
                model="",
                available=False,
                updated_at=_to_iso(preference.updated_at),
            )

        return UserServerPreferenceView(
            default_execution_profile_id=profile.id,
            display_name=profile.display_name,
            agent_backend=profile.agent_backend,
            model=profile.model,
            available=self._is_profile_available(profile.id),
            updated_at=_to_iso(preference.updated_at),
        )

    def set_user_server_preference(
        self,
        user_id: int,
        execution_profile_id: int | None,
    ) -> UserServerPreferenceView:
        if execution_profile_id is not None:
            profile = (
                self.session.query(ExecutionProfileRecord)
                .filter(
                    ExecutionProfileRecord.id == execution_profile_id,
                    ExecutionProfileRecord.enabled.is_(True),
                )
                .one_or_none()
            )
            if profile is None:
                raise LookupError(f"execution profile not found: {execution_profile_id}")
            if not self._is_profile_available(profile.id):
                raise ValueError(f"execution profile is not available: {execution_profile_id}")

        preference = (
            self.session.query(UserPlatformPreferenceRecord)
            .filter(UserPlatformPreferenceRecord.user_id == user_id)
            .one_or_none()
        )
        if preference is None:
            preference = UserPlatformPreferenceRecord(
                user_id=user_id,
                default_execution_profile_id=execution_profile_id,
            )
            self.session.add(preference)
        else:
            preference.default_execution_profile_id = execution_profile_id
        self.session.flush()
        return self.get_user_server_preference(user_id)

    def _to_execution_profile_view(self, row: ExecutionProfileRecord) -> ExecutionProfileView:
        return ExecutionProfileView(
            id=row.id,
            display_name=row.display_name,
            agent_backend=row.agent_backend,
            model=row.model,
            description=row.description,
            recommended=row.recommended,
            available=self._is_profile_available(row.id),
        )

    def _is_profile_available(self, execution_profile_id: int) -> bool:
        return (
            self.session.query(ServerCredentialRecord)
            .filter(
                ServerCredentialRecord.execution_profile_id == execution_profile_id,
                ServerCredentialRecord.enabled.is_(True),
                ServerCredentialRecord.health_status == "healthy",
            )
            .first()
            is not None
        )
