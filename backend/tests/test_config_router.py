import asyncio
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

    def patch(self, _path):
        def decorator(func):
            return func
        return decorator

    def post(self, _path):
        def decorator(func):
            return func
        return decorator


sys.modules["fastapi"] = types.SimpleNamespace(APIRouter=_DummyRouter)
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


def test_test_imggen_truncates_generator_errors(monkeypatch):
    async def fake_generate_images(prompt, asset_type, batch_size=1):
        raise RuntimeError("x" * 400)

    fake_module = types.SimpleNamespace(generate_images=fake_generate_images)
    monkeypatch.setitem(sys.modules, "image.generator", fake_module)

    result = asyncio.run(config_router.test_imggen())

    assert result["ok"] is False
    assert len(result["error"]) == 300


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

    monkeypatch.setattr(config_router, "_config_facade", lambda request: None)
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
