from __future__ import annotations

import os
import secrets

from fastapi import APIRouter, Header, HTTPException, Request

from app.modules.knowledge.infra import knowledge_runtime
from app.shared.infra.config.settings import Settings
from config import get_config

router = APIRouter(prefix="/workstation")
_CONTROL_TOKEN_HEADER = "X-ATS-Workstation-Token"


def _settings(request: Request) -> Settings:
    container = getattr(request.app.state, "container", None)
    if container is not None:
        settings = container.resolve_optional_singleton("settings")
        if isinstance(settings, Settings):
            return settings
    return Settings.from_dict(get_config())


def _require_control_token(
    request: Request,
    x_ats_workstation_token: str = Header(default="", alias=_CONTROL_TOKEN_HEADER),
) -> None:
    platform_execution = _settings(request).platform_execution
    token_env = str(platform_execution.get("control_token_env", "")).strip()
    if not token_env:
        raise HTTPException(status_code=503, detail="workstation control token env is not configured")
    expected = os.environ.get(token_env, "").strip()
    if not expected:
        raise HTTPException(status_code=503, detail="workstation control token is not configured")
    if not x_ats_workstation_token:
        raise HTTPException(status_code=401, detail="workstation control token required")
    if not secrets.compare_digest(x_ats_workstation_token, expected):
        raise HTTPException(status_code=403, detail="invalid workstation control token")


@router.get("/capabilities")
def get_workstation_capabilities(
    request: Request,
    x_ats_workstation_token: str = Header(default="", alias=_CONTROL_TOKEN_HEADER),
):
    _require_control_token(request, x_ats_workstation_token)
    cfg = _settings(request).to_dict()
    sts2_path = str(cfg.get("sts2_path", "")).strip()
    active_pack = knowledge_runtime.get_active_knowledge_pack()
    return {
        "knowledge": {
            "embedded_sts2_guidance": True,
            "knowledge_pack_active": active_pack is not None,
            "active_knowledge_pack_id": str(active_pack.get("pack_id", "")) if active_pack else "",
            "sts2_path_configured": bool(sts2_path),
            "sts2_game_available": False,
        },
        "generation": {
            "text_generation_available": True,
            "code_generation_available": True,
        },
        "build": {
            "server_build_supported": False,
            "dotnet_available": False,
            "godot_configured": False,
            "godot_executable_available": False,
        },
        "deploy": {
            "server_deploy_supported": False,
            "sts2_mods_path_available": False,
        },
    }
