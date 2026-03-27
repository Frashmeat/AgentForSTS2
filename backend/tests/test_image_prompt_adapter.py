import importlib
import asyncio
import sys
import types
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent))

PROMPT_TEMPLATE_PATH = Path(__file__).parent.parent / "app" / "modules" / "image" / "resources" / "prompts" / "adapt_prompt.txt"
PROMPT_RESOURCE_DIR = PROMPT_TEMPLATE_PATH.parent
TRANSPARENT_BG_RULE_PATH = PROMPT_RESOURCE_DIR / "transparent_bg_rule.txt"
FALLBACK_EN_SUFFIX_PATH = PROMPT_RESOURCE_DIR / "fallback_prompt_en_suffix.txt"
FALLBACK_EN_TRANSPARENT_SUFFIX_PATH = PROMPT_RESOURCE_DIR / "fallback_prompt_en_transparent_suffix.txt"
FALLBACK_CN_SUFFIX_PATH = PROMPT_RESOURCE_DIR / "fallback_prompt_cn_suffix.txt"
FALLBACK_CN_TRANSPARENT_SUFFIX_PATH = PROMPT_RESOURCE_DIR / "fallback_prompt_cn_transparent_suffix.txt"
SDXL_NEGATIVE_PATH = PROMPT_RESOURCE_DIR / "fallback_sdxl_negative_prompt.txt"
GUIDE_RESOURCE_PATHS = [
    PROMPT_RESOURCE_DIR / "guide_flux2_formula.txt",
    PROMPT_RESOURCE_DIR / "guide_flux2_rules.txt",
    PROMPT_RESOURCE_DIR / "guide_flux2_example.txt",
    PROMPT_RESOURCE_DIR / "guide_sdxl_formula.txt",
    PROMPT_RESOURCE_DIR / "guide_sdxl_rules.txt",
    PROMPT_RESOURCE_DIR / "guide_sdxl_example.txt",
    PROMPT_RESOURCE_DIR / "guide_sdxl_negative_example.txt",
    PROMPT_RESOURCE_DIR / "guide_jimeng_formula.txt",
    PROMPT_RESOURCE_DIR / "guide_jimeng_rules.txt",
    PROMPT_RESOURCE_DIR / "guide_jimeng_example.txt",
    PROMPT_RESOURCE_DIR / "guide_wanxiang_formula.txt",
    PROMPT_RESOURCE_DIR / "guide_wanxiang_rules.txt",
    PROMPT_RESOURCE_DIR / "guide_wanxiang_example.txt",
]


def _load_prompt_adapter():
    sys.modules.setdefault("litellm", types.SimpleNamespace(acompletion=None))
    sys.modules.setdefault("config", types.SimpleNamespace(get_config=lambda: {"llm": {}}))
    sys.modules.setdefault(
        "llm.text_runner",
        types.SimpleNamespace(
            complete_text=lambda *args, **kwargs: None,
            resolve_model=lambda cfg: "fake-model",
        ),
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
    assert PROMPT_TEMPLATE_PATH.exists()
    content = PROMPT_TEMPLATE_PATH.read_text(encoding="utf-8")
    assert "{{ asset_type }}" in content
    assert "{{ provider }}" in content
    assert "{{ rules_text }}" in content
    assert "{{ user_description }}" in content


def test_image_prompt_resource_fragments_exist():
    paths = [
        TRANSPARENT_BG_RULE_PATH,
        FALLBACK_EN_SUFFIX_PATH,
        FALLBACK_EN_TRANSPARENT_SUFFIX_PATH,
        FALLBACK_CN_SUFFIX_PATH,
        FALLBACK_CN_TRANSPARENT_SUFFIX_PATH,
        SDXL_NEGATIVE_PATH,
    ]

    for path in paths:
        assert path.exists(), f"missing resource: {path.name}"


def test_style_guide_resource_fragments_exist():
    for path in GUIDE_RESOURCE_PATHS:
        assert path.exists(), f"missing guide resource: {path.name}"


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


@pytest.mark.parametrize(
    ("provider", "rules_resource", "negative_example_resource"),
    [
        ("flux2", "guide_flux2_rules.txt", None),
        ("sdxl", "guide_sdxl_rules.txt", "guide_sdxl_negative_example.txt"),
        ("jimeng", "guide_jimeng_rules.txt", None),
        ("wanxiang", "guide_wanxiang_rules.txt", None),
    ],
)
def test_adapt_prompt_includes_provider_guide_resources(monkeypatch, provider, rules_resource, negative_example_resource):
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

    formula = (PROMPT_RESOURCE_DIR / f"guide_{provider}_formula.txt").read_text(encoding="utf-8").strip()
    example = (PROMPT_RESOURCE_DIR / f"guide_{provider}_example.txt").read_text(encoding="utf-8").strip()
    rules = [
        line.strip()[2:]
        for line in (PROMPT_RESOURCE_DIR / rules_resource).read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]

    assert f"Formula: {formula}" in captured["prompt"]
    assert f"Example output: {example}" in captured["prompt"]
    for rule in rules:
        assert f"- {rule}" in captured["prompt"]

    if negative_example_resource:
        negative_example = (PROMPT_RESOURCE_DIR / negative_example_resource).read_text(encoding="utf-8").strip()
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

    assert "The asset requires a transparent/white background (no background scene)" in captured["prompt"]


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
