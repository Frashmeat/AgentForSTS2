"""Tests for shared prompt loading utilities."""
import importlib.util
import shutil
import sys
import uuid
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

_PROMPT_LOADER_SPEC = importlib.util.find_spec("app.shared.prompting.prompt_loader")

pytestmark = pytest.mark.skipif(
    _PROMPT_LOADER_SPEC is None,
    reason="PromptLoader implementation has not landed yet; enable these tests once the migration is wired in.",
)

if _PROMPT_LOADER_SPEC is not None:
    from app.shared.prompting.prompt_loader import PromptLoader, PromptNotFoundError


def _make_temp_root() -> Path:
    root = Path(__file__).parent / ".tmp" / f"prompt-loader-{uuid.uuid4().hex}"
    root.mkdir(parents=True)
    return root


def _write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def test_prompt_loader_reads_utf8_template():
    root = _make_temp_root() / "prompts"
    root.mkdir()
    template = root / "greeting.txt"
    template.write_text("你好，{{ name }}！", encoding="utf-8")

    try:
        loader = PromptLoader(root)

        assert loader.load("greeting.txt") == "你好，{{ name }}！"
    finally:
        shutil.rmtree(root.parent, ignore_errors=True)


def test_prompt_loader_strips_utf8_bom():
    root = _make_temp_root() / "prompts"
    root.mkdir()
    template = root / "bom.txt"
    template.write_text("Header", encoding="utf-8-sig")

    try:
        loader = PromptLoader(root)

        assert loader.load("bom.txt") == "Header"
    finally:
        shutil.rmtree(root.parent, ignore_errors=True)


def test_prompt_loader_raises_for_missing_file():
    root = _make_temp_root()
    loader = PromptLoader(root)

    try:
        with pytest.raises(PromptNotFoundError, match="missing.txt"):
            loader.load("missing.txt")
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_prompt_loader_renders_template_variables():
    root = _make_temp_root() / "prompts"
    root.mkdir()
    template = root / "summary.txt"
    template.write_text("Hello {{ name }}, tasks: {{ count }}.", encoding="utf-8")

    try:
        loader = PromptLoader(root)

        assert loader.render("summary.txt", {"name": "Alice", "count": 3}) == "Hello Alice, tasks: 3."
    finally:
        shutil.rmtree(root.parent, ignore_errors=True)


def test_prompt_loader_renders_nested_template_paths():
    root = _make_temp_root() / "prompts"
    nested = root / "codegen"
    nested.mkdir(parents=True)
    template = nested / "asset.txt"
    template.write_text("Asset {{ asset_name }} uses {{ asset_type }}.", encoding="utf-8")

    try:
        loader = PromptLoader(root)

        rendered = loader.render(
            "codegen/asset.txt",
            {"asset_name": "BurnRelic", "asset_type": "relic"},
        )

        assert rendered == "Asset BurnRelic uses relic."
    finally:
        shutil.rmtree(root.parent, ignore_errors=True)


def test_prompt_loader_uses_fallback_when_template_missing():
    root = _make_temp_root()
    loader = PromptLoader(root)

    try:
        rendered = loader.render(
            "missing.txt",
            {"name": "fallback"},
            fallback_template="Default {{ name }}",
        )

        assert rendered == "Default fallback"
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_prompt_loader_keeps_full_bundle_file_reads():
    root = _make_temp_root()
    bundle = root / "planning.md"
    bundle_text = """# planning prompts

## planner_prompt
Hello {{ name }}
## Keep this heading

"""
    _write_text(bundle, bundle_text)

    try:
        loader = PromptLoader(root)

        assert loader.load("planning.md") == bundle_text
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_prompt_loader_loads_bundle_key_from_markdown_bundle():
    root = _make_temp_root()
    bundle = root / "planning.md"
    bundle_text = """# planning prompts

## planner_prompt
Hello {{ name }}
### Keep this heading

"""
    _write_text(bundle, bundle_text)

    try:
        loader = PromptLoader(root)

        assert loader.load("planning.planner_prompt") == "Hello {{ name }}\n### Keep this heading\n\n"
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_prompt_loader_renders_bundle_key_from_markdown_bundle():
    root = _make_temp_root()
    bundle = root / "planning.md"
    bundle_text = """# planning prompts

## planner_prompt
Hello {{ name }}
### Keep this heading

"""
    _write_text(bundle, bundle_text)

    try:
        loader = PromptLoader(root)

        assert loader.render("planning.planner_prompt", {"name": "Alice"}) == "Hello Alice\n### Keep this heading\n\n"
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_prompt_loader_rejects_legacy_txt_alias_requests():
    root = _make_temp_root()
    bundle = root / "planning.md"
    bundle_text = """## planner_prompt
Plan for {{ name }}
"""
    _write_text(bundle, bundle_text)

    try:
        loader = PromptLoader(root)

        with pytest.raises(PromptNotFoundError, match="legacy_prompt.txt"):
            loader.render("legacy_prompt.txt", {"name": "Silent"})
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_prompt_loader_raises_for_missing_bundle_key():
    root = _make_temp_root()
    _write_text(
        root / "planning.md",
        """## planner_prompt
Plan
""",
    )

    try:
        loader = PromptLoader(root)

        with pytest.raises(PromptNotFoundError, match="planning.missing_key"):
            loader.load("planning.missing_key")
    finally:
        shutil.rmtree(root, ignore_errors=True)


@pytest.mark.parametrize(
    ("bundle_text", "message"),
    [
        (
            """## bad-key
Broken
""",
            "Invalid prompt bundle key",
        ),
        (
            """## planner_prompt
First
## planner_prompt
Second
""",
            "Duplicate prompt bundle key",
        ),
        (
            """## planner_prompt

""",
            "Empty prompt bundle section",
        ),
    ],
)
def test_prompt_loader_rejects_invalid_bundle_markup(bundle_text: str, message: str):
    root = _make_temp_root()
    _write_text(root / "planning.md", bundle_text)

    try:
        loader = PromptLoader(root)

        with pytest.raises(ValueError, match=message):
            loader.load("planning.planner_prompt")
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_prompt_loader_ignores_heading_like_text_inside_code_blocks():
    root = _make_temp_root()
    _write_text(
        root / "planning.md",
        """## planner_prompt
```md
## not_a_key
```

Actual content
## second_prompt
Second
""",
    )

    try:
        loader = PromptLoader(root)

        assert loader.load("planning.planner_prompt") == "```md\n## not_a_key\n```\n\nActual content\n"
        assert loader.load("planning.second_prompt") == "Second\n"
    finally:
        shutil.rmtree(root, ignore_errors=True)
