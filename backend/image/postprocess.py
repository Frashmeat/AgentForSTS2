"""
图片后处理：抠图、缩放、生成 variant、特效
"""
from __future__ import annotations

import io
from pathlib import Path
from typing import Literal

import numpy as np
from PIL import Image, ImageFilter

from app.modules.image.domain.models import ImagePostProcessRequest
from app.modules.image.infra.postprocess import FunctionalImagePostProcessor, ImagePostProcessPipeline

# rembg 懒加载，仅在需要透明处理时导入
_rembg_session = None
_rembg_session_model: str | None = None


def _get_gpu_providers() -> list[str]:
    """检测 ONNX Runtime 是否支持 CUDA，有 GPU 则优先用。"""
    try:
        import onnxruntime as ort
        if "CUDAExecutionProvider" in ort.get_available_providers():
            return ["CUDAExecutionProvider", "CPUExecutionProvider"]
    except Exception:
        pass
    return ["CPUExecutionProvider"]


def _get_rembg_session():
    global _rembg_session, _rembg_session_model
    from config import get_config
    model = get_config().get("image_gen", {}).get("rembg_model", "birefnet-general")
    if _rembg_session is None or _rembg_session_model != model:
        from rembg import new_session
        _rembg_session = new_session(model, providers=_get_gpu_providers())
        _rembg_session_model = model
    return _rembg_session


# ── 规格定义 ────────────────────────────────────────────────────────────────

AssetType = Literal["card", "card_fullscreen", "relic", "power", "character"]

PROFILES: dict[AssetType, dict] = {
    "card": {
        "bg": "opaque",
        "variants": [
            {"rel_path": "images/card_portraits/{name}.png",     "size": (250, 190)},
            {"rel_path": "images/card_portraits/big/{name}.png", "size": (1000, 760)},
        ],
    },
    "card_fullscreen": {
        "bg": "opaque",
        "variants": [
            {"rel_path": "images/card_portraits/{name}.png",     "size": (250, 350)},
            {"rel_path": "images/card_portraits/big/{name}.png", "size": (606, 852)},
        ],
    },
    "relic": {
        "bg": "transparent",
        "variants": [
            {"rel_path": "images/relics/{name}.png",         "size": (94, 94)},
            {"rel_path": "images/relics/{name}_outline.png", "size": (94, 94),  "effect": "outline"},
            {"rel_path": "images/relics/big/{name}.png",     "size": (256, 256)},
        ],
    },
    "power": {
        "bg": "transparent",
        "variants": [
            {"rel_path": "images/powers/{name}.png",         "size": (64, 64)},
            {"rel_path": "images/powers/big/{name}.png",     "size": (256, 256)},
        ],
    },
    "character": {
        "variants": [
            {"rel_path": "images/charui/character_icon_{name}.png",    "size": (128, 128), "bg": "transparent"},
            {"rel_path": "images/charui/char_select_{name}.png",       "size": (132, 195), "bg": "opaque"},
            {"rel_path": "images/charui/char_select_{name}_locked.png","size": (132, 195), "bg": "opaque", "effect": "locked"},
            {"rel_path": "images/charui/map_marker_{name}.png",        "size": (128, 128), "bg": "transparent"},
        ],
    },
}


# ── 核心处理函数 ─────────────────────────────────────────────────────────────

def remove_background(img: Image.Image) -> Image.Image:
    """使用 rembg BiRefNet 抠图，返回 RGBA。"""
    from rembg import remove
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    result_bytes = remove(buf.getvalue(), session=_get_rembg_session())
    return Image.open(io.BytesIO(result_bytes)).convert("RGBA")


def make_outline(img_rgba: Image.Image, outline_px: int = 3) -> Image.Image:
    """
    从 RGBA 图生成白色轮廓图（周围透明）。
    原理：膨胀 alpha 通道 → 减去原 alpha → 填白色。
    """
    alpha = np.array(img_rgba.split()[-1])
    # 膨胀
    from PIL import ImageFilter
    alpha_img = Image.fromarray(alpha)
    dilated = alpha_img.filter(ImageFilter.MaxFilter(outline_px * 2 + 1))
    dilated_arr = np.array(dilated)
    # 轮廓 = 膨胀后 - 原始
    outline_alpha = np.clip(dilated_arr.astype(int) - alpha.astype(int), 0, 255).astype(np.uint8)
    # 白色填充
    result = Image.new("RGBA", img_rgba.size, (255, 255, 255, 0))
    result.putalpha(Image.fromarray(outline_alpha))
    return result


def make_locked(img: Image.Image, brightness: float = 0.55) -> Image.Image:
    """选角锁定图：灰度 + 压暗。"""
    gray = img.convert("L").convert("RGB")
    darkened = gray.point(lambda p: int(p * brightness))
    if img.mode == "RGBA":
        darkened = darkened.convert("RGBA")
        darkened.putalpha(img.split()[-1])
    return darkened


def process_image(
    source_img: Image.Image,
    asset_type: AssetType,
    name: str,
    project_root: Path,
) -> list[Path]:
    """
    对选中图片执行全套后处理，写入 mod 项目目录。
    返回生成的所有文件路径列表。
    """
    pipeline = ImagePostProcessPipeline([FunctionalImagePostProcessor(_write_processed_variants)])
    result = _run_pipeline_sync(
        pipeline.run(
            [source_img],
            ImagePostProcessRequest(asset_type=asset_type, name=name, project_root=project_root),
        )
    )
    return result


async def _write_processed_variants(images: list[Image.Image], request: ImagePostProcessRequest) -> list[Path]:
    source_img = images[0]
    asset_type = request.asset_type
    name = request.name
    project_root = request.project_root
    profile = PROFILES[asset_type]
    variants = profile.get("variants", [])
    # character 类型每个 variant 有自己的 bg，其余类型共用 profile 级 bg
    global_bg = profile.get("bg")

    # 对于需要透明的类型，先抠图一次，后续 variant 复用
    base_transparent: Image.Image | None = None
    if global_bg == "transparent":
        base_transparent = remove_background(source_img)

    written: list[Path] = []

    for variant in variants:
        variant_bg = variant.get("bg", global_bg)
        effect = variant.get("effect")
        size: tuple[int, int] = variant["size"]
        rel_path: str = variant["rel_path"].replace("{name}", name)
        # 图片放在 project_root/<mod_name>/images/... 下
        # 这样 Godot res:// 路径为 <mod_name>/images/...，与代码引用一致
        out_path = project_root / project_root.name / rel_path
        out_path.parent.mkdir(parents=True, exist_ok=True)

        # 选取基底图
        if variant_bg == "transparent":
            if base_transparent is None:
                base = remove_background(source_img)
            else:
                base = base_transparent.copy()
        else:
            base = source_img.convert("RGB")

        # 应用特效
        if effect == "outline":
            base = make_outline(base_transparent or base)
        elif effect == "locked":
            base = make_locked(base)

        # 缩放
        final = base.resize(size, Image.LANCZOS)

        # 保存
        final.save(out_path, format="PNG")
        written.append(out_path)

    return written


def _run_pipeline_sync(coro):
    import asyncio

    try:
        loop = asyncio.get_running_loop()
    except RuntimeError:
        return asyncio.run(coro)

    raise RuntimeError("process_image should be called from a worker thread when event loop is running")
