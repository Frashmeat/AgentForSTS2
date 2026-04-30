from __future__ import annotations

from pathlib import Path

from app.modules.codegen.domain.models import (
    AssetCodegenRequest,
    AssetGroupRequest,
    CustomCodegenRequest,
    ModProjectRequest,
)
from app.shared.contracts.knowledge import KnowledgeQuery
from app.shared.prompting import PromptContextAssembler, PromptLoader

_ASSET_PROMPT_KEY = "codegen.asset_prompt"
_CUSTOM_CODE_PROMPT_KEY = "codegen.custom_code_prompt"
_ASSET_GROUP_PROMPT_KEY = "codegen.asset_group_prompt"
_BUILD_PROMPT_KEY = "codegen.build_prompt"
_CREATE_MOD_PROJECT_PROMPT_KEY = "codegen.create_mod_project_prompt"
_PACKAGE_PROMPT_KEY = "codegen.package_prompt"


class PromptAssembler:
    def __init__(
        self,
        knowledge_source,
        lookup_provider,
        api_ref_path: Path,
        prompt_loader: PromptLoader | None = None,
        knowledge_resolver=None,
        prompt_context_assembler: PromptContextAssembler | None = None,
    ) -> None:
        self.knowledge_source = knowledge_source
        self.lookup_provider = lookup_provider
        self.api_ref_path = api_ref_path
        self.prompt_loader = prompt_loader or PromptLoader()
        self.knowledge_resolver = knowledge_resolver
        self.prompt_context_assembler = prompt_context_assembler or PromptContextAssembler()

    @staticmethod
    def _format_prompt_path(path: Path) -> str:
        return path.as_posix()

    def _build_asset_query(self, request: AssetCodegenRequest) -> KnowledgeQuery:
        return KnowledgeQuery(
            scenario="asset_codegen",
            domain="sts2",
            asset_type=request.asset_type,
            project_root=request.project_root,
            requirements=request.design_description,
            item_name=request.asset_name,
        )

    def _build_custom_code_query(self, request: CustomCodegenRequest) -> KnowledgeQuery:
        return KnowledgeQuery(
            scenario="custom_code_codegen",
            domain="sts2",
            asset_type="custom_code",
            project_root=request.project_root,
            requirements=request.description,
            item_name=request.name,
        )

    def _build_asset_group_query(self, request: AssetGroupRequest) -> KnowledgeQuery:
        return KnowledgeQuery(
            scenario="asset_group_codegen",
            domain="sts2",
            project_root=request.project_root,
            group_asset_types=[asset["item"].type for asset in request.assets],
            symbols=[asset["item"].name for asset in request.assets],
        )

    @staticmethod
    def _build_legacy_prompt_knowledge(guidance_text: str, lookup_text: str) -> dict[str, str]:
        guidance = guidance_text.strip()
        lookup = lookup_text.strip()
        facts = (
            "Structured code facts are unavailable in this legacy fallback path. "
            "Read `MainFile.cs`, the project `.csproj`, and the lookup sources below to verify exact base classes, "
            "signatures, resource paths, and registration behavior before writing code."
        )
        warning = (
            "### Warnings\n"
            "- Legacy knowledge fallback is active in this prompt. Treat the guidance summary as best-effort context, "
            "not as the authoritative code fact source."
        )
        return {
            "facts": facts,
            "guidance": guidance,
            "lookup": lookup,
            "knowledge_warnings": warning,
            "summary": "legacy knowledge fallback",
        }

    def _resolve_prompt_knowledge(self, query: KnowledgeQuery) -> dict[str, str]:
        if self.knowledge_resolver is None:
            return {
                "facts": "",
                "guidance": "",
                "lookup": "",
                "knowledge_warnings": "",
                "summary": "",
            }

        packet = self.knowledge_resolver.resolve(query)
        context = self.prompt_context_assembler.assemble(packet)
        facts = context.get("facts", "")
        guidance = context.get("guidance", "")
        return {
            "facts": facts,
            "guidance": guidance,
            "lookup": context.get("lookup", ""),
            "knowledge_warnings": context.get("knowledge_warnings", ""),
            "summary": context.get("summary", ""),
        }

    def assemble_asset_prompt(self, request: AssetCodegenRequest) -> str:
        knowledge = self._resolve_prompt_knowledge(self._build_asset_query(request))
        legacy_guidance = ""
        legacy_lookup = ""
        if not any(knowledge[key].strip() for key in ("facts", "guidance", "lookup")):
            legacy_guidance = self.knowledge_source.load_context("asset", asset_type=request.asset_type)
            legacy_lookup = self.lookup_provider()
            knowledge = self._build_legacy_prompt_knowledge(legacy_guidance, legacy_lookup)
        if not knowledge["guidance"]:
            legacy_guidance = self.knowledge_source.load_context("asset", asset_type=request.asset_type)
            knowledge["guidance"] = legacy_guidance.strip()
        if not knowledge["lookup"]:
            legacy_lookup = self.lookup_provider()
            knowledge["lookup"] = legacy_lookup.strip()
        img_list = "\n".join(f"  - {self._format_prompt_path(p)}" for p in request.image_paths)
        zhs_hint = f"\nSimplified Chinese display name (name_zhs): {request.name_zhs}" if request.name_zhs else ""
        build_note = (
            "NOTE: Godot headless export always exits with code -1, but if MSBuild reports '0 Error(s)' "
            "and the overall dotnet exit code is 0 — that is SUCCESS. Do NOT re-run just because of Godot's -1."
        )
        if request.skip_build:
            build_step = "6. Do NOT run dotnet publish — the build will be done later after all assets are created."
        else:
            build_step = (
                f"6. Run `dotnet publish` (NOT dotnet build) to compile AND export the Godot .pck file.\n"
                f"   {build_note}\n"
                f"   Fix any actual compilation errors and re-run until it succeeds.\n"
                f"7. Confirm both the .dll and .pck were deployed to the mods folder."
            )
        return self.prompt_loader.render(
            _ASSET_PROMPT_KEY,
            {
                "api_ref_path": self.api_ref_path,
                "asset_name": request.asset_name,
                "asset_type": request.asset_type,
                "build_step": build_step,
                "design_description": request.design_description,
                "facts": knowledge["facts"],
                "guidance": knowledge["guidance"],
                "lookup": knowledge["lookup"],
                "knowledge_warnings": knowledge["knowledge_warnings"],
                "img_list": img_list,
                "mod_name": request.project_root.name,
                "project_root": request.project_root,
                "zhs_hint": zhs_hint,
            },
        )

    def assemble_custom_code_prompt(self, request: CustomCodegenRequest) -> str:
        knowledge = self._resolve_prompt_knowledge(self._build_custom_code_query(request))
        legacy_guidance = ""
        legacy_lookup = ""
        if not any(knowledge[key].strip() for key in ("facts", "guidance", "lookup")):
            legacy_guidance = self.knowledge_source.load_context("asset", asset_type="custom_code")
            legacy_lookup = self.lookup_provider()
            knowledge = self._build_legacy_prompt_knowledge(legacy_guidance, legacy_lookup)
        if not knowledge["guidance"]:
            legacy_guidance = self.knowledge_source.load_context("asset", asset_type="custom_code")
            knowledge["guidance"] = legacy_guidance.strip()
        if not knowledge["lookup"]:
            legacy_lookup = self.lookup_provider()
            knowledge["lookup"] = legacy_lookup.strip()
        build_note = (
            "NOTE: Godot headless export always exits with code -1, but if MSBuild reports '0 Error(s)' "
            "and the overall dotnet exit code is 0 — that is SUCCESS. Do NOT re-run just because of Godot's -1."
        )
        if request.skip_build:
            build_steps = "5. Do NOT run dotnet publish — the build will be done later after all assets are created."
        else:
            build_steps = (
                f"5. Run `dotnet publish` (NOT dotnet build). {build_note}\n"
                f"   Fix any actual compilation errors and re-run until it succeeds.\n"
                f"6. Confirm the build succeeded and files deployed to mods folder."
            )
        mod_name = request.project_root.name
        return self.prompt_loader.render(
            _CUSTOM_CODE_PROMPT_KEY,
            {
                "api_ref_path": self.api_ref_path,
                "build_steps": build_steps,
                "description": request.description,
                "facts": knowledge["facts"],
                "guidance": knowledge["guidance"],
                "lookup": knowledge["lookup"],
                "knowledge_warnings": knowledge["knowledge_warnings"],
                "implementation_notes": request.implementation_notes,
                "mod_name": mod_name,
                "name": request.name,
                "project_root": request.project_root,
            },
        )

    def _legacy_group_guidance(self, request: AssetGroupRequest) -> str:
        seen_types: set[str] = set()
        type_guidance_parts: list[str] = []
        common_included = False
        for asset in request.assets:
            asset_type = asset["item"].type
            if asset_type in seen_types:
                continue
            seen_types.add(asset_type)
            guidance = self.knowledge_source.load_context("asset", asset_type=asset_type)
            if not common_included:
                type_guidance_parts.append(guidance)
                common_included = True
            else:
                common_guidance = self.knowledge_source.load_context("asset", asset_type="unknown_future_type")
                type_guidance_parts.append(
                    guidance[len(common_guidance) :] if guidance.startswith(common_guidance) else guidance
                )
        return "\n\n".join(type_guidance_parts)

    def assemble_asset_group_prompt(self, request: AssetGroupRequest) -> str:
        knowledge = self._resolve_prompt_knowledge(self._build_asset_group_query(request))
        legacy_guidance = ""
        legacy_lookup = ""
        if not any(knowledge[key].strip() for key in ("facts", "guidance", "lookup")):
            legacy_guidance = self._legacy_group_guidance(request)
            legacy_lookup = self.lookup_provider()
            knowledge = self._build_legacy_prompt_knowledge(legacy_guidance, legacy_lookup)
        if not knowledge["guidance"]:
            legacy_guidance = self._legacy_group_guidance(request)
            knowledge["guidance"] = legacy_guidance.strip()
        if not knowledge["lookup"]:
            legacy_lookup = self.lookup_provider()
            knowledge["lookup"] = legacy_lookup.strip()
        assets_section = ""
        class_names = [asset["item"].name for asset in request.assets]
        for index, asset in enumerate(request.assets, 1):
            item = asset["item"]
            img_paths = asset["image_paths"]
            img_list = (
                "\n".join(f"      - {self._format_prompt_path(p)}" for p in img_paths)
                if img_paths
                else "      (no image — code-only asset)"
            )
            zhs = f"\n  - Chinese name: {item.name_zhs}" if item.name_zhs else ""
            assets_section += f"""
### Asset {index}: [{item.type}] {item.name}{zhs}
  - Description: {item.description}
  - Implementation notes: {item.implementation_notes}
  - Image files:
{img_list}
  - Depends on: {', '.join(item.depends_on) if item.depends_on else 'none'}
"""

        mod_name = request.project_root.name
        return self.prompt_loader.render(
            _ASSET_GROUP_PROMPT_KEY,
            {
                "asset_count": len(request.assets),
                "assets_section": assets_section.strip(),
                "class_names": ", ".join(class_names),
                "facts": knowledge["facts"],
                "guidance": knowledge["guidance"],
                "lookup": knowledge["lookup"],
                "knowledge_warnings": knowledge["knowledge_warnings"],
                "mod_name": mod_name,
                "project_root": request.project_root,
            },
        )

    def assemble_build_prompt(self, max_attempts: int) -> str:
        return self.prompt_loader.render(
            _BUILD_PROMPT_KEY,
            {"max_attempts": max_attempts},
        )

    def assemble_create_mod_project_prompt(self, request: ModProjectRequest) -> str:
        project_path = request.target_dir / request.project_name
        return self.prompt_loader.render(
            _CREATE_MOD_PROJECT_PROMPT_KEY,
            {
                "project_name": request.project_name,
                "project_path": project_path,
            },
        )

    def assemble_package_prompt(self) -> str:
        return self.prompt_loader.render(
            _PACKAGE_PROMPT_KEY,
            {},
        )
