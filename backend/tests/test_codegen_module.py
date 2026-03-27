import asyncio
import inspect
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.codegen.application.build_trigger import BuildTrigger
from app.modules.codegen.application.prompt_assembler import PromptAssembler
from app.modules.codegen.application.services import CodegenService
from app.modules.codegen.domain.models import AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest
from app.modules.planning.domain.models import PlanItem
from app.shared.prompting import PromptLoader

_PROMPT_LOADER_SUPPORTED = "prompt_loader" in inspect.signature(PromptAssembler).parameters


class FakeKnowledgeSource:
    def load_context(self, context_type: str, asset_type: str | None = None) -> str:
        return f"docs:{asset_type or context_type}"


def _build_assembler(knowledge_source=None, prompt_loader=None) -> PromptAssembler:
    kwargs = {
        "knowledge_source": knowledge_source or FakeKnowledgeSource(),
        "api_lookup_provider": lambda: "api-lookup",
        "api_ref_path": Path("api.md"),
    }
    if _PROMPT_LOADER_SUPPORTED and prompt_loader is not None:
        kwargs["prompt_loader"] = prompt_loader
    return PromptAssembler(**kwargs)


def _project_root() -> Path:
    return Path("TestMod")


def test_prompt_assembler_accepts_prompt_loader_dependency():
    assert _PROMPT_LOADER_SUPPORTED is True


def test_codegen_service_combines_request_context_and_knowledge():
    captured: dict[str, str] = {}

    async def fake_runner(prompt: str, project_root: Path, stream_callback=None) -> str:
        captured["prompt"] = prompt
        return "ok"

    assembler = _build_assembler()
    service = CodegenService(
        prompt_assembler=assembler,
        agent_runner=fake_runner,
        build_trigger=BuildTrigger(fake_runner),
    )

    asyncio.run(
        service.create_asset(
            AssetCodegenRequest(
                design_description="一个燃烧的遗物",
                asset_type="relic",
                asset_name="BurnRelic",
                image_paths=[],
                project_root=_project_root(),
            )
        )
    )

    assert "docs:relic" in captured["prompt"]
    assert "api-lookup" in captured["prompt"]
    assert "一个燃烧的遗物" in captured["prompt"]
    assert "BurnRelic" in captured["prompt"]
    assert "Run `dotnet publish`" in captured["prompt"]


def test_asset_prompt_includes_skip_build_hint_and_project_variables():
    project_root = _project_root()
    prompt = _build_assembler().assemble_asset_prompt(
        AssetCodegenRequest(
            design_description="一个燃烧的遗物",
            asset_type="relic",
            asset_name="BurnRelic",
            image_paths=[Path("images/burn.png")],
            project_root=project_root,
            name_zhs="燃烧遗物",
            skip_build=True,
        )
    )

    assert "Simplified Chinese display name (name_zhs): 燃烧遗物" in prompt
    assert "Do NOT run dotnet publish" in prompt
    assert f"The Godot resource directory is `{project_root.name}/`" in prompt
    assert "burn.png" in prompt
    assert 'Task: Create a new relic named "BurnRelic".' in prompt


def test_custom_code_prompt_keeps_notes_and_skip_build_guidance():
    project_root = _project_root()
    prompt = _build_assembler().assemble_custom_code_prompt(
        CustomCodegenRequest(
            description="为战斗添加一个自定义 Hook",
            implementation_notes="Use Harmony prefix patch on combat start.",
            name="CombatHook",
            project_root=project_root,
            skip_build=True,
        )
    )

    assert 'Task: Implement a custom code component named "CombatHook".' in prompt
    assert "Use Harmony prefix patch on combat start." in prompt
    assert "Do NOT run dotnet publish" in prompt
    assert "Do not create any image assets." in prompt
    assert f'`{project_root.name}/` — Godot resource dir' in prompt


