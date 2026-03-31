"""AgentTheSpire Backend — FastAPI 主入口"""
import asyncio
import importlib
import logging
import sys
from pathlib import Path

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(name)s] %(levelname)s %(message)s",
)

# Windows 上 SelectorEventLoop 不支持 create_subprocess_exec，切换为 ProactorEventLoop
if sys.platform == "win32":
    asyncio.set_event_loop_policy(asyncio.WindowsProactorEventLoopPolicy())

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.staticfiles import StaticFiles

from app.composition.container import ApplicationContainer
from app.shared.infra.feature_flags import resolve_platform_migration_flags
from config import get_config
from routers.workflow import router as workflow_router
from routers.config_router import router as config_router
from routers.batch_workflow import router as batch_router
from routers.log_analyzer import router as log_analyzer_router
from routers.mod_analyzer import router as mod_analyzer_router
from routers.build_deploy import router as build_deploy_router
from routers.approval_router import router as approval_router

app = FastAPI(title="AgentTheSpire", version="0.1.0")
app.state.container = ApplicationContainer.from_config(get_config())

app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:5173", "http://localhost:7860"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(workflow_router,    prefix="/api")
app.include_router(config_router,      prefix="/api")
app.include_router(batch_router,       prefix="/api")
app.include_router(log_analyzer_router, prefix="/api")
app.include_router(mod_analyzer_router,  prefix="/api")
app.include_router(build_deploy_router,  prefix="/api")
app.include_router(approval_router,      prefix="/api")


def _include_platform_router(module_name: str, attr_name: str = "router") -> None:
    try:
        module = importlib.import_module(module_name)
    except ModuleNotFoundError:
        logging.getLogger(__name__).info("platform router not available yet: %s", module_name)
        return

    router = getattr(module, attr_name, None)
    if router is not None:
        app.include_router(router, prefix="/api")


_platform_flags = resolve_platform_migration_flags(get_config())
if _platform_flags.platform_jobs_api_enabled:
    _include_platform_router("routers.platform_jobs")
if _platform_flags.platform_service_split_enabled:
    _include_platform_router("routers.platform_admin")

# 生产模式下托管前端静态文件
_frontend_dist = Path(__file__).parent.parent / "frontend" / "dist"
if _frontend_dist.exists():
    app.mount("/", StaticFiles(directory=str(_frontend_dist), html=True), name="frontend")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run("main:app", host="127.0.0.1", port=7860, reload=False)
