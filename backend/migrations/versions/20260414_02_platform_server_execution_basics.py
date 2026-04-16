from __future__ import annotations

from alembic import op

from app.modules.platform.infra.persistence.models import (
    CredentialHealthCheckRecord,
    ExecutionProfileRecord,
    ServerCredentialRecord,
    UserPlatformPreferenceRecord,
)


revision = "20260414_02_platform_exec_basics"
down_revision = "20260414_01_auth_user_admin_flag"
branch_labels = None
depends_on = None


def upgrade() -> None:
    bind = op.get_bind()
    for table in (
        ExecutionProfileRecord.__table__,
        ServerCredentialRecord.__table__,
        CredentialHealthCheckRecord.__table__,
        UserPlatformPreferenceRecord.__table__,
    ):
        table.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for table in reversed(
        (
            ExecutionProfileRecord.__table__,
            ServerCredentialRecord.__table__,
            CredentialHealthCheckRecord.__table__,
            UserPlatformPreferenceRecord.__table__,
        )
    ):
        table.drop(bind=bind, checkfirst=True)
