import asyncio
import importlib
import sys
import types
from pathlib import Path

from fastapi import HTTPException

sys.path.insert(0, str(Path(__file__).parent.parent))

config_router = importlib.import_module("routers.config_router")
from app.shared.prompting import PromptLoader


def test_test_imggen_returns_generated_image_size(monkeypatch):
    class FakeImage:
        size = (512, 512)

    async def fake_generate_images(prompt, asset_type, batch_size=1):
        assert prompt == PromptLoader().load("runtime_system.config_image_test_prompt").strip()
        assert asset_type == "power"
        assert batch_size == 1
        return [FakeImage()]

    fake_module = types.SimpleNamespace(generate_images=fake_generate_images)
    monkeypatch.setitem(sys.modules, "image.generator", fake_module)

    result = asyncio.run(config_router.test_imggen())

    assert result == {"ok": True, "size": [512, 512]}


def test_test_imggen_leaves_unexpected_generator_errors_to_global_handler(monkeypatch):
    async def fake_generate_images(prompt, asset_type, batch_size=1):
        raise RuntimeError("x" * 400)

    fake_module = types.SimpleNamespace(generate_images=fake_generate_images)
    monkeypatch.setitem(sys.modules, "image.generator", fake_module)

    try:
        asyncio.run(config_router.test_imggen())
    except RuntimeError as exc:
        assert str(exc) == "x" * 400
    else:
        raise AssertionError("test_imggen should leave unexpected failures to the global handler")


def test_detect_paths_leaves_unexpected_detector_errors_to_global_handler(monkeypatch):
    def fake_detect_paths():
        raise RuntimeError("detect failed")

    fake_module = types.SimpleNamespace(detect_paths=fake_detect_paths)
    monkeypatch.setitem(sys.modules, "project_utils", fake_module)

    try:
        config_router.detect_paths()
    except RuntimeError as exc:
        assert str(exc) == "detect failed"
    else:
        raise AssertionError("detect_paths should leave unexpected failures to the global handler")


def test_start_detect_paths_task_delegates_to_project_utils(monkeypatch):
    def fake_start_detect_paths_task():
        return {"task_id": "task-1", "status": "running", "notes": ["开始检测"], "can_cancel": True}

    fake_module = types.SimpleNamespace(start_detect_paths_task=fake_start_detect_paths_task)
    monkeypatch.setitem(sys.modules, "project_utils", fake_module)

    result = config_router.start_detect_paths_task()

    assert result["task_id"] == "task-1"
    assert result["status"] == "running"


def test_get_detect_paths_task_delegates_to_project_utils(monkeypatch):
    def fake_get_detect_paths_task(task_id):
        assert task_id == "task-1"
        return {"task_id": task_id, "status": "completed", "notes": ["已完成"], "can_cancel": False}

    fake_module = types.SimpleNamespace(get_detect_paths_task=fake_get_detect_paths_task)
    monkeypatch.setitem(sys.modules, "project_utils", fake_module)

    result = config_router.get_detect_paths_task("task-1")

    assert result["task_id"] == "task-1"
    assert result["status"] == "completed"


def test_get_latest_detect_paths_task_delegates_to_project_utils(monkeypatch):
    def fake_get_latest_detect_paths_task():
        return {"task_id": "task-latest", "status": "running", "notes": ["进行中"], "can_cancel": True}

    fake_module = types.SimpleNamespace(get_latest_detect_paths_task=fake_get_latest_detect_paths_task)
    monkeypatch.setitem(sys.modules, "project_utils", fake_module)

    result = config_router.get_latest_detect_paths_task()

    assert result["task_id"] == "task-latest"
    assert result["status"] == "running"


def test_cancel_detect_paths_task_delegates_to_project_utils(monkeypatch):
    def fake_cancel_detect_paths_task(task_id):
        assert task_id == "task-1"
        return {"task_id": task_id, "status": "cancelled", "notes": ["已取消"], "can_cancel": False}

    fake_module = types.SimpleNamespace(cancel_detect_paths_task=fake_cancel_detect_paths_task)
    monkeypatch.setitem(sys.modules, "project_utils", fake_module)

    result = config_router.cancel_detect_paths_task("task-1")

    assert result["task_id"] == "task-1"
    assert result["status"] == "cancelled"


def test_get_detect_paths_task_returns_404_when_task_missing(monkeypatch):
    def fake_get_detect_paths_task(task_id):
        raise KeyError(task_id)

    fake_module = types.SimpleNamespace(get_detect_paths_task=fake_get_detect_paths_task)
    monkeypatch.setitem(sys.modules, "project_utils", fake_module)

    try:
        config_router.get_detect_paths_task("missing")
    except HTTPException as exc:
        assert exc.status_code == 404
        assert "missing" in exc.detail
    else:
        raise AssertionError("missing detect task should return 404")


def test_pick_path_delegates_to_project_utils(monkeypatch):
    calls = {}

    def fake_pick_path(*, kind, title, initial_path, filters):
        calls["payload"] = {
            "kind": kind,
            "title": title,
            "initial_path": initial_path,
            "filters": filters,
        }
        return {"path": "C:/tools/Godot_v4.5.1-stable_mono_win64.exe"}

    fake_module = types.SimpleNamespace(pick_path=fake_pick_path)
    monkeypatch.setitem(sys.modules, "project_utils", fake_module)

    result = config_router.pick_path(
        {
            "kind": "file",
            "title": "选择 Godot",
            "initial_path": "C:/tools",
            "filters": [["Godot executable", "*.exe"]],
        }
    )

    assert result == {"path": "C:/tools/Godot_v4.5.1-stable_mono_win64.exe"}
    assert calls["payload"] == {
        "kind": "file",
        "title": "选择 Godot",
        "initial_path": "C:/tools",
        "filters": [["Godot executable", "*.exe"]],
    }


def test_pick_path_leaves_unexpected_picker_errors_to_global_handler(monkeypatch):
    def fake_pick_path(*, kind, title, initial_path, filters):
        raise RuntimeError("picker unavailable")

    fake_module = types.SimpleNamespace(pick_path=fake_pick_path)
    monkeypatch.setitem(sys.modules, "project_utils", fake_module)

    try:
        config_router.pick_path({"kind": "directory", "title": "选择目录"})
    except RuntimeError as exc:
        assert str(exc) == "picker unavailable"
    else:
        raise AssertionError("pick_path should leave unexpected failures to the global handler")
