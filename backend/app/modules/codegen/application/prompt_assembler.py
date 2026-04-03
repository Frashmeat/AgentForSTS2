from __future__ import annotations

from pathlib import Path

from app.modules.codegen.domain.models import AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest
from app.shared.prompting import PromptLoader

_ASSET_PROMPT_KEY = "codegen.asset_prompt"
_CUSTOM_CODE_PROMPT_KEY = "codegen.custom_code_prompt"
_ASSET_GROUP_PROMPT_KEY = "codegen.asset_group_prompt"
_BUILD_PROMPT_KEY = "codegen.build_prompt"
_CREATE_MOD_PROJECT_PROMPT_KEY = "codegen.create_mod_project_prompt"
_PACKAGE_PROMPT_KEY = "codegen.package_prompt"


class PromptAssembler:
    def __init__(self, knowledge_source, api_lookup_provider, api_ref_path: Path, prompt_loader: PromptLoader | None = None) -> None:
        self.knowledge_source = knowledge_source
        self.api_lookup_provider = api_lookup_provider
        self.api_ref_path = api_ref_path
        self.prompt_loader = prompt_loader or PromptLoader()

    @staticmethod
    def _format_prompt_path(path: Path) -> str:
        return path.as_posix()

    def assemble_asset_prompt(self, request: AssetCodegenRequest) -> str:
        docs = self.knowledge_source.load_context("asset", asset_type=request.asset_type)
        img_list = "\n".join(f"  - {self._format_prompt_path(p)}" for p in request.image_paths)
        zhs_hint = f"\nSimplified Chinese display name (name_zhs): {request.name_zhs}" if request.name_zhs else ""
        api_lookup = self.api_lookup_provider()
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
                "api_lookup": api_lookup,
                "api_ref_path": self.api_ref_path,
                "asset_name": request.asset_name,
                "asset_type": request.asset_type,
                "build_step": build_step,
                "design_description": request.design_description,
                "docs": docs,
                "img_list": img_list,
                "mod_name": request.project_root.name,
                "project_root": request.project_root,
                "zhs_hint": zhs_hint,
            },
        )

    def assemble_custom_code_prompt(self, request: CustomCodegenRequest) -> str:
        docs = self.knowledge_source.load_context("asset", asset_type="custom_code")
        api_lookup = self.api_lookup_provider()
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
                "api_lookup": api_lookup,
                "api_ref_path": self.api_ref_path,
                "build_steps": build_steps,
                "description": request.description,
                "docs": docs,
                "implementation_notes": request.implementation_notes,
                "mod_name": mod_name,
                "name": request.name,
                "project_root": request.project_root,
            },
        )

    def assemble_asset_group_prompt(self, request: AssetGroupRequest) -> str:
        seen_types: set[str] = set()
        type_docs_parts: list[str] = []
        common_included = False
        for asset in request.assets:
            asset_type = asset["item"].type
            if asset_type in seen_types:
                continue
            seen_types.add(asset_type)
            doc = self.knowledge_source.load_context("asset", asset_type=asset_type)
            if not common_included:
                type_docs_parts.append(doc)
                common_included = True
            else:
                common_docs = self.knowledge_source.load_context("asset", asset_type="unknown_future_type")
                type_docs_parts.append(doc[len(common_docs):] if doc.startswith(common_docs) else doc)

        combined_docs = "\n\n".join(type_docs_parts)
        api_lookup = self.api_lookup_provider()
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
                "api_lookup": api_lookup,
                "asset_count": len(request.assets),
                "assets_section": assets_section.strip(),
                "class_names": ", ".join(class_names),
                "combined_docs": combined_docs,
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
