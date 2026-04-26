from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy import inspect


revision = "20260414_03_job_exec_profile"
down_revision = "20260414_02_platform_exec_basics"
branch_labels = None
depends_on = None


def upgrade() -> None:
    existing_columns = {column["name"] for column in inspect(op.get_bind()).get_columns("jobs")}
    if "selected_execution_profile_id" not in existing_columns:
        op.add_column("jobs", sa.Column("selected_execution_profile_id", sa.BigInteger(), nullable=True))
    if "selected_agent_backend" not in existing_columns:
        op.add_column("jobs", sa.Column("selected_agent_backend", sa.String(length=32), nullable=False, server_default=""))
        op.alter_column("jobs", "selected_agent_backend", server_default=None)
    if "selected_model" not in existing_columns:
        op.add_column("jobs", sa.Column("selected_model", sa.String(length=128), nullable=False, server_default=""))
        op.alter_column("jobs", "selected_model", server_default=None)


def downgrade() -> None:
    op.drop_column("jobs", "selected_model")
    op.drop_column("jobs", "selected_agent_backend")
    op.drop_column("jobs", "selected_execution_profile_id")
