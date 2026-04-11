from __future__ import annotations

import sys
from pathlib import Path
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

    def fake_base_app(role: str, config: dict) -> FastAPI:
        app = FastAPI()
        app.state.container = object()
        return app

    def fake_include_router(app: FastAPI, module_name: str, attr_name: str = "router") -> None:
        included_modules.append(module_name)

    def fake_mount_frontend(app: FastAPI) -> None:
        nonlocal frontend_mounted
        frontend_mounted = True

    monkeypatch.setattr(app_factory, "_create_base_app", fake_base_app)
    monkeypatch.setattr(app_factory, "_include_router", fake_include_router)
    monkeypatch.setattr(app_factory, "_mount_frontend", fake_mount_frontend)
    monkeypatch.setattr(app_factory, "get_config", lambda: {})

    app_factory.create_app("web")

    assert included_modules == list(app_factory.WEB_ROUTER_MODULES)
    assert frontend_mounted is False


def test_create_app_for_workstation_includes_only_workstation_routes_and_mounts_frontend(monkeypatch):
    included_modules: list[str] = []
    frontend_mounted = False

    def fake_base_app(role: str, config: dict) -> FastAPI:
        app = FastAPI()
        app.state.container = object()
        return app

    def fake_include_router(app: FastAPI, module_name: str, attr_name: str = "router") -> None:
        included_modules.append(module_name)

    def fake_mount_frontend(app: FastAPI) -> None:
        nonlocal frontend_mounted
        frontend_mounted = True

    monkeypatch.setattr(app_factory, "_create_base_app", fake_base_app)
    monkeypatch.setattr(app_factory, "_include_router", fake_include_router)
    monkeypatch.setattr(app_factory, "_mount_frontend", fake_mount_frontend)
    monkeypatch.setattr(app_factory, "get_config", lambda: {})

    app_factory.create_app("workstation")

    assert included_modules == list(app_factory.WORKSTATION_ROUTER_MODULES)
    assert frontend_mounted is True
