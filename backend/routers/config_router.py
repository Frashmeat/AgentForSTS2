from fastapi import APIRouter, HTTPException
from starlette.requests import Request
from app.shared.infra.llm.agent_backend import resolve_agent_backend_name
from app.shared.prompting import PromptLoader
from config import get_config, update_config

router = APIRouter(prefix="/config")
_TEXT_LOADER = PromptLoader()


def _config_facade(request):
    if request is None:
        return None
    container = getattr(getattr(request.app.state, "container", None), "resolve_optional_singleton", None)
    if container is None:
        return None
    flags = getattr(request.app.state.container, "platform_migration_flags", None)
    if flags is None or not getattr(flags, "platform_service_split_enabled", False):
        return None
    return request.app.state.container.resolve_optional_singleton("platform.config_facade_service")


@router.get("")
def get_cfg(request: Request = None):
    facade = _config_facade(request)
    if facade is not None:
        return facade.get_masked_config()
    cfg = get_config()
    # 返回前脱敏 API key（只显示后4位）
    safe = _mask_keys(cfg)
    return safe


@router.patch("")
def patch_cfg(body: dict, request: Request = None):
    facade = _config_facade(request)
    if facade is not None:
        return facade.patch_config(body)
    # Don't overwrite keys with masked placeholder values (e.g. "****yYWE")
    for section_key in ("llm", "image_gen"):
        section = body.get(section_key, {})
        for field in ("api_key", "api_secret"):
            if isinstance(section.get(field), str) and section[field].startswith("****"):
                section.pop(field, None)
    updated = update_config(body)
    return _mask_keys(updated)


@router.get("/detect_paths")
def detect_paths(request: Request = None):
    try:
        facade = _config_facade(request)
        if facade is not None:
            return facade.detect_paths()
        """自动检测 STS2 和 Godot 路径，返回检测结果供用户确认后填入配置。"""
        from project_utils import detect_paths as _detect
        return _detect()
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.post("/detect_paths/start")
def start_detect_paths_task(request: Request = None):
    try:
        facade = _config_facade(request)
        if facade is not None and hasattr(facade, "start_detect_paths_task"):
            return facade.start_detect_paths_task()
        from project_utils import start_detect_paths_task as _start
        return _start()
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.get("/detect_paths/{task_id}")
def get_detect_paths_task(task_id: str, request: Request = None):
    try:
        facade = _config_facade(request)
        if facade is not None and hasattr(facade, "get_detect_paths_task"):
            return facade.get_detect_paths_task(task_id)
        from project_utils import get_detect_paths_task as _get
        return _get(task_id)
    except HTTPException:
        raise
    except KeyError as exc:
        missing_task_id = exc.args[0] if exc.args else task_id
        raise HTTPException(status_code=404, detail=f"未找到检测任务: {missing_task_id}") from exc
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.post("/detect_paths/{task_id}/cancel")
def cancel_detect_paths_task(task_id: str, request: Request = None):
    try:
        facade = _config_facade(request)
        if facade is not None and hasattr(facade, "cancel_detect_paths_task"):
            return facade.cancel_detect_paths_task(task_id)
        from project_utils import cancel_detect_paths_task as _cancel
        return _cancel(task_id)
    except HTTPException:
        raise
    except KeyError as exc:
        missing_task_id = exc.args[0] if exc.args else task_id
        raise HTTPException(status_code=404, detail=f"未找到检测任务: {missing_task_id}") from exc
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.post("/pick_path")
def pick_path(body: dict, request: Request = None):
    try:
        facade = _config_facade(request)
        if facade is not None:
            return facade.pick_path(body)
        from project_utils import pick_path as _pick

        return _pick(
            kind=body.get("kind", ""),
            title=body.get("title", ""),
            initial_path=body.get("initial_path", ""),
            filters=body.get("filters") or [],
        )
    except HTTPException:
        raise
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)) from exc


@router.get("/local_ai_capability_status")
def local_ai_capability_status(request: Request = None):
    facade = _config_facade(request)
    if facade is not None:
        return facade.get_local_ai_capability_status()
    cfg = get_config()
    text_ai_available, text_ai_missing_reasons = _resolve_text_ai_capability(cfg.get("llm", {}))
    code_agent_available, code_agent_missing_reasons = _resolve_code_agent_capability(cfg.get("llm", {}))
    image_ai_available, image_ai_missing_reasons = _resolve_image_ai_capability(cfg.get("image_gen", {}))
    return {
        "text_ai_available": text_ai_available,
        "code_agent_available": code_agent_available,
        "image_ai_available": image_ai_available,
        "text_ai_missing_reasons": text_ai_missing_reasons,
        "code_agent_missing_reasons": code_agent_missing_reasons,
        "image_ai_missing_reasons": image_ai_missing_reasons,
    }


