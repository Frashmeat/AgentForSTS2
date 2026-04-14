from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "20260414_01_auth_user_admin_flag"
down_revision = "20260403_01_auth_user_email_ver"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.add_column(
        "users",
        sa.Column("is_admin", sa.Boolean(), nullable=False, server_default=sa.false()),
    )
    op.alter_column("users", "is_admin", server_default=None)


def downgrade() -> None:
    op.drop_column("users", "is_admin")
