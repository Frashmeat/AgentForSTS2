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

_ASSET_PROMPT_TEMPLATE = """You are an expert STS2 (Slay the Spire 2) mod developer using Godot 4 + C# + BaseLib (Alchyr.Sts2.BaseLib).

{{ docs }}

---

{{ api_lookup }}

---

Task: Create a new {{ asset_type }} named "{{ asset_name }}".{{ zhs_hint }}

Design description (Chinese):
{{ design_description }}

Image assets already generated and placed at:
{{ img_list }}

## Project already initialized
The project at `{{ project_root }}` is already set up (copied from a working template).
- `MainFile.cs` — entry point (read it to confirm the exact namespace and ModId)
- `local.props` — already correct for this machine (do NOT recreate)
- `nuget.config` — already correct (do NOT recreate)
- `Extensions/StringExtensions.cs` — image path helpers, already present
- `{{ mod_name }}/` — Godot resource dir (named after the MOD, NOT the asset). Images and localization go here.

IMPORTANT: The Godot resource directory is `{{ mod_name }}/`, not `{{ asset_name }}/`.
All image paths and res:// references must use `{{ mod_name }}` as the root.

DO NOT re-clone from GitHub. DO NOT recreate local.props or nuget.config.
Read MainFile.cs first to confirm the exact namespace and ModId.

Steps to complete:
1. Read `MainFile.cs` to confirm the namespace and ModId. Read `{{ asset_name }}.csproj` to understand project structure.
2. If you are unsure of an exact API signature, method name, or base class, read `{{ api_ref_path }}` before writing code.
3. Create the C# class file for this {{ asset_type }} following BaseLib conventions (see reference above).
   CRITICAL rules for cards:
   - Cards MUST have [Pool(typeof(SomeCardPool))] attribute (e.g. ColorlessCardPool) — without it the game crashes on startup.
   - Do NOT create a Harmony patch to manually add cards to pools — BaseLib autoAdd handles this.
4. Create BOTH localization files:
   - `{{ mod_name }}/localization/eng/<type>s.json` — English
   - `{{ mod_name }}/localization/zhs/<type>s.json` — Simplified Chinese
5. Register it in MainFile.cs if needed (BaseLib handles most registration automatically).
{{ build_step }}

Follow the existing code style in the project."""

_CUSTOM_CODE_PROMPT_TEMPLATE = """You are an expert STS2 (Slay the Spire 2) mod developer using Godot 4 + C# + BaseLib (Alchyr.Sts2.BaseLib).

{{ docs }}

---

{{ api_lookup }}

---

Task: Implement a custom code component named "{{ name }}".

Design description:
{{ description }}

Technical implementation notes:
{{ implementation_notes }}

## Project already initialized
The project at `{{ project_root }}` is already set up (copied from a working template).
- `MainFile.cs` — entry point with `harmony.PatchAll()` already wired up
- `local.props` and `nuget.config` — already correct, do NOT recreate
- `{{ mod_name }}/` — Godot resource dir (named after the MOD: "{{ mod_name }}")

DO NOT re-clone from GitHub. DO NOT recreate local.props or nuget.config.

Steps to complete:
1. Read `MainFile.cs` to confirm the namespace and ModId.
2. If you are unsure of an exact API signature, read `{{ api_ref_path }}` before writing code.
3. Create the C# implementation file(s) following BaseLib/Harmony conventions.
4. `MainFile.cs` already calls `harmony.PatchAll()` — Harmony patches are auto-discovered, no manual registration needed.
{{ build_steps }}

Do not create any image assets."""

_ASSET_GROUP_PROMPT_TEMPLATE = """You are an expert STS2 (Slay the Spire 2) mod developer using Godot 4 + C# + BaseLib (Alchyr.Sts2.BaseLib).

{{ combined_docs }}

---

{{ api_lookup }}

---

## Task: Create {{ asset_count }} related assets in ONE batch

These assets are grouped because they depend on each other. Generate ALL of them in this single session.
Class names in this group: {{ class_names }}

{{ assets_section }}

## Project already initialized
The project at `{{ project_root }}` is set up. Read `MainFile.cs` first to confirm namespace and ModId.
- `local.props` and `nuget.config` — already correct, do NOT recreate
- `Extensions/StringExtensions.cs` — image path helpers, already present
- `{{ mod_name }}/` — Godot resource dir (named after the MOD: "{{ mod_name }}"). All images and localization go here.

IMPORTANT: The Godot resource directory is `{{ mod_name }}/`, NOT individual asset names.
All res:// paths must use `{{ mod_name }}` as root.

DO NOT re-clone from GitHub. DO NOT recreate local.props or nuget.config.

## Steps

1. Read `MainFile.cs` to confirm the exact namespace and ModId.
2. For each asset in the group (in dependency order — dependencies first):
   a. Create the C# class file following BaseLib conventions.
   b. Create localization files (eng + zhs) under `<ModDir>/localization/`.
   When an asset references another asset in this group, use its exact class name from the list above.
3. After ALL assets are written, run `dotnet publish` ONCE (not once per asset).
4. Fix any compilation errors and re-run until it succeeds.
5. Confirm both .dll and .pck deployed to the mods folder.

Write all assets before running dotnet publish — do not build after each individual asset."""

_BUILD_PROMPT_TEMPLATE = """Run `dotnet publish` in this STS2 mod project (this builds the DLL and exports the Godot .pck file).
If there are compilation errors, fix them and re-run dotnet publish.
Repeat until it succeeds or you've tried {{ max_attempts }} times.
Report the final status clearly."""

_CREATE_MOD_PROJECT_PROMPT_TEMPLATE = """Create a new STS2 mod project named "{{ project_name }}" at {{ project_path }}.

Steps:
1. Clone the ModTemplate from https://github.com/Alchyr/ModTemplate-StS2 into {{ project_path }}
2. Rename the project: update .csproj file, ModEntry.cs class name, and any other references to the template name.
3. Check that `dotnet build` works (may fail without local.props, that's OK — just note it).
4. Report what was created and what the user needs to configure next (local.props paths)."""

_PACKAGE_PROMPT_TEMPLATE = """Build and package this STS2 mod completely:
1. Run `dotnet build` with Release configuration.
2. Verify the .dll and .pck output files exist in the expected output directory.
3. Report the output file paths."""


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
            fallback_template=_ASSET_PROMPT_TEMPLATE,
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
            fallback_template=_CUSTOM_CODE_PROMPT_TEMPLATE,
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
            fallback_template=_ASSET_GROUP_PROMPT_TEMPLATE,
        )

    def assemble_build_prompt(self, max_attempts: int) -> str:
        return self.prompt_loader.render(
            _BUILD_PROMPT_KEY,
            {"max_attempts": max_attempts},
            fallback_template=_BUILD_PROMPT_TEMPLATE,
        )

    def assemble_create_mod_project_prompt(self, request: ModProjectRequest) -> str:
        project_path = request.target_dir / request.project_name
        return self.prompt_loader.render(
            _CREATE_MOD_PROJECT_PROMPT_KEY,
            {
                "project_name": request.project_name,
                "project_path": project_path,
            },
            fallback_template=_CREATE_MOD_PROJECT_PROMPT_TEMPLATE,
        )

    def assemble_package_prompt(self) -> str:
        return self.prompt_loader.render(
            _PACKAGE_PROMPT_KEY,
            {},
            fallback_template=_PACKAGE_PROMPT_TEMPLATE,
        )
