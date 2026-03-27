"""
Prompt Adapter：将用户的卡牌/遗物设计描述翻译为各图生模型专用 prompt。
通过 LLM 调用实现，不硬编码翻译规则。
"""
from __future__ import annotations

from pathlib import Path
from typing import Literal

import litellm

from app.modules.image.application.ports import PromptOptimizer
from app.shared.prompting import PromptLoader
from config import get_config
from llm.text_runner import complete_text, resolve_model

ImageProvider = Literal["flux2", "sdxl", "jimeng", "wanxiang"]

_PROMPT_ROOT = Path(__file__).resolve().parent.parent / "app" / "modules" / "image" / "resources" / "prompts"
_PROMPT_LOADER = PromptLoader(root=_PROMPT_ROOT)
_TRANSPARENT_BG_RULE = "The asset requires a transparent/white background (no background scene)"
_FALLBACK_PROMPT_EN_SUFFIX = ", trading card game art style, dramatic cinematic lighting, highly detailed, sharp focus"
_FALLBACK_PROMPT_EN_TRANSPARENT_SUFFIX = ", isolated on pure white background, no shadow, no background"
_FALLBACK_PROMPT_CN_SUFFIX = "，交易卡牌艺术风格，电影级光照，高清细节"
_FALLBACK_PROMPT_CN_TRANSPARENT_SUFFIX = "，白色纯净背景"
_FALLBACK_SDXL_NEGATIVE_PROMPT = "blurry, low quality, text, watermark, signature, deformed"
_ADAPT_PROMPT_TEMPLATE = """You are an expert at writing image generation prompts for trading card game assets. Extract ONLY the visual/artistic elements from the user's description and convert them into an optimized image prompt. IMPORTANT: Ignore all game mechanics — damage values, costs, card effects, upgrade conditions, numbers, keywords like 'deal X damage', 'gain Y block', etc. Focus solely on: appearance, materials, colors, style, lighting, mood, and composition. Return ONLY a JSON object with keys: 'prompt' and optionally 'negative_prompt'. No explanation, no markdown, just the JSON.

Asset type: {{ asset_type }}
Target model family: {{ provider }} ({{ guide_lang }} prompt style)
Formula: {{ guide_formula }}
Rules:
{{ rules_text }}
Example output: {{ guide_example }}
{{ guide_negative_example_block }}

User design description:
{{ user_description }}

Generate the optimized image prompt now (visual elements only, no game mechanics).
"""

_STYLE_GUIDE_DEFAULTS: dict[ImageProvider, dict[str, object]] = {
    "flux2": {
        "lang": "English",
        "formula": "Subject (most important first) + Action/Detail + Style + Camera + Lighting",
        "rules": [
            "Natural language sentences, NOT tag lists",
            "Put the main subject at the very beginning",
            "Include specific materials, HEX colors where helpful (e.g. #9B59B6 purple)",
            "Add camera/lens info: e.g. 'shot on Canon 85mm f/2.8, sharp focus'",
            "Cinematic lighting description",
            "Trading card game art style",
            "NO negative prompts",
            "For transparent-bg assets (relics, powers, icons): add 'isolated on pure white background, no shadow, no background'",
        ],
        "example": "A dark obsidian dagger with glowing #9B59B6 purple edge, dramatic rim lighting, intricate engravings catching golden highlights, trading card art style, shot on Canon 85mm f/2.8, sharp focus, cinematic atmosphere, isolated on pure white background",
    },
    "sdxl": {
        "lang": "English",
        "formula": "comma-separated tags + quality words + (emphasis:weight)",
        "rules": [
            "Use tag-based format, comma separated",
            "Add quality tags: masterpiece, best quality, highly detailed, sharp focus",
            "Use (tag:1.2) syntax for emphasis",
            "Add negative prompt separately",
            "For transparent-bg assets: add 'white background, simple background'",
        ],
        "example": "dark obsidian dagger, glowing purple edge, intricate engravings, golden highlights, (masterpiece:1.2), best quality, highly detailed, trading card art, dramatic lighting, (sharp focus:1.3)",
        "negative_example": "blurry, low quality, text, watermark, signature",
    },
    "jimeng": {
        "lang": "Chinese",
        "formula": "主体 + 外观描述 + 细节 + 风格 + 质量词",
        "rules": [
            "中文自然语言，使用短句，句子之间用逗号分隔",
            "突出主体，外观描述具体清晰",
            "避免古诗词或过于文学化的表达",
            "加入风格词和质量词",
            "透明背景类资产加：白色背景，简洁背景",
        ],
        "example": "暗黑黑曜石匕首，紫色边缘发光，精致雕纹，戏剧性光影，交易卡牌艺术风格，电影级照明，4K 高清细节，白色背景",
    },
    "wanxiang": {
        "lang": "Chinese",
        "formula": "主体描述 + 场景描述 + 风格 + 镜头语言（特写/俯视等） + 氛围词 + 细节",
        "rules": [
            "中文，风格词可以放前面",
            "使用镜头语言词：特写、全景、俯视、仰视等",
            "加入氛围词：戏剧性、神秘、史诗感等",
            "系统会自动开启 prompt_extend 优化",
            "透明背景类资产加：白色纯净背景",
        ],
        "example": "暗黑黑曜石匕首，紫色边缘发光，特写镜头，戏剧性逆光，交易卡牌艺术风格，电影级光照，精致细节，4K，白色纯净背景",
    },
}


