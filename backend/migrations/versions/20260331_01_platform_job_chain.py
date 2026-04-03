from __future__ import annotations

from alembic import op

from app.modules.platform.infra.persistence.models import platform_tables


revision = "20260331_01_platform_job_chain"
down_revision = None
branch_labels = None
depends_on = None


def upgrade() -> None:
    bind = op.get_bind()
    for table in platform_tables():
        table.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for table in reversed(platform_tables()):
        table.drop(bind=bind, checkfirst=True)
