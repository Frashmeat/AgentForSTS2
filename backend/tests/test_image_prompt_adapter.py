import importlib
import asyncio
import sys
import types
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.prompting import PromptLoader

PROMPT_LOADER = PromptLoader()


def _load_prompt_adapter():
    sys.modules.setdefault("litellm", types.SimpleNamespace(acompletion=None))
    sys.modules.setdefault("config", types.SimpleNamespace(get_config=lambda: {"llm": {}}))
    sys.modules["llm.text_runner"] = types.SimpleNamespace(
        complete_text=lambda *args, **kwargs: None,
        resolve_model=lambda cfg: "fake-model",
    )
    sys.modules.pop("image.prompt_adapter", None)
    return importlib.import_module("image.prompt_adapter")


def test_sanitize_for_content_policy_replaces_sensitive_terms():
    module = _load_prompt_adapter()

    sanitized = module._sanitize_for_content_policy("A bomb, pistol, and rifle trigger an explosion blast.")

    assert "bomb" not in sanitized.lower()
    assert "pistol" not in sanitized.lower()
    assert "rifle" not in sanitized.lower()
    assert "explosion" not in sanitized.lower()
    assert "orb" in sanitized
    assert "rod" in sanitized
    assert "staff" in sanitized
    assert "burst" in sanitized


def test_adapt_prompt_template_exists():
    content = PROMPT_LOADER.load("image.adapt_prompt")
    assert "{{ asset_type }}" in content
    assert "{{ provider }}" in content
    assert "{{ rules_text }}" in content
    assert "{{ user_description }}" in content


def test_image_prompt_resource_fragments_exist():
    keys = [
        "image.transparent_bg_rule",
        "image.fallback_prompt_en_suffix",
        "image.fallback_prompt_en_transparent_suffix",
        "image.fallback_prompt_cn_suffix",
        "image.fallback_prompt_cn_transparent_suffix",
        "image.fallback_sdxl_negative_prompt",
    ]

    for key in keys:
        assert PROMPT_LOADER.load(key).strip(), f"missing resource: {key}"


def test_style_guide_resource_fragments_exist():
    keys = [
        "image.guide_flux2_formula",
        "image.guide_flux2_rules",
        "image.guide_flux2_example",
        "image.guide_sdxl_formula",
        "image.guide_sdxl_rules",
        "image.guide_sdxl_example",
        "image.guide_sdxl_negative_example",
        "image.guide_jimeng_formula",
        "image.guide_jimeng_rules",
        "image.guide_jimeng_example",
        "image.guide_wanxiang_formula",
        "image.guide_wanxiang_rules",
        "image.guide_wanxiang_example",
    ]
    for key in keys:
        assert PROMPT_LOADER.load(key).strip(), f"missing guide resource: {key}"


def test_adapt_prompt_renders_template_variables(monkeypatch):
    module = _load_prompt_adapter()
    captured = {}

    async def fake_complete_text(prompt, llm_cfg):
        captured["prompt"] = prompt
        captured["llm_cfg"] = llm_cfg
        return '{"prompt":"ok","negative_prompt":"none"}'

    monkeypatch.setattr(module, "complete_text", fake_complete_text)

    result = asyncio.run(
        module.adapt_prompt(
            user_description="Deal 8 damage with a glowing dagger relic",
            asset_type="relic",
            provider="flux2",
            needs_transparent_bg=True,
        )
    )

    assert result == {"prompt": "ok", "negative_prompt": "none"}
    assert "Asset type: relic" in captured["prompt"]
    assert "Target model family: flux2 (English prompt style)" in captured["prompt"]
    assert "Formula: Subject (most important first) + Action/Detail + Style + Camera + Lighting" in captured["prompt"]
    assert "User design description:\nDeal 8 damage with a glowing dagger relic" in captured["prompt"]
    assert "isolated on pure white background" in captured["prompt"]


def test_image_prompt_adapter_uses_bundle_keys_for_prompt_resources(monkeypatch):
    module = _load_prompt_adapter()

    class FakePromptLoader:
        def __init__(self) -> None:
            self.load_calls: list[tuple[str, str]] = []
            self.render_calls: list[tuple[str, dict[str, object], str]] = []

        def load(self, template_name: str, *, fallback_template: str | None = None) -> str:
            self.load_calls.append((template_name, fallback_template or ""))
            return fallback_template or template_name

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.render_calls.append((template_name, variables, fallback_template or ""))
            return '{"prompt":"ok"}'

    async def fake_complete_text(prompt, llm_cfg):
        return prompt

    loader = FakePromptLoader()
    monkeypatch.setattr(module, "_PROMPT_LOADER", loader)
    monkeypatch.setattr(module, "complete_text", fake_complete_text)

    asyncio.run(
        module.adapt_prompt(
            user_description="A floating blessing icon",
            asset_type="icon",
            provider="jimeng",
            needs_transparent_bg=True,
        )
    )

    assert loader.render_calls[0][0] == "image.adapt_prompt"
    assert ("image.transparent_bg_rule", "") in loader.load_calls


