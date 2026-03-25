import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.codegen.application.build_trigger import BuildTrigger
from app.modules.codegen.application.prompt_assembler import PromptAssembler
from app.modules.codegen.application.services import CodegenService
from app.modules.codegen.domain.models import AssetCodegenRequest


class FakeKnowledgeSource:
    def load_context(self, context_type: str, asset_type: str | None = None) -> str:
        return f"docs:{asset_type or context_type}"


@pytest.mark.asyncio
async def test_codegen_service_combines_request_context_and_knowledge(tmp_path):
    captured: dict[str, str] = {}

    async def fake_runner(prompt: str, project_root: Path, stream_callback=None) -> str:
        captured["prompt"] = prompt
        return "ok"

    assembler = PromptAssembler(
        knowledge_source=FakeKnowledgeSource(),
        api_lookup_provider=lambda: "api-lookup",
        api_ref_path=Path("api.md"),
    )
    service = CodegenService(
        prompt_assembler=assembler,
        agent_runner=fake_runner,
        build_trigger=BuildTrigger(fake_runner),
    )

    await service.create_asset(
        AssetCodegenRequest(
            design_description="一个燃烧的遗物",
            asset_type="relic",
            asset_name="BurnRelic",
            image_paths=[],
            project_root=tmp_path,
        )
    )

    assert "docs:relic" in captured["prompt"]
    assert "一个燃烧的遗物" in captured["prompt"]
    assert "BurnRelic" in captured["prompt"]
