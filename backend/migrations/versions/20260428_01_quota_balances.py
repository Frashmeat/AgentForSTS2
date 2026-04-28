from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy import inspect

from app.modules.platform.infra.persistence.models import QuotaBalanceRecord


revision = "20260428_01_quota_balances"
down_revision = "20260422_01_rt_audit_evt"
branch_labels = None
depends_on = None


def upgrade() -> None:
    bind = op.get_bind()
    QuotaBalanceRecord.__table__.create(bind=bind, checkfirst=True)
    existing_columns = {column["name"] for column in inspect(bind).get_columns("usage_ledgers")}
    if "quota_balance_id" not in existing_columns:
        op.add_column(
            "usage_ledgers",
            sa.Column(
                "quota_balance_id",
                sa.BigInteger().with_variant(sa.Integer(), "sqlite"),
                sa.ForeignKey("quota_balances.id"),
                nullable=True,
            ),
        )


def downgrade() -> None:
    bind = op.get_bind()
    existing_columns = {column["name"] for column in inspect(bind).get_columns("usage_ledgers")}
    if "quota_balance_id" in existing_columns:
        op.drop_column("usage_ledgers", "quota_balance_id")
    QuotaBalanceRecord.__table__.drop(bind=bind, checkfirst=True)
