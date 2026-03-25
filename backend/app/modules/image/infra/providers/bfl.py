from __future__ import annotations

from app.modules.image.domain.models import ImageGenerationRequest


class BflImageProvider:
    def __init__(self, generate_fn) -> None:
        self._generate_fn = generate_fn

    async def generate(self, request: ImageGenerationRequest, progress_callback=None):
        return await self._generate_fn(request, progress_callback)
