from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "20260414_04_ai_exec_mode_facts"
down_revision = "20260414_03_job_exec_profile"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.add_column("ai_executions", sa.Column("credential_ref", sa.String(length=128), nullable=False, server_default=""))
    op.add_column("ai_executions", sa.Column("retry_attempt", sa.BigInteger(), nullable=False, server_default="0"))
    op.add_column("ai_executions", sa.Column("switched_credential", sa.Boolean(), nullable=False, server_default=sa.false()))
    op.alter_column("ai_executions", "credential_ref", server_default=None)
    op.alter_column("ai_executions", "retry_attempt", server_default=None)
    op.alter_column("ai_executions", "switched_credential", server_default=None)


def downgrade() -> None:
    op.drop_column("ai_executions", "switched_credential")
    op.drop_column("ai_executions", "retry_attempt")
    op.drop_column("ai_executions", "credential_ref")
