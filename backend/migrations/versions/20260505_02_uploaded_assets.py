from __future__ import annotations

import sqlalchemy as sa
from alembic import op

from app.modules.platform.infra.persistence.models import UploadedAssetRecord


revision = "20260505_02_uploaded_assets"
down_revision = "20260505_01_exec_config_fields"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.alter_column(
        "artifacts",
        "object_key",
        existing_type=sa.String(length=256),
        type_=sa.String(length=512),
        existing_nullable=False,
    )
    bind = op.get_bind()
    UploadedAssetRecord.__table__.create(bind=bind, checkfirst=True)


def downgrade() -> None:
    bind = op.get_bind()
    UploadedAssetRecord.__table__.drop(bind=bind, checkfirst=True)
    op.alter_column(
        "artifacts",
        "object_key",
        existing_type=sa.String(length=512),
        type_=sa.String(length=256),
        existing_nullable=False,
    )
