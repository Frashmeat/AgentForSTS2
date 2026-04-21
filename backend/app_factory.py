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
from app.modules.platform.application.platform_runtime_builder import build_job_application_service_from_container
from app.modules.platform.application.services import ServerExecutionService, ServerQueuedJobWorkerService
from app.shared.infra.http_errors import install_http_error_handlers
from app.shared.infra.config.settings import Settings
from config import get_config
from routers import WEB_ROUTER_MODULES, WORKSTATION_ROUTER_MODULES

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(name)s] %(levelname)s %(message)s",
)

if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsProactorEventLoopPolicy())

AppRole = Literal["workstation", "web"]
_QUEUE_WORKER_POLL_INTERVAL_SECONDS = 3.0
_QUEUE_WORKER_RETRY_COOLDOWN_SECONDS = 5


def _create_base_app(role: AppRole, config: dict) -> FastAPI:
    settings = Settings.from_dict(config)
    runtime_config_errors = settings.validate_for_role(role)
    if runtime_config_errors:
        raise RuntimeError(f"invalid {role} runtime config: {'; '.join(runtime_config_errors)}")
    runtime_config = settings.get_runtime(role)
    app = FastAPI(title="AgentTheSpire", version="0.1.0")
    install_http_error_handlers(app)
    app.state.container = ApplicationContainer.from_config(config, runtime_role=role)
    app.state.runtime_role = role
    app.state.runtime_config_errors = runtime_config_errors

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


def get_router_modules_for_role(role: AppRole, config: dict | None = None) -> tuple[str, ...]:
    if role == "workstation":
        return WORKSTATION_ROUTER_MODULES
    return WEB_ROUTER_MODULES


def should_mount_frontend(role: AppRole) -> bool:
    return role == "workstation"


def _mount_frontend(app: FastAPI) -> None:
    frontend_dist = Path(__file__).resolve().parent.parent / "frontend" / "dist"
    if frontend_dist.exists():
        app.mount("/", StaticFiles(directory=str(frontend_dist), html=True), name="frontend")


def _bootstrap_web_execution_profiles(app: FastAPI) -> None:
    container = app.state.container
    session_factory = container.resolve_optional_singleton("platform.db_session_factory")
    if session_factory is None:
        return
    session = session_factory()
    try:
        repository = container.resolve_singleton("platform.server_execution_repository_factory")(session)
        service = container.resolve_singleton("platform.server_execution_service_factory")(
            server_execution_repository=repository,
        )
        if isinstance(service, ServerExecutionService):
            service.ensure_default_execution_profiles_seeded()
        session.commit()
    except Exception:
        session.rollback()
        logging.getLogger(__name__).exception("failed to seed default execution profiles")
    finally:
        session.close()


def _build_web_queue_worker_service(app: FastAPI) -> ServerQueuedJobWorkerService | None:
    container = app.state.container
    session_factory = container.resolve_optional_singleton("platform.db_session_factory")
    if session_factory is None:
        return None
    return ServerQueuedJobWorkerService(
        session_factory=session_factory,
        job_application_service_builder=lambda session: build_job_application_service_from_container(session, container),
        poll_interval_seconds=_QUEUE_WORKER_POLL_INTERVAL_SECONDS,
        retry_cooldown_seconds=_QUEUE_WORKER_RETRY_COOLDOWN_SECONDS,
    )


def _register_web_queue_worker_lifecycle(app: FastAPI) -> None:
    worker = _build_web_queue_worker_service(app)
    if worker is None:
        return
    app.state.platform_queue_worker_service = worker

    @app.on_event("startup")
    async def _start_platform_queue_worker() -> None:
        await worker.start()

    @app.on_event("shutdown")
    async def _stop_platform_queue_worker() -> None:
        await worker.stop()


def create_app(role: AppRole) -> FastAPI:
    config = get_config()
    app = _create_base_app(role, config)

    for module_name in get_router_modules_for_role(role, config):
        _include_router(app, module_name)

    if role == "web":
        _bootstrap_web_execution_profiles(app)
        _register_web_queue_worker_lifecycle(app)

    if should_mount_frontend(role):
        _mount_frontend(app)

    return app
