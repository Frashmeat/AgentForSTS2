"""
Code Agent：通过 subprocess 调用 Claude Code CLI。
负责生成/修改 C# 代码、dotnet build、错误修复、打包。
"""
from __future__ import annotations

from pathlib import Path

from agents.sts2_docs import API_REF_PATH, BASELIB_SRC_PATH, _get_docs_for_type_raw, _get_planner_api_hints_raw
from app.modules.codegen.application.artifact_writer import ArtifactWriter
from app.modules.codegen.application.build_trigger import BuildTrigger
from app.modules.codegen.application.prompt_assembler import PromptAssembler
from app.modules.codegen.application.services import CodegenService
from app.modules.codegen.domain.models import AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest
from app.modules.knowledge.infra.sts2_docs_source import Sts2DocsKnowledgeSource
from config import get_decompiled_src_path
from llm.agent_runner import run_agent_task


async def run_claude_code(prompt: str, project_root: Path, stream_callback=None) -> str:
    """兼容旧调用名，实际走统一 agent runner。"""
    return await run_agent_task(prompt, project_root, stream_callback)


# ── API lookup prompt 段落（根据配置动态生成）────────────────────────────────

def _build_api_lookup_section() -> str:
    """
    生成注入到 agent prompt 的 API 查找指引段落。
    - 若 decompiled_src_path 已配置：告知 agent 可直接 Read/Grep
    - 否则：告知用 ilspycmd（降级模式）
    BaseLib 反编译始终可用（在仓库内）。
    """
    baselib_note = (
        f"BaseLib (Alchyr.Sts2.BaseLib) decompiled source: `{BASELIB_SRC_PATH}`\n"
        "Read this file directly for CustomCardModel, CustomPotionModel, PlaceholderCharacterModel, etc.\n"
        "Do NOT curl GitHub for BaseLib — the local decompiled copy is authoritative."
    )

    decompiled_path = get_decompiled_src_path()
    if decompiled_path:
        sts2_note = (
            f"Full decompiled sts2.dll source: `{decompiled_path}` (Read/Grep directly).\n"
            "Key subdirs: `MegaCrit.Sts2.Core.Commands\\` (DamageCmd, PowerCmd, CreatureCmd…),\n"
            "`MegaCrit.Sts2.Core.Models.Cards\\` (StrikeIronclad etc.), `MegaCrit.Sts2.Core.Models\\`.\n"
            "Only fall back to ilspycmd if a specific class is missing from this directory."
        )
    else:
        sts2_note = (
            "sts2.dll decompiled source is NOT available on this machine.\n"
            "Use `ilspycmd <path_to_sts2.dll>` to look up specific classes when needed.\n"
            "Game DLL is typically at: `<STS2GamePath>/data_sts2_windows_x86_64/sts2.dll`"
        )

    return f"## API Lookup\n{baselib_note}\n\n{sts2_note}"


def _build_codegen_service() -> CodegenService:
    knowledge_source = Sts2DocsKnowledgeSource(
        docs_for_type=_get_docs_for_type_raw,
        planner_hints=_get_planner_api_hints_raw,
    )
    assembler = PromptAssembler(
        knowledge_source=knowledge_source,
        api_lookup_provider=_build_api_lookup_section,
        api_ref_path=API_REF_PATH,
    )
    return CodegenService(
        prompt_assembler=assembler,
        agent_runner=run_claude_code,
        artifact_writer=ArtifactWriter(),
        build_trigger=BuildTrigger(run_claude_code),
    )


# ── 高层任务 ─────────────────────────────────────────────────────────────────

async def create_asset(
    design_description: str,
    asset_type: str,          # "card" | "relic" | "power" | "character"
    asset_name: str,
    image_paths: list[Path],  # 后处理后的图片路径列表
    project_root: Path,
    stream_callback=None,
    name_zhs: str = "",       # Simplified Chinese display name (optional)
    skip_build: bool = False, # True = skip dotnet publish (batch mode: build once at end)
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
    assets: list[dict],   # [{"item": PlanItem, "image_paths": list[Path]}]
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