def test_asset_group_prompt_deduplicates_common_docs_and_keeps_dependency_details():
    class GroupKnowledgeSource:
        def load_context(self, context_type: str, asset_type: str | None = None) -> str:
            common = "COMMON-DOCS"
            docs = {
                "card": f"{common}\nCARD-DOCS",
                "relic": f"{common}\nRELIC-DOCS",
                "unknown_future_type": common,
            }
            return docs.get(asset_type or context_type, common)

    prompt = _build_assembler(knowledge_source=GroupKnowledgeSource()).assemble_asset_group_prompt(
        AssetGroupRequest(
            assets=[
                {
                    "item": PlanItem(
                        id="card_ignite",
                        type="card",
                        name="Ignite",
                        name_zhs="点燃",
                        description="Deal damage and apply burn.",
                        implementation_notes="Use OnPlay.",
                        depends_on=["power_burn"],
                    ),
                    "image_paths": [Path("images/ignite.png")],
                },
                {
                    "item": PlanItem(
                        id="relic_burn",
                        type="relic",
                        name="BurnRelic",
                        description="Buff burn damage.",
                        implementation_notes="Use Flash().",
                        needs_image=False,
                    ),
                    "image_paths": [],
                },
            ],
            project_root=_project_root(),
        )
    )

    assert prompt.count("COMMON-DOCS") == 1
    assert "CARD-DOCS" in prompt
    assert "RELIC-DOCS" in prompt
    assert "Class names in this group: Ignite, BurnRelic" in prompt
    assert "Depends on: power_burn" in prompt
    assert "(no image — code-only asset)" in prompt


def test_codegen_prompt_templates_exist_for_real_loader():
    loader = PromptLoader(root=Path("backend/app/modules/codegen/resources/prompts"))

    template = loader.load("asset_prompt.txt")
    group_template = loader.load("asset_group_prompt.txt")
    build_template = loader.load("build_prompt.txt")
    create_project_template = loader.load("create_mod_project_prompt.txt")
    package_template = loader.load("package_prompt.txt")

    assert 'Task: Create a new {{ asset_type }} named "{{ asset_name }}".{{ zhs_hint }}' in template
    assert "## Task: Create {{ asset_count }} related assets in ONE batch" in group_template
    assert "Repeat until it succeeds or you've tried {{ max_attempts }} times." in build_template
    assert 'Create a new STS2 mod project named "{{ project_name }}" at {{ project_path }}.' in create_project_template
    assert "Build and package this STS2 mod completely:" in package_template


@pytest.mark.skipif(
    not _PROMPT_LOADER_SUPPORTED,
    reason="PromptAssembler template-loader integration has not landed yet.",
)
def test_asset_prompt_renders_codegen_template_with_runtime_variables():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-asset-prompt"

    loader = FakePromptLoader()
    prompt = _build_assembler(prompt_loader=loader).assemble_asset_prompt(
        AssetCodegenRequest(
            design_description="一个燃烧的遗物",
            asset_type="relic",
            asset_name="BurnRelic",
            image_paths=[Path("images/burn.png")],
            project_root=_project_root(),
            name_zhs="燃烧遗物",
            skip_build=True,
        )
    )

    assert prompt == "rendered-asset-prompt"
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "asset_prompt.txt"
    assert variables["docs"] == "docs:relic"
    assert variables["api_lookup"] == "api-lookup"
    assert variables["zhs_hint"] == "\nSimplified Chinese display name (name_zhs): 燃烧遗物"
    assert variables["img_list"] == "  - images/burn.png"
    assert variables["build_step"] == "6. Do NOT run dotnet publish — the build will be done later after all assets are created."
    assert 'Task: Create a new {{ asset_type }} named "{{ asset_name }}".{{ zhs_hint }}' in fallback_template


