from __future__ import annotations

from fastapi import Request

from app.modules.platform.application.platform_runtime_builder import (
    build_execution_orchestrator_service_from_container,
)
from app.modules.platform.application.services import ExecutionOrchestratorService


def build_execution_orchestrator_service(session, request: Request) -> ExecutionOrchestratorService:
    return build_execution_orchestrator_service_from_container(session, request.app.state.container)
