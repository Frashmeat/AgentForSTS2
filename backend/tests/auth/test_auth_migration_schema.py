from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from app.modules.auth.infra.persistence import models as _auth_models  # noqa: F401
from app.shared.infra.db.base import Base


def test_auth_tables_are_registered_in_metadata():
    tables = Base.metadata.tables

    assert "users" in tables
    assert "email_verifications" in tables
    assert tables["users"].c["username"].unique is True
    assert tables["users"].c["email"].unique is True
    assert tables["email_verifications"].c["code"].unique is True


def test_initial_auth_revision_does_not_include_admin_flag():
    migration_path = (
        Path(__file__).resolve().parents[2]
        / "migrations"
        / "versions"
        / "20260403_01_auth_user_email_ver.py"
    )

    source = migration_path.read_text(encoding="utf-8")

    assert "auth_tables" not in source
    assert "is_admin" not in source
    assert "op.create_table" in source
