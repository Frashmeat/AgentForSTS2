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
from app.shared.infra.feature_flags import resolve_platform_migration_flags
from config import get_config

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(name)s] %(levelname)s %(message)s",
)

if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsProactorEventLoopPolicy())

AppRole = Literal["full", "workstation", "web"]

_WORKSTATION_ROUTER_MODULES = (
    "routers.workflow",
    "routers.config_router",
    "routers.batch_workflow",
    "routers.log_analyzer",
    "routers.mod_analyzer",
    "routers.build_deploy",
    "routers.approval_router",
)

_WEB_ROUTER_MODULES = (
    "routers.platform_jobs",
    "routers.platform_admin",
)


def _create_base_app() -> FastAPI:
    app = FastAPI(title="AgentTheSpire", version="0.1.0")
    app.state.container = ApplicationContainer.from_config(get_config())

    app.add_middleware(
        CORSMiddleware,
        allow_origins=["http://localhost:5173", "http://localhost:7860", "http://localhost:7870", "http://localhost:8080"],
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
    for module_name in _WORKSTATION_ROUTER_MODULES:
        _include_router(app, module_name)


def _include_web_routers(app: FastAPI) -> None:
    for module_name in _WEB_ROUTER_MODULES:
        _include_router(app, module_name)


def _mount_frontend(app: FastAPI) -> None:
    frontend_dist = Path(__file__).resolve().parent.parent / "frontend" / "dist"
    if frontend_dist.exists():
        app.mount("/", StaticFiles(directory=str(frontend_dist), html=True), name="frontend")


def create_app(role: AppRole) -> FastAPI:
    app = _create_base_app()

    if role in {"full", "workstation"}:
        _include_workstation_routers(app)

    if role == "web":
        _include_web_routers(app)
    elif role == "full":
        platform_flags = resolve_platform_migration_flags(get_config())
        if platform_flags.platform_jobs_api_enabled:
            _include_router(app, "routers.platform_jobs")
        if platform_flags.platform_service_split_enabled:
            _include_router(app, "routers.platform_admin")

    if role in {"full", "workstation"}:
        _mount_frontend(app)

    return app
