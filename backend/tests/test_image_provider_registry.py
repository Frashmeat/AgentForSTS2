import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.image.application.services import ImageService
from app.modules.image.domain.models import ImageGenerationRequest


class FakeProvider:
    async def generate(self, request: ImageGenerationRequest, progress_callback=None):
        return [f"generated:{request.prompt}:{request.asset_type}"]


class FakePostProcessor:
    async def process(self, images, request: ImageGenerationRequest):
        return [f"{image}:processed" for image in images]


@pytest.mark.asyncio
async def test_image_service_selects_named_provider_and_runs_postprocess():
    service = ImageService(
        providers={"mock": FakeProvider()},
        postprocessors=[FakePostProcessor()],
    )
    request = ImageGenerationRequest(
        provider="mock",
        prompt="glowing relic",
        asset_type="relic",
        batch_size=1,
    )

    result = await service.generate(request)

    assert result == ["generated:glowing relic:relic:processed"]
