from __future__ import annotations

from alembic import op

from app.modules.platform.infra.persistence.models import (
    AIExecutionRecord,
    ArtifactRecord,
    ExecutionChargeRecord,
    JobEventRecord,
    JobItemRecord,
    JobRecord,
    QuotaAccountRecord,
    QuotaBalanceRecord,
    QuotaBucketRecord,
    UsageLedgerRecord,
)


revision = "20260331_01_platform_job_chain"
down_revision = None
branch_labels = None
depends_on = None


def _platform_job_chain_tables():
    return (
        JobRecord.__table__,
        JobItemRecord.__table__,
        AIExecutionRecord.__table__,
        ExecutionChargeRecord.__table__,
        QuotaAccountRecord.__table__,
        QuotaBalanceRecord.__table__,
        QuotaBucketRecord.__table__,
        UsageLedgerRecord.__table__,
        ArtifactRecord.__table__,
        JobEventRecord.__table__,
    )


def upgrade() -> None:
    bind = op.get_bind()
    for table in _platform_job_chain_tables():
        table.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    for table in reversed(_platform_job_chain_tables()):
        table.drop(bind=bind, checkfirst=True)
