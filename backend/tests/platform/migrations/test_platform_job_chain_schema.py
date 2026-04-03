import importlib
import sys
from pathlib import Path

from sqlalchemy.schema import CreateIndex, CreateTable
from sqlalchemy.dialects import postgresql

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.shared.infra.db.base import Base


EXPECTED_TABLES = {
    "jobs",
    "job_items",
    "ai_executions",
    "execution_charges",
    "quota_accounts",
    "quota_buckets",
    "usage_ledgers",
    "artifacts",
    "job_events",
}


def _load_platform_models() -> None:
    importlib.import_module("app.modules.platform.infra.persistence.models")


def _table_ddl(table_name: str) -> str:
    table = Base.metadata.tables[table_name]
    return str(CreateTable(table).compile(dialect=postgresql.dialect()))


def _index_ddl(table_name: str, index_name: str) -> str:
    table = Base.metadata.tables[table_name]
    index = next(index for index in table.indexes if index.name == index_name)
    return str(CreateIndex(index).compile(dialect=postgresql.dialect()))


def test_platform_metadata_contains_all_platform_job_chain_tables():
    _load_platform_models()

    assert EXPECTED_TABLES.issubset(Base.metadata.tables.keys())


def test_job_chain_tables_freeze_version_fields_and_integer_quota_columns():
    _load_platform_models()

    jobs = Base.metadata.tables["jobs"]
    ai_executions = Base.metadata.tables["ai_executions"]
    execution_charges = Base.metadata.tables["execution_charges"]
    quota_buckets = Base.metadata.tables["quota_buckets"]
    usage_ledgers = Base.metadata.tables["usage_ledgers"]

    assert "workflow_version" in jobs.c
    assert "step_protocol_version" in ai_executions.c
    assert "result_schema_version" in ai_executions.c

    assert execution_charges.c["charge_amount"].type.python_type is int
    assert quota_buckets.c["quota_limit"].type.python_type is int
    assert quota_buckets.c["used_amount"].type.python_type is int
    assert quota_buckets.c["refunded_amount"].type.python_type is int
    assert usage_ledgers.c["amount"].type.python_type is int
    assert usage_ledgers.c["balance_after"].type.python_type is int


def test_platform_status_columns_compile_with_frozen_fine_grained_values():
    _load_platform_models()

    jobs_sql = _table_ddl("jobs")
    job_items_sql = _table_ddl("job_items")
    ai_executions_sql = _table_ddl("ai_executions")

    assert "'draft'" in jobs_sql
    assert "'queued'" in jobs_sql
    assert "'quota_exhausted'" in jobs_sql
    assert "'cancelled_after_start'" in job_items_sql
    assert "'quota_skipped'" in job_items_sql
    assert "'dispatching'" in ai_executions_sql
    assert "'completed_with_refund'" in ai_executions_sql


def test_platform_indexes_compile_expected_query_and_idempotency_guards():
    _load_platform_models()

    jobs_index_sql = _index_ddl("jobs", "ix_jobs_user_id_created_at_desc")
    executions_index_sql = _index_ddl(
        "ai_executions",
        "uq_ai_executions_user_item_idempotency_key",
    )
    ledgers_index_sql = _index_ddl("usage_ledgers", "ix_usage_ledgers_user_id_created_at_desc")
    events_index_sql = _index_ddl("job_events", "ix_job_events_job_id_created_at")

    assert "CREATE INDEX ix_jobs_user_id_created_at_desc" in jobs_index_sql
    assert "(user_id, created_at DESC)" in jobs_index_sql
    assert "CREATE UNIQUE INDEX uq_ai_executions_user_item_idempotency_key" in executions_index_sql
    assert "(user_id, job_item_id, request_idempotency_key)" in executions_index_sql
    assert "WHERE request_idempotency_key IS NOT NULL" in executions_index_sql
    assert "(user_id, created_at DESC)" in ledgers_index_sql
    assert "(job_id, created_at)" in events_index_sql


def test_platform_migration_script_exists_for_first_job_chain_revision():
    migration_path = (
        Path(__file__).resolve().parents[3]
        / "migrations"
        / "versions"
        / "20260331_01_platform_job_chain.py"
    )

    assert migration_path.exists()

    source = migration_path.read_text(encoding="utf-8")
    assert "def upgrade()" in source
    assert "def downgrade()" in source


def test_first_platform_revision_identifier_fits_alembic_version_limit():
    migration_path = (
        Path(__file__).resolve().parents[3]
        / "migrations"
        / "versions"
        / "20260331_01_platform_job_chain.py"
    )

    source = migration_path.read_text(encoding="utf-8")
    revision_line = next(line for line in source.splitlines() if line.startswith('revision = "'))
    revision = revision_line.split('"')[1]

    assert len(revision) <= 32
