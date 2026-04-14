from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "20260414_03_platform_job_selected_execution_profile"
down_revision = "20260414_02_platform_server_execution_basics"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.add_column("jobs", sa.Column("selected_execution_profile_id", sa.BigInteger(), nullable=True))
    op.add_column("jobs", sa.Column("selected_agent_backend", sa.String(length=32), nullable=False, server_default=""))
    op.add_column("jobs", sa.Column("selected_model", sa.String(length=128), nullable=False, server_default=""))
    op.alter_column("jobs", "selected_agent_backend", server_default=None)
    op.alter_column("jobs", "selected_model", server_default=None)


def downgrade() -> None:
    op.drop_column("jobs", "selected_model")
    op.drop_column("jobs", "selected_agent_backend")
    op.drop_column("jobs", "selected_execution_profile_id")
