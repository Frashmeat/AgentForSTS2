"""构建统一 AI 审批动作的提示骨架。"""

from pathlib import Path

from app.shared.prompting import PromptLoader

_PROMPT_ROOT = Path(__file__).resolve().parent / "resources" / "prompts"
_PROMPT_LOADER = PromptLoader(root=_PROMPT_ROOT)
_ACTION_PROMPT_TEMPLATE = """你正在撰写统一 AI 审批动作的指导。 Output ONLY JSON，后续步骤依赖纯 JSON 格式输出。

User Input Requirements:
- {{ requirements_line }}

Template JSON structure to fill:
{"actions": [
  {
    "kind": "read_file | write_file | run_command | build_project | deploy_mod",
    "title": "简要标题，方便审批人员理解",
    "reason": "说明动作所需的业务背景与目标",
    "payload": {}
  }
]}
"""


def build_action_prompt(requirements: str) -> str:
    """返回包含统一动作结构的 AI 提示文本。"""
    requirements_line = requirements.strip() or "请提供必须满足的输入信息。"
    return _PROMPT_LOADER.render(
        "action_prompt.txt",
        {"requirements_line": requirements_line},
        fallback_template=_ACTION_PROMPT_TEMPLATE,
    )
