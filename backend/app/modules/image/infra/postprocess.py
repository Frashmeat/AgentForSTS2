from __future__ import annotations

from typing import Any

from app.modules.image.domain.models import ImagePostProcessRequest


class FunctionalImagePostProcessor:
    def __init__(self, process_fn) -> None:
        self._process_fn = process_fn

    async def process(self, images: list[Any], request: ImagePostProcessRequest) -> list[Any]:
        return await self._process_fn(images, request)


class ImagePostProcessPipeline:
    def __init__(self, processors: list[FunctionalImagePostProcessor] | None = None) -> None:
        self.processors = processors or []

    async def run(self, images: list[Any], request: ImagePostProcessRequest) -> list[Any]:
        current = images
        for processor in self.processors:
            current = await processor.process(current, request)
        return current
