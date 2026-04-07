from __future__ import annotations

from fastapi import HTTPException

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

        try:
            return _detect()
        except HTTPException:
            raise
        except Exception as exc:
            raise HTTPException(status_code=500, detail=str(exc)) from exc

    def pick_path(self, body: dict) -> dict:
        from project_utils import pick_path as _pick

        try:
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

    def get_local_ai_capability_status(self) -> dict:
        config = get_config()
        return {
            "text_ai_available": self._has_text_ai(config.get("llm", {})),
            "image_ai_available": self._has_image_ai(config.get("image_gen", {})),
        }

    async def test_imggen(self):
        from image.generator import generate_images

        try:
            prompt = self._text_loader.load("runtime_system.config_image_test_prompt").strip()
            imgs = await generate_images(prompt, "power", batch_size=1)
            return {"ok": True, "size": list(imgs[0].size)}
        except Exception as exc:
            raise HTTPException(status_code=500, detail=str(exc)[:300]) from exc

    def _mask_keys(self, cfg: dict) -> dict:
        import copy

        masked = copy.deepcopy(cfg)
        for section in (masked.get("llm", {}), masked.get("image_gen", {})):
            for field in ("api_key", "api_secret"):
                if section.get(field):
                    value = section[field]
                    section[field] = f"****{value[-4:]}" if len(value) > 4 else "****"
        return masked

    def _has_text_ai(self, llm_cfg: dict) -> bool:
        mode = str(llm_cfg.get("mode", "")).strip()
        if mode == "agent_cli":
            return str(llm_cfg.get("agent_backend", "")).strip() in {"claude", "codex"}
        return all(str(llm_cfg.get(field, "")).strip() for field in ("provider", "model", "api_key"))

    def _has_image_ai(self, image_cfg: dict) -> bool:
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