@pytest.mark.skipif(
    not _PROMPT_LOADER_SUPPORTED,
    reason="PromptAssembler template-loader integration has not landed yet.",
)
def test_asset_group_prompt_renders_codegen_template_with_prepared_sections():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-group-prompt"

    loader = FakePromptLoader()
    prompt = _build_assembler(prompt_loader=loader).assemble_asset_group_prompt(
        AssetGroupRequest(
            assets=[
                {
                    "item": PlanItem(
                        id="card_ignite",
                        type="card",
                        name="Ignite",
                        name_zhs="点燃",
                        description="Deal damage and apply burn.",
                        implementation_notes="Use OnPlay.",
                        depends_on=["power_burn"],
                    ),
                    "image_paths": [Path("images/ignite.png")],
                },
                {
                    "item": PlanItem(
                        id="relic_burn",
                        type="relic",
                        name="BurnRelic",
                        description="Buff burn damage.",
                        implementation_notes="Use Flash().",
                        needs_image=False,
                    ),
                    "image_paths": [],
                },
            ],
            project_root=_project_root(),
        )
    )

    assert prompt == "rendered-group-prompt"
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "asset_group_prompt.txt"
    assert variables["api_lookup"] == "api-lookup"
    assert variables["class_names"] == "Ignite, BurnRelic"
    assert "### Asset 1: [card] Ignite" in variables["assets_section"]
    assert "Depends on: power_burn" in variables["assets_section"]
    assert "(no image — code-only asset)" in variables["assets_section"]
    assert '## Task: Create {{ asset_count }} related assets in ONE batch' in fallback_template


@pytest.mark.skipif(
    not _PROMPT_LOADER_SUPPORTED,
    reason="PromptAssembler template-loader integration has not landed yet.",
)
def test_build_prompt_renders_codegen_template_with_attempt_limit():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-build-prompt"

    loader = FakePromptLoader()
    prompt = _build_assembler(prompt_loader=loader).assemble_build_prompt(4)

    assert prompt == "rendered-build-prompt"
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "build_prompt.txt"
    assert variables == {"max_attempts": 4}
    assert "Repeat until it succeeds or you've tried {{ max_attempts }} times." in fallback_template


@pytest.mark.skipif(
    not _PROMPT_LOADER_SUPPORTED,
    reason="PromptAssembler template-loader integration has not landed yet.",
)
def test_create_mod_project_prompt_renders_codegen_template_with_project_variables():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-project-prompt"

    loader = FakePromptLoader()
    request = ModProjectRequest(project_name="MyCoolMod", target_dir=Path("mods"))
    prompt = _build_assembler(prompt_loader=loader).assemble_create_mod_project_prompt(request)

    assert prompt == "rendered-project-prompt"
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "create_mod_project_prompt.txt"
    assert variables["project_name"] == "MyCoolMod"
    assert variables["project_path"] == Path("mods/MyCoolMod")
    assert 'Create a new STS2 mod project named "{{ project_name }}" at {{ project_path }}.' in fallback_template


@pytest.mark.skipif(
    not _PROMPT_LOADER_SUPPORTED,
    reason="PromptAssembler template-loader integration has not landed yet.",
)
def test_package_prompt_renders_codegen_template():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-package-prompt"

    loader = FakePromptLoader()
    prompt = _build_assembler(prompt_loader=loader).assemble_package_prompt()

    assert prompt == "rendered-package-prompt"
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "package_prompt.txt"
    assert variables == {}
    assert "Build and package this STS2 mod completely:" in fallback_template


def test_codegen_service_build_and_fix_uses_build_prompt():
    captured: dict[str, str] = {}

    async def fake_runner(prompt: str, project_root: Path, stream_callback=None) -> str:
        captured["prompt"] = prompt
        return "Build succeeded"

    service = CodegenService(
        prompt_assembler=_build_assembler(),
        agent_runner=fake_runner,
        build_trigger=BuildTrigger(fake_runner),
    )

    success, output = asyncio.run(service.build_and_fix(_project_root(), max_attempts=4))

    assert success is True
    assert output == "Build succeeded"
    assert "you've tried 4 times" in captured["prompt"]