# 内容审核敏感词替换表（针对即梦/火山引擎等国内 API 的限制）
_CONTENT_SAFE_REPLACEMENTS: list[tuple[str, str]] = [
    # 爆炸物相关
    (r'\bbomb\b', 'orb'),
    (r'\bexplosive\b', 'charged'),
    (r'\bexplosion\b', 'burst'),
    (r'\bdetonate\b', 'release'),
    (r'\bblast\b', 'surge'),
    # 武器相关（部分 API 限制）
    (r'\bgun\b', 'wand'),
    (r'\bpistol\b', 'rod'),
    (r'\brifle\b', 'staff'),
]


def _load_prompt_resource(name: str, fallback_text: str) -> str:
    text = _PROMPT_LOADER.load(name, fallback_template=fallback_text)
    return text.strip()


def _load_prompt_resource_lines(name: str, fallback_lines: list[str]) -> list[str]:
    text = _load_prompt_resource(name, "\n".join(f"- {line}" for line in fallback_lines))
    lines = []
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line:
            continue
        if line.startswith("- "):
            line = line[2:].strip()
        lines.append(line)
    return lines


def _build_style_guides() -> dict[ImageProvider, dict]:
    guides: dict[ImageProvider, dict] = {}
    for provider, defaults in _STYLE_GUIDE_DEFAULTS.items():
        guide = {
            "lang": defaults["lang"],
            "formula": _load_prompt_resource(
                f"guide_{provider}_formula.txt",
                str(defaults["formula"]),
            ),
            "rules": _load_prompt_resource_lines(
                f"guide_{provider}_rules.txt",
                list(defaults["rules"]),
            ),
            "example": _load_prompt_resource(
                f"guide_{provider}_example.txt",
                str(defaults["example"]),
            ),
        }
        negative_example = defaults.get("negative_example")
        if negative_example is not None:
            guide["negative_example"] = _load_prompt_resource(
                f"guide_{provider}_negative_example.txt",
                str(negative_example),
            )
        guides[provider] = guide
    return guides


# 各模型的 prompt 风格说明，注入给 LLM 用于生成
STYLE_GUIDES: dict[ImageProvider, dict] = _build_style_guides()


def _sanitize_for_content_policy(text: str) -> str:
    """替换可能触发内容审核的词汇（主要针对国内图生 API）。"""
    import re
    result = text
    for pattern, replacement in _CONTENT_SAFE_REPLACEMENTS:
        result = re.sub(pattern, replacement, result, flags=re.IGNORECASE)
    return result


async def adapt_prompt(
    user_description: str,
    asset_type: str,
    provider: ImageProvider,
    needs_transparent_bg: bool,
) -> dict:
    """
    调用 LLM 将用户描述翻译为目标模型的 prompt。
    返回 {"prompt": str, "negative_prompt": str | None}
    """
    cfg = get_config()
    llm_cfg = cfg["llm"]
    guide = STYLE_GUIDES[provider]
    negative_example_block = ""
    if guide.get("negative_example"):
        negative_example_block = f"Example negative prompt: {guide['negative_example']}\n"

    rules_text = "\n".join(f"- {r}" for r in guide["rules"])
    if needs_transparent_bg and "transparent" not in " ".join(guide["rules"]).lower():
        rules_text += "\n- " + _load_prompt_resource("transparent_bg_rule.txt", _TRANSPARENT_BG_RULE)

    full_prompt = _PROMPT_LOADER.render(
        "adapt_prompt.txt",
        {
            "asset_type": asset_type,
            "provider": provider,
            "guide_example": guide["example"],
            "guide_formula": guide["formula"],
            "guide_lang": guide["lang"],
            "guide_negative_example_block": negative_example_block,
            "rules_text": rules_text,
            "user_description": user_description,
        },
        fallback_template=_ADAPT_PROMPT_TEMPLATE,
    )

    try:
        raw = await complete_text(full_prompt, llm_cfg)
        result = _parse_json_result(raw)
    except Exception as e:
        result = _fallback_prompt(user_description, provider, needs_transparent_bg)
        result["fallback_warning"] = f"提示词 AI 优化失败，已使用模板回退。原因：{type(e).__name__}: {e}"

    # 对最终 prompt 做内容安全替换（防止国内图生 API 内容审核拦截）
    if result.get("prompt"):
        result["prompt"] = _sanitize_for_content_policy(result["prompt"])
    return result


