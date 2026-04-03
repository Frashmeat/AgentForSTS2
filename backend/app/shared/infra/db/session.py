from __future__ import annotations

from typing import Any

from sqlalchemy import Engine, create_engine
from sqlalchemy.orm import Session, sessionmaker


def create_engine_from_settings(database_cfg: dict[str, Any]) -> Engine:
    url = str(database_cfg.get("url", "")).strip()
    if not url:
        raise ValueError("database.url is required")

    return create_engine(
        url,
        echo=bool(database_cfg.get("echo", False)),
        pool_pre_ping=bool(database_cfg.get("pool_pre_ping", True)),
    )


def create_session_factory(database_cfg: dict[str, Any]) -> sessionmaker[Session]:
    engine = create_engine_from_settings(database_cfg)
    return sessionmaker(bind=engine, autoflush=False, autocommit=False, expire_on_commit=False)