@pytest.mark.skipif(
    not _PROMPT_LOADER_SUPPORTED,
    reason="PromptAssembler template-loader integration has not landed yet.",
)
def test_codegen_service_build_and_fix_uses_template_backed_build_prompt():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return f"rendered-build:{variables['max_attempts']}"

    captured: dict[str, object] = {}

    async def fake_runner(prompt: str, project_root: Path, stream_callback=None) -> str:
        captured["prompt"] = prompt
        captured["project_root"] = project_root
        return "Build succeeded"

    loader = FakePromptLoader()
    service = CodegenService(
        prompt_assembler=_build_assembler(prompt_loader=loader),
        agent_runner=fake_runner,
        build_trigger=BuildTrigger(fake_runner),
    )

    success, output = asyncio.run(service.build_and_fix(_project_root(), max_attempts=5))

    assert success is True
    assert output == "Build succeeded"
    assert captured["prompt"] == "rendered-build:5"
    assert captured["project_root"] == _project_root()
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "build_prompt.txt"
    assert variables == {"max_attempts": 5}
    assert "Repeat until it succeeds or you've tried {{ max_attempts }} times." in fallback_template


@pytest.mark.skipif(
    not _PROMPT_LOADER_SUPPORTED,
    reason="PromptAssembler template-loader integration has not landed yet.",
)
def test_codegen_service_create_mod_project_uses_template_backed_project_prompt():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return f"rendered-project:{variables['project_name']}"

    captured: dict[str, object] = {}

    async def fake_runner(prompt: str, project_root: Path, stream_callback=None) -> str:
        captured["prompt"] = prompt
        captured["project_root"] = project_root
        return "created"

    loader = FakePromptLoader()
    request = ModProjectRequest(project_name="MyCoolMod", target_dir=Path("mods"))
    service = CodegenService(
        prompt_assembler=_build_assembler(prompt_loader=loader),
        agent_runner=fake_runner,
        build_trigger=BuildTrigger(fake_runner),
    )

    created_path = asyncio.run(service.create_mod_project(request))

    assert created_path == Path("mods/MyCoolMod")
    assert captured["prompt"] == "rendered-project:MyCoolMod"
    assert captured["project_root"] == Path("mods")
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "create_mod_project_prompt.txt"
    assert variables["project_name"] == "MyCoolMod"
    assert variables["project_path"] == Path("mods/MyCoolMod")
    assert 'Create a new STS2 mod project named "{{ project_name }}" at {{ project_path }}.' in fallback_template


@pytest.mark.skipif(
    not _PROMPT_LOADER_SUPPORTED,
    reason="PromptAssembler template-loader integration has not landed yet.",
)
def test_codegen_service_package_mod_uses_template_backed_package_prompt():
    class FakePromptLoader:
        def __init__(self) -> None:
            self.calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.calls.append((template_name, variables, fallback_template or ""))
            return "rendered-package"

    captured: dict[str, object] = {}

    async def fake_runner(prompt: str, project_root: Path, stream_callback=None) -> str:
        captured["prompt"] = prompt
        captured["project_root"] = project_root
        return "Build succeeded\n0 Error(s)"

    loader = FakePromptLoader()
    service = CodegenService(
        prompt_assembler=_build_assembler(prompt_loader=loader),
        agent_runner=fake_runner,
        build_trigger=BuildTrigger(fake_runner),
    )

    packaged = asyncio.run(service.package_mod(_project_root()))

    assert packaged is True
    assert captured["prompt"] == "rendered-package"
    assert captured["project_root"] == _project_root()
    assert len(loader.calls) == 1
    template_name, variables, fallback_template = loader.calls[0]
    assert template_name == "package_prompt.txt"
    assert variables == {}
    assert "Build and package this STS2 mod completely:" in fallback_template
