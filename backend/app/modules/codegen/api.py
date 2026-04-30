from __future__ import annotations

from pathlib import Path

from agents.sts2_guidance import get_game_api_reference_path, get_guidance_for_asset_type, get_planner_guidance
from app.modules.codegen.application.artifact_writer import ArtifactWriter
from app.modules.codegen.application.build_trigger import BuildTrigger
from app.modules.codegen.application.prompt_assembler import PromptAssembler
from app.modules.codegen.application.services import CodegenService
from app.modules.codegen.domain.models import (
    AssetCodegenRequest,
    AssetGroupRequest,
    CustomCodegenRequest,
    ModProjectRequest,
)
from app.modules.knowledge.application.knowledge_facade import build_lookup_context
from app.modules.knowledge.infra.sts2_code_facts_provider import Sts2CodeFactsProvider
from app.modules.knowledge.infra.sts2_guidance_provider import Sts2GuidanceProvider
from app.modules.knowledge.infra.sts2_guidance_source import Sts2GuidanceKnowledgeSource
from app.modules.knowledge.infra.sts2_knowledge_resolver import Sts2KnowledgeResolver
from app.modules.knowledge.infra.sts2_lookup_provider import Sts2LookupProvider
from app.shared.prompting import PromptContextAssembler, PromptLoader
from llm.agent_runner import run_agent_task

_PROMPT_LOADER = PromptLoader()
_LOOKUP_TITLE_KEY = "codegen.lookup_title"
_LOOKUP_BASELIB_KEY = "codegen.lookup_baselib"
_LOOKUP_STS2_LOCAL_KEY = "codegen.lookup_sts2_local"
_LOOKUP_STS2_FALLBACK_KEY = "codegen.lookup_sts2_fallback"


async def run_claude_code(prompt: str, project_root: Path, stream_callback=None) -> str:
    return await run_agent_task(prompt, project_root, stream_callback)


def _build_lookup_section() -> str:
    lookup_context = build_lookup_context()
    title = _PROMPT_LOADER.load(_LOOKUP_TITLE_KEY).strip()
    baselib_note = _PROMPT_LOADER.render(
        _LOOKUP_BASELIB_KEY,
        {
            "baselib_src_path": f"`{lookup_context['baselib_src_path']}`",
        },
    ).strip()

    if lookup_context["game_source_mode"] == "runtime_decompiled":
        sts2_note = _PROMPT_LOADER.render(
            _LOOKUP_STS2_LOCAL_KEY,
            {
                "knowledge_path": f"`{lookup_context['game_path']}`",
            },
        ).strip()
    else:
        sts2_note = _PROMPT_LOADER.render(
            _LOOKUP_STS2_FALLBACK_KEY,
            {
                "ilspy_example_dll_path": f"`{lookup_context['ilspy_example_dll_path']}`",
            },
        ).strip()

    return f"{title}\n{baselib_note}\n\n{sts2_note}"


def _build_codegen_service() -> CodegenService:
    assembler = build_codegen_prompt_assembler()
    return CodegenService(
        prompt_assembler=assembler,
        agent_runner=run_claude_code,
        artifact_writer=ArtifactWriter(),
        build_trigger=BuildTrigger(run_claude_code),
    )


def build_codegen_prompt_assembler() -> PromptAssembler:
    knowledge_source = Sts2GuidanceKnowledgeSource(
        guidance_for_asset_type=get_guidance_for_asset_type,
        planner_guidance=get_planner_guidance,
    )
    knowledge_resolver = Sts2KnowledgeResolver(
        code_facts_provider=Sts2CodeFactsProvider(),
        guidance_provider=Sts2GuidanceProvider(),
        lookup_provider=Sts2LookupProvider(),
    )
    return PromptAssembler(
        knowledge_source=knowledge_source,
        lookup_provider=_build_lookup_section,
        api_ref_path=get_game_api_reference_path(),
        knowledge_resolver=knowledge_resolver,
        prompt_context_assembler=PromptContextAssembler(),
    )


def build_custom_code_prompt(request: CustomCodegenRequest) -> str:
    return build_codegen_prompt_assembler().assemble_custom_code_prompt(request)


def build_asset_prompt(request: AssetCodegenRequest) -> str:
    return build_codegen_prompt_assembler().assemble_asset_prompt(request)


async def create_asset(
    design_description: str,
    asset_type: str,
    asset_name: str,
    image_paths: list[Path],
    project_root: Path,
    stream_callback=None,
    name_zhs: str = "",
    skip_build: bool = False,
) -> str:
    service = _build_codegen_service()
    return await service.create_asset(
        AssetCodegenRequest(
            design_description=design_description,
            asset_type=asset_type,
            asset_name=asset_name,
            image_paths=image_paths,
            project_root=project_root,
            name_zhs=name_zhs,
            skip_build=skip_build,
        ),
        stream_callback,
    )


async def create_custom_code(
    description: str,
    implementation_notes: str,
    name: str,
    project_root: Path,
    stream_callback=None,
    skip_build: bool = False,
) -> str:
    service = _build_codegen_service()
    return await service.create_custom_code(
        CustomCodegenRequest(
            description=description,
            implementation_notes=implementation_notes,
            name=name,
            project_root=project_root,
            skip_build=skip_build,
        ),
        stream_callback,
    )


async def create_asset_group(
    assets: list[dict],
    project_root: Path,
    stream_callback=None,
) -> str:
    service = _build_codegen_service()
    return await service.create_asset_group(
        AssetGroupRequest(assets=assets, project_root=project_root),
        stream_callback,
    )


async def build_and_fix(
    project_root: Path,
    stream_callback=None,
    max_attempts: int = 3,
) -> tuple[bool, str]:
    service = _build_codegen_service()
    return await service.build_and_fix(project_root, max_attempts=max_attempts, stream_callback=stream_callback)


async def create_mod_project(
    project_name: str,
    target_dir: Path,
    stream_callback=None,
) -> Path:
    service = _build_codegen_service()
    return await service.create_mod_project(
        ModProjectRequest(project_name=project_name, target_dir=target_dir),
        stream_callback,
    )


async def package_mod(
    project_root: Path,
    stream_callback=None,
) -> bool:
    service = _build_codegen_service()
    return await service.package_mod(project_root, stream_callback)


__all__ = [
    "build_and_fix",
    "build_asset_prompt",
    "build_codegen_prompt_assembler",
    "build_custom_code_prompt",
    "create_asset",
    "create_asset_group",
    "create_custom_code",
    "create_mod_project",
    "package_mod",
    "run_claude_code",
]
