import sys
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.modules.codegen import api as codegen_api


def test_api_lookup_section_uses_configured_decompiled_source(monkeypatch):
    decompiled_dir = "I:/fake/sts2-decompiled"
    monkeypatch.setattr(
        codegen_api,
        "build_api_lookup_context",
        lambda: {
            "baselib_src_path": "I:/runtime/knowledge/baselib/BaseLib.decompiled.cs",
            "game_source_mode": "runtime_decompiled",
            "game_path": decompiled_dir,
            "ilspy_example_dll_path": "<STS2GamePath>/data_sts2_windows_x86_64/sts2.dll",
        },
    )

    section = codegen_api._build_api_lookup_section()

    assert "## API Lookup" in section
    assert "Do NOT curl GitHub for BaseLib" in section
    assert f"Full decompiled sts2.dll source: `{decompiled_dir}`" in section
    assert "Key subdirs:" in section
    assert "Only fall back to ilspycmd if a specific class is missing from this directory." in section
    assert "Use `ilspycmd <path_to_sts2.dll>`" not in section


def test_api_lookup_section_falls_back_to_ilspy_when_no_decompiled_source(monkeypatch):
    monkeypatch.setattr(
        codegen_api,
        "build_api_lookup_context",
        lambda: {
            "baselib_src_path": "I:/runtime/knowledge/baselib/BaseLib.decompiled.cs",
            "game_source_mode": "missing",
            "game_path": "",
            "ilspy_example_dll_path": "<STS2GamePath>/data_sts2_windows_x86_64/sts2.dll",
        },
    )

    section = codegen_api._build_api_lookup_section()

    assert "BaseLib (Alchyr.Sts2.BaseLib) decompiled source:" in section
    assert "sts2.dll decompiled source is NOT available on this machine." in section
    assert "Use `ilspycmd <path_to_sts2.dll>` to look up specific classes when needed." in section
    assert "Game DLL is typically at:" in section


def test_api_lookup_section_reads_from_resource_templates(monkeypatch):
    monkeypatch.setattr(
        codegen_api,
        "build_api_lookup_context",
        lambda: {
            "baselib_src_path": "I:/runtime/knowledge/baselib/BaseLib.decompiled.cs",
            "game_source_mode": "runtime_decompiled",
            "game_path": "I:/fake/sts2-decompiled",
            "ilspy_example_dll_path": "<STS2GamePath>/data_sts2_windows_x86_64/sts2.dll",
        },
    )

    temp_root = Path(__file__).parent / ".tmp" / f"api-lookup-{uuid.uuid4().hex}"
    resource_root = temp_root / "prompts"
    resource_root.mkdir(parents=True)
    try:
        (resource_root / "codegen.md").write_text(
            """## api_lookup_title
Resource Title

## api_lookup_baselib
BaseLib from {{ baselib_src_path }}

## api_lookup_sts2_local
Local src {{ decompiled_src_path }}

## api_lookup_sts2_fallback
Fallback {{ ilspy_example_dll_path }}
""",
            encoding="utf-8",
        )
        monkeypatch.setattr(codegen_api, "_PROMPT_LOADER", codegen_api.PromptLoader(root=resource_root))

        section = codegen_api._build_api_lookup_section()

        assert section == "Resource Title\nBaseLib from `I:/runtime/knowledge/baselib/BaseLib.decompiled.cs`\n\nLocal src `I:/fake/sts2-decompiled`"
    finally:
        for path in sorted(temp_root.rglob("*"), reverse=True):
            if path.is_file():
                path.unlink()
            elif path.is_dir():
                path.rmdir()


def test_api_lookup_section_prefers_runtime_baselib_source(monkeypatch):
    monkeypatch.setattr(
        codegen_api,
        "build_api_lookup_context",
        lambda: {
            "baselib_src_path": "I:/runtime/knowledge/baselib_decompiled/BaseLib.decompiled.cs",
            "game_source_mode": "runtime_decompiled",
            "game_path": "I:/fake/sts2-decompiled",
            "ilspy_example_dll_path": "<STS2GamePath>/data_sts2_windows_x86_64/sts2.dll",
        },
    )

    section = codegen_api._build_api_lookup_section()

    assert "I:/runtime/knowledge/baselib_decompiled/BaseLib.decompiled.cs" in section
