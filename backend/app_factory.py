"""AgentTheSpire Backend app factory."""
from __future__ import annotations

import asyncio
import importlib
import logging
import sys
from pathlib import Path
from typing import Literal

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles

from app.composition.container import ApplicationContainer
from app.shared.infra.http_errors import install_http_error_handlers
from app.shared.infra.config.settings import Settings
from app.shared.infra.feature_flags import resolve_platform_migration_flags
from config import get_config
from routers import WEB_ROUTER_MODULES, WORKSTATION_ROUTER_MODULES

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(name)s] %(levelname)s %(message)s",
)

if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsProactorEventLoopPolicy())

AppRole = Literal["full", "workstation", "web"]


def _create_base_app(role: AppRole, config: dict) -> FastAPI:
    settings = Settings.from_dict(config)
    runtime_config = settings.get_runtime(role)
    app = FastAPI(title="AgentTheSpire", version="0.1.0")
    install_http_error_handlers(app)
    app.state.container = ApplicationContainer.from_config(config, runtime_role=role)
    app.state.runtime_role = role
    app.state.runtime_config_errors = settings.validate_for_role(role)

    app.add_middleware(
        CORSMiddleware,
        allow_origins=runtime_config.get("cors_origins", []),
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )
    return app


def _include_router(app: FastAPI, module_name: str, attr_name: str = "router") -> None:
    try:
        module = importlib.import_module(module_name)
    except ModuleNotFoundError:
        logging.getLogger(__name__).info("router not available yet: %s", module_name)
        return

    router = getattr(module, attr_name, None)
    if router is not None:
        app.include_router(router, prefix="/api")


def _include_workstation_routers(app: FastAPI) -> None:
    for module_name in WORKSTATION_ROUTER_MODULES:
        _include_router(app, module_name)


def _include_web_routers(app: FastAPI) -> None:
    for module_name in WEB_ROUTER_MODULES:
        _include_router(app, module_name)


def _resolve_platform_router_modules(config: dict) -> tuple[str, ...]:
    platform_flags = resolve_platform_migration_flags(config)
    modules: list[str] = [
        "routers.auth_router",
        "routers.me_router",
    ]
    if platform_flags.platform_jobs_api_enabled:
        modules.append("routers.platform_jobs")
    if platform_flags.platform_service_split_enabled:
        modules.append("routers.platform_admin")
    return tuple(modules)


def get_router_modules_for_role(role: AppRole, config: dict | None = None) -> tuple[str, ...]:
    resolved_config = config or get_config()

    if role == "workstation":
        return WORKSTATION_ROUTER_MODULES
    if role == "web":
        return WEB_ROUTER_MODULES

    return WORKSTATION_ROUTER_MODULES + _resolve_platform_router_modules(resolved_config)


def should_mount_frontend(role: AppRole) -> bool:
    return role in {"full", "workstation"}


def _mount_frontend(app: FastAPI) -> None:
    frontend_dist = Path(__file__).resolve().parent.parent / "frontend" / "dist"
    if frontend_dist.exists():
        app.mount("/", StaticFiles(directory=str(frontend_dist), html=True), name="frontend")


def create_app(role: AppRole) -> FastAPI:
    config = get_config()
    app = _create_base_app(role, config)

    for module_name in get_router_modules_for_role(role, config):
        _include_router(app, module_name)

    if should_mount_frontend(role):
        _mount_frontend(app)

    return app
