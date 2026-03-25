from __future__ import annotations

from typing import Any

from app.modules.image.application.ports import ImagePostProcessor, ImageProvider, PromptOptimizer
from app.modules.image.domain.models import ImageGenerationRequest, ImagePostProcessRequest


class ImageService:
    def __init__(
        self,
        providers: dict[str, ImageProvider],
        prompt_optimizer: PromptOptimizer | None = None,
        postprocessors: list[ImagePostProcessor] | None = None,
    ) -> None:
        self.providers = providers
        self.prompt_optimizer = prompt_optimizer
        self.postprocessors = postprocessors or []

    async def generate(
        self,
        request: ImageGenerationRequest,
        progress_callback=None,
    ) -> list[Any]:
        images = await self.providers[request.provider].generate(request, progress_callback)
        for postprocessor in self.postprocessors:
            images = await postprocessor.process(
                images,
                ImagePostProcessRequest(
                    asset_type=request.asset_type,
                    name=request.options.get("name", ""),
                    project_root=request.options.get("project_root"),
                ),
            )
        return images

    async def optimize_prompt(
        self,
        user_description: str,
        asset_type: str,
        provider: str,
        needs_transparent_bg: bool,
    ) -> dict[str, Any]:
        if self.prompt_optimizer is None:
            raise RuntimeError("Prompt optimizer is not configured")
        return await self.prompt_optimizer.optimize(user_description, asset_type, provider, needs_transparent_bg)
