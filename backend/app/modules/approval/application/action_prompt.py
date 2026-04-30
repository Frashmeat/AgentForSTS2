"""构建统一 AI 审批动作的提示骨架。"""

from app.shared.prompting import PromptLoader

_SHARED_PROMPT_LOADER = PromptLoader()
_ACTION_PROMPT_BUNDLE_KEY = "runtime_agent.approval_action_prompt"


def build_action_prompt(requirements: str) -> str:
    """返回包含统一动作结构的 AI 提示文本。"""
    requirements_line = (
        requirements.strip() or _SHARED_PROMPT_LOADER.load("runtime_agent.approval_default_requirements_line").strip()
    )
    return _SHARED_PROMPT_LOADER.render(
        _ACTION_PROMPT_BUNDLE_KEY,
        {"requirements_line": requirements_line},
    )
