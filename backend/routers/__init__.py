"""Router registration groups for each backend runtime role."""
from __future__ import annotations

WORKSTATION_ROUTER_MODULES = (
    "routers.workflow",
    "routers.config_router",
    "routers.batch_workflow",
    "routers.log_analyzer",
    "routers.mod_analyzer",
    "routers.build_deploy",
    "routers.approval_router",
)

WEB_ROUTER_MODULES = (
    "routers.auth_router",
    "routers.me_router",
    "routers.platform_jobs",
    "routers.platform_admin",
)

__all__ = [
    "WEB_ROUTER_MODULES",
    "WORKSTATION_ROUTER_MODULES",
]
