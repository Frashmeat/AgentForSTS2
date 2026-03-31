from __future__ import annotations

from app.shared.prompting import PromptLoader
from config import get_config, update_config


class ConfigFacadeService:
    def __init__(self) -> None:
        self._text_loader = PromptLoader()

    def get_masked_config(self) -> dict:
        return self._mask_keys(get_config())

    def patch_config(self, body: dict) -> dict:
        patch = dict(body)
        for section_key in ("llm", "image_gen"):
            section = dict(patch.get(section_key, {}))
            for field in ("api_key", "api_secret"):
                if isinstance(section.get(field), str) and section[field].startswith("****"):
                    section.pop(field, None)
            if section:
                patch[section_key] = section
            elif section_key in patch:
                patch.pop(section_key, None)
        return self._mask_keys(update_config(patch))

    def detect_paths(self):
        from project_utils import detect_paths as _detect

        return _detect()

    async def test_imggen(self):
        from image.generator import generate_images

        try:
            prompt = self._text_loader.load("runtime_system.config_image_test_prompt").strip()
            imgs = await generate_images(prompt, "power", batch_size=1)
            return {"ok": True, "size": list(imgs[0].size)}
        except Exception as exc:
            return {"ok": False, "error": str(exc)[:300]}

    def _mask_keys(self, cfg: dict) -> dict:
        import copy

        masked = copy.deepcopy(cfg)
        for section in (masked.get("llm", {}), masked.get("image_gen", {})):
            for field in ("api_key", "api_secret"):
                if section.get(field):
                    value = section[field]
                    section[field] = f"****{value[-4:]}" if len(value) > 4 else "****"
        return masked
