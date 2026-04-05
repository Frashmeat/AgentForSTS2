from __future__ import annotations

from alembic import op

from app.modules.auth.infra.persistence.models import auth_tables


revision = "20260403_01_auth_user_email_ver"
down_revision = "20260331_01_platform_job_chain"
branch_labels = None
depends_on = None


def upgrade() -> None:
    bind = op.get_bind()
    for table in auth_tables():
        table.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for table in reversed(auth_tables()):
        table.drop(bind=bind, checkfirst=True)
