from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Protocol

from app.modules.platform.contracts import JobEventView
from app.modules.platform.infra.persistence.models import PlatformRuntimeAuditEventRecord
from app.shared.infra.db.session_scope import session_scope


class _Session(Protocol):
    def add(self, value) -> None: ...

    def flush(self) -> None: ...

    def refresh(self, value) -> None: ...

    def commit(self) -> None: ...

    def rollback(self) -> None: ...

    def close(self) -> None: ...

    def query(self, *args, **kwargs): ...


@dataclass(slots=True)
class RuntimeAuditRecord:
    event_id: int
    event_type: str
    occurred_at: str
    payload: dict[str, object]
    job_id: int = 0
    job_item_id: int | None = None
    ai_execution_id: int | None = None


class PlatformRuntimeAuditService:
    def __init__(
        self,
        storage_root: Path | None = None,
        *,
        session_factory=None,
        file_name: str = "runtime-audit.jsonl",
        max_returned_events: int = 200,
    ) -> None:
        self.storage_root = storage_root or Path(__file__).resolve().parents[6] / "runtime" / "platform-runtime-audit"
        self.session_factory = session_factory
        self.file_name = str(file_name).strip() or "runtime-audit.jsonl"
        self.max_returned_events = max(int(max_returned_events or 0), 1)

    def append_event(
        self,
        *,
        event_type: str,
        payload: dict[str, object],
        occurred_at: datetime | None = None,
    ) -> RuntimeAuditRecord:
        if self.session_factory is not None:
            record = self._append_event_db(
                event_type=event_type,
                payload=payload,
            )
            if record is not None:
                return record
        self.storage_root.mkdir(parents=True, exist_ok=True)
        file_path = self.storage_root / self.file_name
        next_event_id = self._next_event_id(file_path)
        record = RuntimeAuditRecord(
            event_id=next_event_id,
            event_type=str(event_type).strip(),
            occurred_at=(occurred_at or datetime.now(UTC)).isoformat(),
            payload=dict(payload),
        )
        with file_path.open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(self._record_payload(record), ensure_ascii=False))
            stream.write("\n")
        return record

    def list_events(
        self,
        *,
        after_id: int | None = None,
        limit: int | None = None,
        event_type_prefix: str | None = None,
    ) -> list[JobEventView]:
        if self.session_factory is not None:
            db_events = self._list_events_db(after_id=after_id, limit=limit, event_type_prefix=event_type_prefix)
            if db_events:
                return db_events
        file_path = self.storage_root / self.file_name
        if not file_path.exists():
            return []
        rows: list[RuntimeAuditRecord] = []
        with file_path.open("r", encoding="utf-8") as stream:
            for raw_line in stream:
                text = raw_line.strip()
                if not text:
                    continue
                try:
                    payload = json.loads(text)
                except Exception:
                    continue
                if str(event_type_prefix or "").strip() and not str(payload.get("event_type", "")).strip().startswith(
                    str(event_type_prefix).strip()
                ):
                    continue
                try:
                    event_id = int(payload.get("event_id", 0) or 0)
                    if after_id is not None and event_id <= after_id:
                        continue
                    rows.append(
                        RuntimeAuditRecord(
                            event_id=event_id,
                            event_type=str(payload.get("event_type", "")).strip(),
                            occurred_at=str(payload.get("occurred_at", "")).strip(),
                            payload=dict(payload.get("payload", {}) or {}),
                            job_id=int(payload.get("job_id", 0) or 0),
                            job_item_id=(
                                int(payload["job_item_id"]) if payload.get("job_item_id") is not None else None
                            ),
                            ai_execution_id=(
                                int(payload["ai_execution_id"]) if payload.get("ai_execution_id") is not None else None
                            ),
                        )
                    )
                except Exception:
                    continue
        rows = rows[-min(limit or self.max_returned_events, self.max_returned_events) :]
        return [
            JobEventView(
                event_id=row.event_id,
                event_type=row.event_type,
                job_id=row.job_id,
                job_item_id=row.job_item_id,
                ai_execution_id=row.ai_execution_id,
                occurred_at=row.occurred_at,
                payload=dict(row.payload),
            )
            for row in rows
        ]

    def _append_event_db(
        self,
        *,
        event_type: str,
        payload: dict[str, object],
    ) -> RuntimeAuditRecord | None:
        try:
            with session_scope(self.session_factory) as session:
                row = PlatformRuntimeAuditEventRecord(
                    event_type=str(event_type).strip(),
                    event_payload=dict(payload),
                )
                session.add(row)
                session.flush()
                session.refresh(row)
                return RuntimeAuditRecord(
                    event_id=row.id,
                    event_type=row.event_type,
                    occurred_at=row.created_at.isoformat() if getattr(row, "created_at", None) is not None else "",
                    payload=dict(row.event_payload or {}),
                )
        except Exception:
            return None

    def _list_events_db(
        self,
        *,
        after_id: int | None = None,
        limit: int | None = None,
        event_type_prefix: str | None = None,
    ) -> list[JobEventView] | None:
        try:
            with session_scope(self.session_factory) as session:
                query = session.query(PlatformRuntimeAuditEventRecord)
                if after_id is not None:
                    query = query.filter(PlatformRuntimeAuditEventRecord.id > after_id)
                if str(event_type_prefix or "").strip():
                    query = query.filter(
                        PlatformRuntimeAuditEventRecord.event_type.like(f"{str(event_type_prefix).strip()}%")
                    )
                rows = (
                    query.order_by(
                        PlatformRuntimeAuditEventRecord.created_at.desc(), PlatformRuntimeAuditEventRecord.id.desc()
                    )
                    .limit(min(limit or self.max_returned_events, self.max_returned_events))
                    .all()
                )
                rows.reverse()
                return [
                    JobEventView(
                        event_id=row.id,
                        event_type=row.event_type,
                        job_id=0,
                        job_item_id=None,
                        ai_execution_id=None,
                        occurred_at=row.created_at.isoformat() if getattr(row, "created_at", None) is not None else "",
                        payload=dict(row.event_payload or {}),
                    )
                    for row in rows
                ]
        except Exception:
            return None

    @staticmethod
    def _record_payload(record: RuntimeAuditRecord) -> dict[str, object]:
        return {
            "event_id": record.event_id,
            "event_type": record.event_type,
            "job_id": record.job_id,
            "job_item_id": record.job_item_id,
            "ai_execution_id": record.ai_execution_id,
            "occurred_at": record.occurred_at,
            "payload": dict(record.payload),
        }

    def _next_event_id(self, file_path: Path) -> int:
        if not file_path.exists():
            return 1
        last_id = 0
        with file_path.open("r", encoding="utf-8") as stream:
            for raw_line in stream:
                text = raw_line.strip()
                if not text:
                    continue
                try:
                    payload = json.loads(text)
                except Exception:
                    continue
                try:
                    last_id = max(last_id, int(payload.get("event_id", 0) or 0))
                except Exception:
                    continue
        return last_id + 1
