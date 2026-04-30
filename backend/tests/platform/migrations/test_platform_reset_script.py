from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parents[3] / "tools"


def test_platform_reset_ps1_exists_and_restarts_docker_postgres_with_sql_imports() -> None:
    script_path = TOOLS_DIR / "reset_platform_test_data.ps1"

    assert script_path.exists()

    content = script_path.read_text(encoding="utf-8")
    assert "docker restart" in content
    assert "pg_isready" in content
    assert "2026-03-31-create-platform-database.sql" in content
    assert "2026-03-31-seed-platform-test-data.sql" in content
    assert "docker exec -i" in content


def test_web_database_reset_script_rebuilds_current_compose_database() -> None:
    script_path = TOOLS_DIR / "reset_web_database_with_test_data.ps1"

    assert script_path.exists()

    content = script_path.read_text(encoding="utf-8")
    assert "agentthespire-web-release" in content
    assert '"compose", "--project-name"' in content
    assert "ReleaseRoot" in content
    assert "DROP DATABASE IF EXISTS" in content
    assert "CREATE DATABASE" in content
    assert '"alembic", "upgrade", "head"' in content
    assert '"run", "--rm", "--no-deps"' in content
    assert '"stop", $WebService' in content
    assert "2026-03-31-seed-platform-test-data.sql" in content
    assert "-Yes" in content
