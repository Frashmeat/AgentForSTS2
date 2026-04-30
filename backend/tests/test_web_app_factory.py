from __future__ import annotations

import sys
from pathlib import Path

import pytest
from fastapi import FastAPI

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import app_factory


def test_get_router_modules_for_web_role_isolated_from_workstation_routes():
    modules = app_factory.get_router_modules_for_role("web", config={})

    assert modules == (
        "routers.auth_router",
        "routers.me_router",
        "routers.platform_jobs",
        "routers.platform_admin",
    )
    assert "routers.workflow" not in modules
    assert "routers.config_router" not in modules


def test_get_router_modules_for_workstation_role_excludes_web_routes():
    modules = app_factory.get_router_modules_for_role("workstation", config={})

    assert modules == app_factory.WORKSTATION_ROUTER_MODULES
    assert "routers.auth_router" not in modules
    assert "routers.platform_jobs" not in modules


def test_create_app_for_web_includes_only_web_routes_and_skips_frontend_mount(monkeypatch):
    included_modules: list[str] = []
    frontend_mounted = False
    execution_profiles_seeded = False
    queue_worker_registered = False
    workstation_runtime_registered = False

    def fake_base_app(role: str, config: dict) -> FastAPI:
        app = FastAPI()
        app.state.container = object()
        return app

    def fake_include_router(app: FastAPI, module_name: str, attr_name: str = "router") -> None:
        included_modules.append(module_name)

    def fake_mount_frontend(app: FastAPI) -> None:
        nonlocal frontend_mounted
        frontend_mounted = True

    def fake_bootstrap_execution_profiles(app: FastAPI) -> None:
        nonlocal execution_profiles_seeded
        execution_profiles_seeded = True

    def fake_register_web_queue_worker_lifecycle(app: FastAPI) -> None:
        nonlocal queue_worker_registered
        queue_worker_registered = True

    def fake_register_web_workstation_runtime_lifecycle(app: FastAPI) -> None:
        nonlocal workstation_runtime_registered
        workstation_runtime_registered = True

    monkeypatch.setattr(app_factory, "_create_base_app", fake_base_app)
    monkeypatch.setattr(app_factory, "_include_router", fake_include_router)
    monkeypatch.setattr(app_factory, "_mount_frontend", fake_mount_frontend)
    monkeypatch.setattr(app_factory, "_bootstrap_web_execution_profiles", fake_bootstrap_execution_profiles)
    monkeypatch.setattr(
        app_factory, "_register_web_workstation_runtime_lifecycle", fake_register_web_workstation_runtime_lifecycle
    )
    monkeypatch.setattr(app_factory, "_register_web_queue_worker_lifecycle", fake_register_web_queue_worker_lifecycle)
    monkeypatch.setattr(app_factory, "get_config", lambda: {})

    app_factory.create_app("web")

    assert included_modules == list(app_factory.WEB_ROUTER_MODULES)
    assert execution_profiles_seeded is True
    assert workstation_runtime_registered is True
    assert queue_worker_registered is True
    assert frontend_mounted is False


def test_create_app_for_workstation_includes_only_workstation_routes_and_mounts_frontend(monkeypatch):
    included_modules: list[str] = []
    frontend_mounted = False
    execution_profiles_seeded = False
    queue_worker_registered = False
    workstation_runtime_registered = False

    def fake_base_app(role: str, config: dict) -> FastAPI:
        app = FastAPI()
        app.state.container = object()
        return app

    def fake_include_router(app: FastAPI, module_name: str, attr_name: str = "router") -> None:
        included_modules.append(module_name)

    def fake_mount_frontend(app: FastAPI) -> None:
        nonlocal frontend_mounted
        frontend_mounted = True

    def fake_bootstrap_execution_profiles(app: FastAPI) -> None:
        nonlocal execution_profiles_seeded
        execution_profiles_seeded = True

    def fake_register_web_queue_worker_lifecycle(app: FastAPI) -> None:
        nonlocal queue_worker_registered
        queue_worker_registered = True

    def fake_register_web_workstation_runtime_lifecycle(app: FastAPI) -> None:
        nonlocal workstation_runtime_registered
        workstation_runtime_registered = True

    monkeypatch.setattr(app_factory, "_create_base_app", fake_base_app)
    monkeypatch.setattr(app_factory, "_include_router", fake_include_router)
    monkeypatch.setattr(app_factory, "_mount_frontend", fake_mount_frontend)
    monkeypatch.setattr(app_factory, "_bootstrap_web_execution_profiles", fake_bootstrap_execution_profiles)
    monkeypatch.setattr(
        app_factory, "_register_web_workstation_runtime_lifecycle", fake_register_web_workstation_runtime_lifecycle
    )
    monkeypatch.setattr(app_factory, "_register_web_queue_worker_lifecycle", fake_register_web_queue_worker_lifecycle)
    monkeypatch.setattr(app_factory, "get_config", lambda: {})

    app_factory.create_app("workstation")

    assert included_modules == list(app_factory.WORKSTATION_ROUTER_MODULES)
    assert execution_profiles_seeded is False
    assert workstation_runtime_registered is False
    assert queue_worker_registered is False
    assert frontend_mounted is True


def test_create_app_for_web_fails_fast_when_runtime_secret_is_missing(monkeypatch):
    monkeypatch.setattr(
        app_factory,
        "get_config",
        lambda: {
            "database": {
                "url": "sqlite+pysqlite:///:memory:",
            }
        },
    )

    with pytest.raises(RuntimeError, match="auth.session_secret is required for web runtime"):
        app_factory.create_app("web")


def test_register_web_workstation_runtime_lifecycle_stores_manager_in_app_and_container(monkeypatch):
    registered: dict[str, object] = {}

    class FakeContainer:
        def resolve_singleton(self, name: str):
            assert name == "settings"
            return object()

        def register_singleton(self, name: str, value: object) -> None:
            registered[name] = value

    class FakeRuntimeManager:
        def __init__(self, *, settings, cwd):
            self.settings = settings
            self.cwd = cwd
            self.started = False
            self.stopped = False

        def ensure_started(self):
            self.started = True

        def stop(self):
            self.stopped = True

    monkeypatch.setattr(app_factory, "WorkstationRuntimeManager", FakeRuntimeManager)
    app = FastAPI()
    app.state.container = FakeContainer()

    app_factory._register_web_workstation_runtime_lifecycle(app)

    manager = app.state.workstation_runtime_manager
    assert isinstance(manager, FakeRuntimeManager)
    assert registered["platform.workstation_runtime_manager"] is manager


def test_resolve_cors_allow_origin_regex_only_enables_loopback_when_requested():
    assert (
        app_factory._resolve_cors_allow_origin_regex({"allow_loopback_origins": True})
        == app_factory._LOOPBACK_CORS_ORIGIN_REGEX
    )
    assert app_factory._resolve_cors_allow_origin_regex({"allow_loopback_origins": False}) is None
    assert app_factory._resolve_cors_allow_origin_regex({}) is None