@router.get("/test_imggen")
async def test_imggen(request: Request = None):
    facade = _config_facade(request)
    if facade is not None:
        return await facade.test_imggen()
    from image.generator import generate_images
    try:
        imgs = await generate_images(_TEXT_LOADER.load("runtime_system.config_image_test_prompt").strip(), "power", batch_size=1)
        return {"ok": True, "size": list(imgs[0].size)}
    except Exception as exc:
        raise HTTPException(status_code=500, detail=str(exc)[:300]) from exc


def _mask_keys(cfg: dict) -> dict:
    import copy
    c = copy.deepcopy(cfg)
    for section in (c.get("llm", {}), c.get("image_gen", {})):
        for field in ("api_key", "api_secret"):
            if section.get(field):
                v = section[field]
                section[field] = f"****{v[-4:]}" if len(v) > 4 else "****"
    return c


def _has_text_ai(llm_cfg: dict) -> bool:
    available, _ = _resolve_text_ai_capability(llm_cfg)
    return available


def _has_image_ai(image_cfg: dict) -> bool:
    available, _ = _resolve_image_ai_capability(image_cfg)
    return available


def _resolve_code_agent_capability(llm_cfg: dict) -> tuple[bool, list[str]]:
    mode = str(llm_cfg.get("mode", "")).strip()
    reasons: list[str] = []

    if mode == "agent_cli":
        if str(llm_cfg.get("agent_backend", "")).strip() not in {"claude", "codex"}:
            reasons.append("请先在设置中选择可用的代码代理后端（Claude CLI 或 Codex CLI）。")
        return len(reasons) == 0, reasons

    backend = resolve_agent_backend_name(llm_cfg)
    if not str(llm_cfg.get("provider", "")).strip():
        reasons.append("请先在设置中填写文本 API 提供商。")
    if not str(llm_cfg.get("model", "")).strip():
        reasons.append("请先在设置中填写文本模型。")
    if not str(llm_cfg.get("api_key", "")).strip():
        reasons.append("请先在设置中填写文本 API Key。")
    if backend == "claude" and str(llm_cfg.get("provider", "")).strip() != "anthropic":
        reasons.append("当前代码代理只会在 Anthropic 提供商下使用 Claude CLI。")
    return len(reasons) == 0, reasons


def _resolve_text_ai_capability(llm_cfg: dict) -> tuple[bool, list[str]]:
    mode = str(llm_cfg.get("mode", "")).strip()
    reasons: list[str] = []

    if mode == "agent_cli":
        if str(llm_cfg.get("agent_backend", "")).strip() not in {"claude", "codex"}:
            reasons.append("请先在设置中选择可用的代码代理后端（Claude CLI 或 Codex CLI）。")
        return len(reasons) == 0, reasons

    if not str(llm_cfg.get("provider", "")).strip():
        reasons.append("请先在设置中填写文本 API 提供商。")
    if not str(llm_cfg.get("model", "")).strip():
        reasons.append("请先在设置中填写文本模型。")
    if not str(llm_cfg.get("api_key", "")).strip():
        reasons.append("请先在设置中填写文本 API Key。")
    return len(reasons) == 0, reasons


def _resolve_image_ai_capability(image_cfg: dict) -> tuple[bool, list[str]]:
    mode = str(image_cfg.get("mode", "")).strip()
    provider = str(image_cfg.get("provider", "")).strip()
    model = str(image_cfg.get("model", "")).strip()
    reasons: list[str] = []

    if mode == "local":
        local_cfg = image_cfg.get("local", {})
        if not model:
            reasons.append("请先在设置中填写本地图像模型。")
        if not str(local_cfg.get("comfyui_url", "")).strip():
            reasons.append("请先在设置中填写 ComfyUI 地址。")
        return len(reasons) == 0, reasons

    if not provider:
        reasons.append("请先在设置中填写图像提供商。")
    if not model:
        reasons.append("请先在设置中填写图像模型。")
    if not str(image_cfg.get("api_key", "")).strip():
        reasons.append("请先在设置中填写图像 API Key。")
    if provider == "volcengine":
        if not str(image_cfg.get("api_secret", "")).strip():
            reasons.append("火山引擎模式下请先填写图像 Secret Key。")
    return len(reasons) == 0, reasons

