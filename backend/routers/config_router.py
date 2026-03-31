from fastapi import APIRouter
from app.shared.prompting import PromptLoader
from config import get_config, update_config

router = APIRouter(prefix="/config")
_TEXT_LOADER = PromptLoader()


@router.get("")
def get_cfg():
    cfg = get_config()
    # 返回前脱敏 API key（只显示后4位）
    safe = _mask_keys(cfg)
    return safe


@router.patch("")
def patch_cfg(body: dict):
    # Don't overwrite keys with masked placeholder values (e.g. "****yYWE")
    for section_key in ("llm", "image_gen"):
        section = body.get(section_key, {})
        for field in ("api_key", "api_secret"):
            if isinstance(section.get(field), str) and section[field].startswith("****"):
                section.pop(field, None)
    updated = update_config(body)
    return _mask_keys(updated)


@router.get("/detect_paths")
def detect_paths():
    """自动检测 STS2 和 Godot 路径，返回检测结果供用户确认后填入配置。"""
    from project_utils import detect_paths as _detect
    return _detect()


@router.get("/test_imggen")
async def test_imggen():
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

