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
