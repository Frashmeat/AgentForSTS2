from __future__ import annotations

from typing import Any, Awaitable, Callable, Protocol

from app.modules.image.domain.models import ImageGenerationRequest, ImagePostProcessRequest


ImageProgressCallback = Callable[[str], Awaitable[None]]


class PromptOptimizer(Protocol):
    async def optimize(
        self,
        user_description: str,
        asset_type: str,
        provider: str,
        needs_transparent_bg: bool,
    ) -> dict[str, Any]: ...


class ImageProvider(Protocol):
    async def generate(
        self,
        request: ImageGenerationRequest,
        progress_callback: ImageProgressCallback | None = None,
    ) -> list[Any]: ...


class ImagePostProcessor(Protocol):
    async def process(
        self,
        images: list[Any],
        request: ImagePostProcessRequest,
    ) -> list[Any]: ...
