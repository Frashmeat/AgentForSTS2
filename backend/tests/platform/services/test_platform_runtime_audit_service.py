from __future__ import annotations

import sys
from pathlib import Path

from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from app.modules.auth.infra.persistence import models as _auth_models  # noqa: F401
from app.modules.auth.infra.persistence.models import auth_tables
from app.modules.platform.application.services.platform_runtime_audit_service import PlatformRuntimeAuditService
from app.modules.platform.infra.persistence import models as _platform_models  # noqa: F401
from app.modules.platform.infra.persistence.models import platform_tables
from app.shared.infra.db.base import Base


def test_platform_runtime_audit_service_can_append_and_list_events_in_database():
    engine = create_engine("sqlite+pysqlite:///:memory:")
    Base.metadata.create_all(engine, tables=[*auth_tables(), *platform_tables()])
    session_factory = sessionmaker(bind=engine, autoflush=False, autocommit=False, expire_on_commit=False)
    service = PlatformRuntimeAuditService(session_factory=session_factory)

    service.append_event(
        event_type="runtime.queue_worker.leader_acquired",
        payload={
            "owner_id": "queue-worker:123",
            "leader_epoch": 1,
            "detail": "queue worker became leader",
        },
    )

    events = service.list_events()

    assert len(events) == 1
    assert events[0].event_type == "runtime.queue_worker.leader_acquired"
    assert events[0].job_id == 0
    assert events[0].payload["owner_id"] == "queue-worker:123"

    engine.dispose()


def test_platform_runtime_audit_service_can_fallback_to_file_storage(tmp_path):
    service = PlatformRuntimeAuditService(storage_root=tmp_path / "runtime-audit")

    service.append_event(
        event_type="runtime.queue_worker.leader_acquired",
        payload={
            "owner_id": "queue-worker:file",
            "leader_epoch": 1,
            "detail": "queue worker became leader",
        },
    )

    events = service.list_events()

    assert len(events) == 1
    assert events[0].payload["owner_id"] == "queue-worker:file"
