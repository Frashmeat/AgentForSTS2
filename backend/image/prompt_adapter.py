"""
Prompt Adapter：将用户的卡牌/遗物设计描述翻译为各图生模型专用 prompt。
通过 LLM 调用实现，不硬编码翻译规则。
"""

from __future__ import annotations

from typing import Literal

import litellm

from app.modules.image.application.ports import PromptOptimizer
from app.shared.prompting import PromptLoader
from config import get_config
from llm.text_runner import complete_text, resolve_model

ImageProvider = Literal["flux2", "sdxl", "jimeng", "wanxiang"]

_PROMPT_LOADER = PromptLoader()
_ADAPT_PROMPT_KEY = "image.adapt_prompt"


# 内容审核敏感词替换表（针对即梦/火山引擎等国内 API 的限制）
_CONTENT_SAFE_REPLACEMENTS: list[tuple[str, str]] = [
    # 爆炸物相关
    (r"\bbomb\b", "orb"),
    (r"\bexplosive\b", "charged"),
    (r"\bexplosion\b", "burst"),
    (r"\bdetonate\b", "release"),
    (r"\bblast\b", "surge"),
    # 武器相关（部分 API 限制）
    (r"\bgun\b", "wand"),
    (r"\bpistol\b", "rod"),
    (r"\brifle\b", "staff"),
]


def _load_prompt_resource(name: str, fallback_text: str) -> str:
    return _PROMPT_LOADER.load(name).strip()


def _load_prompt_resource_lines(name: str, fallback_lines: list[str]) -> list[str]:
    text = _load_prompt_resource(name, "")
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
    for provider in ("flux2", "sdxl", "jimeng", "wanxiang"):
        guide = {
            "lang": _load_prompt_resource(f"image.guide_{provider}_lang", ""),
            "formula": _load_prompt_resource(f"image.guide_{provider}_formula", ""),
            "rules": _load_prompt_resource_lines(f"image.guide_{provider}_rules", []),
            "example": _load_prompt_resource(f"image.guide_{provider}_example", ""),
        }
        negative_example_key = f"image.guide_{provider}_negative_example"
        try:
            guide["negative_example"] = _load_prompt_resource(negative_example_key, "")
        except FileNotFoundError:
            pass
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
        rules_text += "\n- " + _load_prompt_resource("image.transparent_bg_rule", "")

    full_prompt = _PROMPT_LOADER.render(
        _ADAPT_PROMPT_KEY,
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
    )

    try:
        raw = await complete_text(full_prompt, llm_cfg)
        result = _parse_json_result(raw)
    except Exception as e:
        result = _fallback_prompt(user_description, provider, needs_transparent_bg)
        result["fallback_warning"] = _PROMPT_LOADER.render(
            "image.adapt_fallback_warning",
            {
                "error_type": type(e).__name__,
                "error_message": e,
            },
        )

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
    visual = re.sub(
        r"[\d]+\s*[点块次张回]?[\w]*(?:伤害|攻击|防御|血|费|damage|block|cost|hp)\w*",
        "",
        description,
        flags=re.IGNORECASE,
    )
    visual = re.sub(r"(?:造成|deal|gain|lose|add|remove)\s+[\d\w]+", "", visual, flags=re.IGNORECASE)
    # 清理第一步去掉数字后残留的孤立动词（如 "造成，" "获得，"）
    visual = re.sub(r"(?:造成|获得|失去|消耗|附加|增加|减少)[，,。\s]*", "", visual)
    visual = re.sub(r"升级后[\w，。,. ]+", "", visual)
    visual = re.sub(r"[，,]{2,}", "，", visual)  # 多余连续逗号
    visual = re.sub(r"\s{2,}", " ", visual).strip(" ，,.")

    bg_suffix = (
        _load_prompt_resource(
            "image.fallback_prompt_en_transparent_suffix",
            "",
        )
        if needs_transparent_bg
        else ""
    )
    if provider in ("flux2", "sdxl"):
        prompt = f"{visual}{_load_prompt_resource('image.fallback_prompt_en_suffix', '')}{bg_suffix}"
        neg = _load_prompt_resource("image.fallback_sdxl_negative_prompt", "") if provider == "sdxl" else None
    else:
        bg_cn = (
            _load_prompt_resource(
                "image.fallback_prompt_cn_transparent_suffix",
                "",
            )
            if needs_transparent_bg
            else ""
        )
        prompt = f"{visual}{_load_prompt_resource('image.fallback_prompt_cn_suffix', '')}{bg_cn}"
        neg = None
    return {"prompt": prompt, "negative_prompt": neg}