@pytest.mark.parametrize(
    ("provider", "rules_key", "negative_example_key"),
    [
        ("flux2", "image.guide_flux2_rules", None),
        ("sdxl", "image.guide_sdxl_rules", "image.guide_sdxl_negative_example"),
        ("jimeng", "image.guide_jimeng_rules", None),
        ("wanxiang", "image.guide_wanxiang_rules", None),
    ],
)
def test_adapt_prompt_includes_provider_guide_resources(monkeypatch, provider, rules_key, negative_example_key):
    module = _load_prompt_adapter()
    captured = {}

    async def fake_complete_text(prompt, llm_cfg):
        captured["prompt"] = prompt
        return '{"prompt":"ok"}'

    monkeypatch.setattr(module, "complete_text", fake_complete_text)

    asyncio.run(
        module.adapt_prompt(
            user_description="A glowing relic with etched details",
            asset_type="relic",
            provider=provider,
            needs_transparent_bg=False,
        )
    )

    formula = PROMPT_LOADER.load(f"image.guide_{provider}_formula").strip()
    example = PROMPT_LOADER.load(f"image.guide_{provider}_example").strip()
    rules = [
        line.strip()[2:]
        for line in PROMPT_LOADER.load(rules_key).splitlines()
        if line.strip()
    ]

    assert f"Formula: {formula}" in captured["prompt"]
    assert f"Example output: {example}" in captured["prompt"]
    for rule in rules:
        assert f"- {rule}" in captured["prompt"]

    if negative_example_key:
        negative_example = PROMPT_LOADER.load(negative_example_key).strip()
        assert negative_example in captured["prompt"]


def test_adapt_prompt_falls_back_and_sanitizes_when_llm_fails(monkeypatch):
    module = _load_prompt_adapter()

    async def raise_timeout(*args, **kwargs):
        raise RuntimeError("llm unavailable")

    monkeypatch.setattr(module, "complete_text", raise_timeout)

    result = asyncio.run(
        module.adapt_prompt(
            user_description="Deal 8 damage with a bomb relic",
            asset_type="relic",
            provider="sdxl",
            needs_transparent_bg=True,
        )
    )

    assert result["negative_prompt"] == "blurry, low quality, text, watermark, signature, deformed"
    assert "fallback_warning" in result
    assert "damage" not in result["prompt"].lower()
    assert "bomb" not in result["prompt"].lower()
    assert "orb" in result["prompt"].lower()
    assert "white background" in result["prompt"].lower()


def test_adapt_prompt_adds_resource_backed_transparent_rule(monkeypatch):
    module = _load_prompt_adapter()
    captured = {}

    async def fake_complete_text(prompt, llm_cfg):
        captured["prompt"] = prompt
        return '{"prompt":"ok"}'

    monkeypatch.setattr(module, "complete_text", fake_complete_text)

    asyncio.run(
        module.adapt_prompt(
            user_description="A floating blessing icon",
            asset_type="icon",
            provider="jimeng",
            needs_transparent_bg=True,
        )
    )

    assert PROMPT_LOADER.load("image.transparent_bg_rule").strip() in captured["prompt"]


def test_fallback_prompt_keeps_resource_backed_suffixes_and_negative_prompt():
    module = _load_prompt_adapter()

    english = module._fallback_prompt("Deal 8 damage with a glowing dagger relic", "sdxl", True)
    chinese = module._fallback_prompt("造成8点伤害，发光的匕首遗物", "jimeng", True)

    assert english == {
        "prompt": "a glowing dagger relic, trading card game art style, dramatic cinematic lighting, highly detailed, sharp focus, isolated on pure white background, no shadow, no background",
        "negative_prompt": "blurry, low quality, text, watermark, signature, deformed",
    }
    assert chinese == {
        "prompt": "发光的匕首遗物，交易卡牌艺术风格，电影级光照，高清细节，白色纯净背景",
        "negative_prompt": None,
    }
