import importlib
import sys
import types
from pathlib import Path

import pytest
from fastapi import HTTPException

sys.path.insert(0, str(Path(__file__).parent.parent))

knowledge_router = importlib.import_module("routers.knowledge_router")


def test_status_delegates_to_runtime(monkeypatch):
    monkeypatch.setattr(
        knowledge_router,
        "_runtime",
        lambda: types.SimpleNamespace(get_knowledge_status=lambda: {"status": "fresh"}),
    )

    assert knowledge_router.get_knowledge_status() == {"status": "fresh"}


def test_check_delegates_to_runtime(monkeypatch):
    monkeypatch.setattr(
        knowledge_router,
        "_runtime",
        lambda: types.SimpleNamespace(check_knowledge_status=lambda: {"status": "stale"}),
    )

    assert knowledge_router.check_knowledge_status() == {"status": "stale"}


def test_refresh_start_delegates_to_runtime(monkeypatch):
    monkeypatch.setattr(
        knowledge_router,
        "_runtime",
        lambda: types.SimpleNamespace(start_refresh_task=lambda: {"task_id": "refresh-1", "status": "running"}),
    )

    result = knowledge_router.start_refresh_knowledge()

    assert result["task_id"] == "refresh-1"
    assert result["status"] == "running"


def test_latest_refresh_delegates_to_runtime(monkeypatch):
    monkeypatch.setattr(
        knowledge_router,
        "_runtime",
        lambda: types.SimpleNamespace(get_latest_refresh_task=lambda: {"task_id": "refresh-latest", "status": "running"}),
    )

    result = knowledge_router.get_latest_refresh_knowledge()

    assert result["task_id"] == "refresh-latest"
    assert result["status"] == "running"


def test_export_pack_delegates_to_runtime(monkeypatch):
    monkeypatch.setattr(
        knowledge_router,
        "_runtime",
        lambda: types.SimpleNamespace(
            export_current_knowledge_pack_zip=lambda: {
                "content": b"zip-bytes",
                "file_name": "current.zip",
                "file_count": 2,
            }
        ),
    )

    response = knowledge_router.export_current_knowledge_pack()

    assert response.content == b"zip-bytes"
    assert response.media_type == "application/zip"
    assert response.headers["Content-Disposition"] == 'attachment; filename="current.zip"'
    assert response.headers["X-ATS-Knowledge-Pack-File-Count"] == "2"


def test_refresh_task_returns_404_when_missing(monkeypatch):
    def fake_get_refresh_task(task_id):
        raise KeyError(task_id)

    monkeypatch.setattr(
        knowledge_router,
        "_runtime",
        lambda: types.SimpleNamespace(get_refresh_task=fake_get_refresh_task),
    )

    try:
        knowledge_router.get_refresh_knowledge("missing-task")
    except HTTPException as exc:
        assert exc.status_code == 404
        assert "missing-task" in exc.detail
    else:
        raise AssertionError("missing refresh task should return 404")
