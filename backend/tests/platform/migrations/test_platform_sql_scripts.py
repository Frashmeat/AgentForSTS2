from pathlib import Path


SQL_DIR = Path(__file__).resolve().parents[3] / "migrations" / "sql"


def test_platform_schema_sql_exists_and_contains_database_and_table_bootstrap() -> None:
    schema_sql = SQL_DIR / "2026-03-31-create-platform-database.sql"

    assert schema_sql.exists()

    content = schema_sql.read_text(encoding="utf-8")
    assert "CREATE DATABASE agent_the_spire_platform" in content
    assert "\\connect agent_the_spire_platform" in content
    assert "CREATE TABLE IF NOT EXISTS jobs" in content
    assert "CREATE TABLE IF NOT EXISTS ai_executions" in content
    assert "CREATE UNIQUE INDEX IF NOT EXISTS uq_ai_executions_user_item_idempotency_key" in content


def test_platform_seed_sql_exists_and_contains_minimal_end_to_end_fixture() -> None:
    seed_sql = SQL_DIR / "2026-03-31-seed-platform-test-data.sql"

    assert seed_sql.exists()

    content = seed_sql.read_text(encoding="utf-8")
    assert "INSERT INTO quota_accounts" in content
    assert "INSERT INTO jobs" in content
    assert "INSERT INTO job_items" in content
    assert "INSERT INTO ai_executions" in content
    assert "INSERT INTO job_events" in content
