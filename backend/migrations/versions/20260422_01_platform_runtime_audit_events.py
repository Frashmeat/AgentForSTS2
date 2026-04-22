from __future__ import annotations

from alembic import op

from app.modules.platform.infra.persistence.models import PlatformRuntimeAuditEventRecord


revision = "20260422_01_rt_audit_evt"
down_revision = "20260414_04_ai_exec_mode_facts"
branch_labels = None
depends_on = None


def upgrade() -> None:
    bind = op.get_bind()
    PlatformRuntimeAuditEventRecord.__table__.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    PlatformRuntimeAuditEventRecord.__table__.drop(bind=bind, checkfirst=True)
