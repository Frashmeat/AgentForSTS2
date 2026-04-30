from __future__ import annotations

import os
from dataclasses import dataclass

import psycopg
from psycopg.rows import tuple_row
from psycopg.sql import SQL, Identifier

DEFAULT_ADMIN_URL = "postgresql://postgres:postgres@127.0.0.1:5432/postgres"
DEFAULT_DATABASE_NAME = "agent_the_spire_platform"


@dataclass(slots=True)
class BootstrapResult:
    existed: bool
    database_name: str
    application_url: str


def build_application_url(admin_url: str, database_name: str) -> str:
    prefix, _, _ = admin_url.rpartition("/")
    return f"{prefix}/{database_name}"


def create_database_if_missing(admin_url: str, database_name: str) -> BootstrapResult:
    with (
        psycopg.connect(admin_url, autocommit=True, row_factory=tuple_row) as conn,
        conn.cursor() as cur,
    ):
        cur.execute("select 1 from pg_database where datname = %s", (database_name,))
        existed = cur.fetchone() is not None
        if not existed:
            cur.execute(SQL("create database {}").format(Identifier(database_name)))

    return BootstrapResult(
        existed=existed,
        database_name=database_name,
        application_url=build_application_url(admin_url, database_name),
    )


def main() -> None:
    admin_url = os.environ.get("ATS_POSTGRES_ADMIN_URL", DEFAULT_ADMIN_URL)
    database_name = os.environ.get("ATS_PLATFORM_DB_NAME", DEFAULT_DATABASE_NAME)
    result = create_database_if_missing(admin_url, database_name)

    if result.existed:
        print(f"database already exists: {result.database_name}")
    else:
        print(f"database created: {result.database_name}")
    print(f"application url: {result.application_url}")


if __name__ == "__main__":
    main()
