from __future__ import annotations

import sqlalchemy as sa
from alembic import op
from sqlalchemy import inspect


revision = "20260505_01_exec_config_fields"
down_revision = "20260428_01_quota_balances"
branch_labels = None
depends_on = None


def _columns(table_name: str) -> set[str]:
    return {column["name"] for column in inspect(op.get_bind()).get_columns(table_name)}


def _rename_or_add(
    table_name: str,
    old_name: str,
    new_name: str,
    column_type: sa.TypeEngine,
    default: str = "",
) -> None:
    columns = _columns(table_name)
    if new_name in columns:
        return
    if old_name in columns:
        op.alter_column(
            table_name,
            old_name,
            new_column_name=new_name,
            existing_type=column_type,
            existing_nullable=False,
        )
        return
    op.add_column(table_name, sa.Column(new_name, column_type, nullable=False, server_default=default))
    op.alter_column(table_name, new_name, server_default=None)


def _map_protocol_values(table_name: str) -> None:
    op.execute(
        sa.text(
            f"""
            UPDATE {table_name}
            SET api_protocol = CASE
                WHEN api_protocol = 'openai' THEN 'openai_compatible'
                WHEN api_protocol = 'anthropic' THEN 'anthropic_compatible'
                ELSE api_protocol
            END
            """
        )
    )


def upgrade() -> None:
    _rename_or_add("jobs", "selected_agent_backend", "selected_runner_type", sa.String(length=32))
    op.execute(
        sa.text(
            """
            UPDATE jobs
            SET selected_runner_type = CASE
                WHEN selected_runner_type = 'codex' THEN 'codex_cli'
                WHEN selected_runner_type = 'claude' THEN 'claude_cli'
                ELSE selected_runner_type
            END
            """
        )
    )

    _rename_or_add("execution_profiles", "agent_backend", "runner_type", sa.String(length=32), "codex_cli")
    op.execute(
        sa.text(
            """
            UPDATE execution_profiles
            SET runner_type = CASE
                WHEN runner_type = 'codex' THEN 'codex_cli'
                WHEN runner_type = 'claude' THEN 'claude_cli'
                ELSE runner_type
            END
            """
        )
    )

    _rename_or_add("server_credentials", "provider", "api_protocol", sa.String(length=64), "openai_compatible")
    _rename_or_add("server_credentials", "base_url", "api_base_url", sa.String(length=255))
    _map_protocol_values("server_credentials")

    _rename_or_add("ai_executions", "provider", "api_protocol", sa.String(length=64), "openai_compatible")
    _map_protocol_values("ai_executions")


def downgrade() -> None:
    columns = _columns("ai_executions")
    if "provider" not in columns and "api_protocol" in columns:
        op.execute(
            sa.text(
                """
                UPDATE ai_executions
                SET api_protocol = CASE
                    WHEN api_protocol = 'openai_compatible' THEN 'openai'
                    WHEN api_protocol = 'anthropic_compatible' THEN 'anthropic'
                    ELSE api_protocol
                END
                """
            )
        )
        op.alter_column(
            "ai_executions",
            "api_protocol",
            new_column_name="provider",
            existing_type=sa.String(length=64),
            existing_nullable=False,
        )

    columns = _columns("server_credentials")
    if "base_url" not in columns and "api_base_url" in columns:
        op.alter_column(
            "server_credentials",
            "api_base_url",
            new_column_name="base_url",
            existing_type=sa.String(length=255),
            existing_nullable=False,
        )
    columns = _columns("server_credentials")
    if "provider" not in columns and "api_protocol" in columns:
        op.execute(
            sa.text(
                """
                UPDATE server_credentials
                SET api_protocol = CASE
                    WHEN api_protocol = 'openai_compatible' THEN 'openai'
                    WHEN api_protocol = 'anthropic_compatible' THEN 'anthropic'
                    ELSE api_protocol
                END
                """
            )
        )
        op.alter_column(
            "server_credentials",
            "api_protocol",
            new_column_name="provider",
            existing_type=sa.String(length=64),
            existing_nullable=False,
        )

    columns = _columns("execution_profiles")
    if "agent_backend" not in columns and "runner_type" in columns:
        op.execute(
            sa.text(
                """
                UPDATE execution_profiles
                SET runner_type = CASE
                    WHEN runner_type = 'codex_cli' THEN 'codex'
                    WHEN runner_type = 'claude_cli' THEN 'claude'
                    ELSE runner_type
                END
                """
            )
        )
        op.alter_column(
            "execution_profiles",
            "runner_type",
            new_column_name="agent_backend",
            existing_type=sa.String(length=32),
            existing_nullable=False,
        )

    columns = _columns("jobs")
    if "selected_agent_backend" not in columns and "selected_runner_type" in columns:
        op.execute(
            sa.text(
                """
                UPDATE jobs
                SET selected_runner_type = CASE
                    WHEN selected_runner_type = 'codex_cli' THEN 'codex'
                    WHEN selected_runner_type = 'claude_cli' THEN 'claude'
                    ELSE selected_runner_type
                END
                """
            )
        )
        op.alter_column(
            "jobs",
            "selected_runner_type",
            new_column_name="selected_agent_backend",
            existing_type=sa.String(length=32),
            existing_nullable=False,
        )