class DefaultPromptOptimizer(PromptOptimizer):
    async def optimize(
        self,
        user_description: str,
        asset_type: str,
        provider: str,
        needs_transparent_bg: bool,
    ) -> dict:
        return await adapt_prompt(user_description, asset_type, provider, needs_transparent_bg)


def _parse_json_result(text: str) -> dict:
    import json as _json

    start = text.find("{")
    end = text.rfind("}") + 1
    result = _json.loads(text[start:end] if start != -1 else text)
    return {
        "prompt": result.get("prompt", ""),
        "negative_prompt": result.get("negative_prompt"),
    }


async def _adapt_via_litellm(
    prompt: str,
    llm_cfg: dict,
    user_description: str,
    provider: ImageProvider,
    needs_transparent_bg: bool,
) -> dict:
    """用 litellm 做 prompt 适配（API key 模式）。"""
    model = resolve_model(llm_cfg)
    response = await litellm.acompletion(
        model=model,
        messages=[{"role": "user", "content": prompt}],
        api_key=llm_cfg.get("api_key") or None,
        api_base=llm_cfg.get("base_url") or None,
        temperature=0.3,
    )
    return _parse_json_result(response.choices[0].message.content.strip())


def _fallback_prompt(description: str, provider: ImageProvider, needs_transparent_bg: bool) -> dict:
    """LLM 不可用时的模板回退：只取外观关键词，去掉数值/机制。"""
    import re
    # 粗略去掉数字相关的游戏机制描述（"8点伤害"、"deal 8 damage" 等）
    visual = re.sub(r'[\d]+\s*[点块次张回]?[\w]*(?:伤害|攻击|防御|血|费|damage|block|cost|hp)\w*', '', description, flags=re.IGNORECASE)
    visual = re.sub(r'(?:造成|deal|gain|lose|add|remove)\s+[\d\w]+', '', visual, flags=re.IGNORECASE)
    # 清理第一步去掉数字后残留的孤立动词（如 "造成，" "获得，"）
    visual = re.sub(r'(?:造成|获得|失去|消耗|附加|增加|减少)[，,。\s]*', '', visual)
    visual = re.sub(r'升级后[\w，。,. ]+', '', visual)
    visual = re.sub(r'[，,]{2,}', '，', visual)  # 多余连续逗号
    visual = re.sub(r'\s{2,}', ' ', visual).strip(' ，,.')

    bg_suffix = _load_prompt_resource(
        "fallback_prompt_en_transparent_suffix.txt",
        _FALLBACK_PROMPT_EN_TRANSPARENT_SUFFIX,
    ) if needs_transparent_bg else ""
    if provider in ("flux2", "sdxl"):
        prompt = f"{visual}{_load_prompt_resource('fallback_prompt_en_suffix.txt', _FALLBACK_PROMPT_EN_SUFFIX)}{bg_suffix}"
        neg = _load_prompt_resource("fallback_sdxl_negative_prompt.txt", _FALLBACK_SDXL_NEGATIVE_PROMPT) if provider == "sdxl" else None
    else:
        bg_cn = _load_prompt_resource(
            "fallback_prompt_cn_transparent_suffix.txt",
            _FALLBACK_PROMPT_CN_TRANSPARENT_SUFFIX,
        ) if needs_transparent_bg else ""
        prompt = f"{visual}{_load_prompt_resource('fallback_prompt_cn_suffix.txt', _FALLBACK_PROMPT_CN_SUFFIX)}{bg_cn}"
        neg = None
    return {"prompt": prompt, "negative_prompt": neg}
