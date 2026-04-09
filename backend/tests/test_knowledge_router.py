import importlib
import sys
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))


class _DummyRouter:
    def __init__(self, prefix=""):
        self.prefix = prefix

    def get(self, _path):
        def decorator(func):
            return func
        return decorator

    def post(self, _path):
        def decorator(func):
            return func
        return decorator


class _DummyHttpException(Exception):
    def __init__(self, status_code, detail):
        super().__init__(detail)
        self.status_code = status_code
        self.detail = detail


sys.modules["fastapi"] = types.SimpleNamespace(APIRouter=_DummyRouter, HTTPException=_DummyHttpException)
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
    except _DummyHttpException as exc:
        assert exc.status_code == 404
        assert "missing-task" in exc.detail
    else:
        raise AssertionError("missing refresh task should return 404")
