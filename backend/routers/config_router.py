from fastapi import APIRouter
from starlette.requests import Request
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
    facade = _config_facade(request)
    if facade is not None:
        return facade.detect_paths()
    """自动检测 STS2 和 Godot 路径，返回检测结果供用户确认后填入配置。"""
    from project_utils import detect_paths as _detect
    return _detect()


@router.get("/local_ai_capability_status")
def local_ai_capability_status(request: Request = None):
    facade = _config_facade(request)
    if facade is not None:
        return facade.get_local_ai_capability_status()
    cfg = get_config()
    return {
        "text_ai_available": _has_text_ai(cfg.get("llm", {})),
        "image_ai_available": _has_image_ai(cfg.get("image_gen", {})),
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
    except Exception as e:
        return {"ok": False, "error": str(e)[:300]}


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
    mode = str(llm_cfg.get("mode", "")).strip()
    if mode == "agent_cli":
        return str(llm_cfg.get("agent_backend", "")).strip() in {"claude", "codex"}
    return all(str(llm_cfg.get(field, "")).strip() for field in ("provider", "model", "api_key"))


def _has_image_ai(image_cfg: dict) -> bool:
    mode = str(image_cfg.get("mode", "")).strip()
    provider = str(image_cfg.get("provider", "")).strip()
    model = str(image_cfg.get("model", "")).strip()

    if mode == "local":
        local_cfg = image_cfg.get("local", {})
        return bool(model and str(local_cfg.get("comfyui_url", "")).strip())

    if not all((provider, model, str(image_cfg.get("api_key", "")).strip())):
        return False
    if provider == "volcengine":
        return bool(str(image_cfg.get("api_secret", "")).strip())
    return True

