from __future__ import annotations


def allowed_api_protocols_for_runner_type(runner_type: str) -> set[str]:
    runner = str(runner_type or "").strip()
    if runner == "codex_cli":
        return {"openai_compatible"}
    if runner == "claude_cli":
        return {"anthropic_compatible"}
    if runner == "api":
        return {"openai_compatible", "anthropic_compatible"}
    return set()


def is_api_protocol_compatible_with_runner_type(*, runner_type: str, api_protocol: str) -> bool:
    protocol = str(api_protocol or "").strip()
    return bool(protocol) and protocol in allowed_api_protocols_for_runner_type(runner_type)


def require_api_protocol_compatible_with_runner_type(*, runner_type: str, api_protocol: str) -> None:
    if is_api_protocol_compatible_with_runner_type(runner_type=runner_type, api_protocol=api_protocol):
        return
    allowed = sorted(allowed_api_protocols_for_runner_type(runner_type))
    if not allowed:
        raise ValueError("runner_type must be codex_cli, claude_cli or api")
    raise ValueError(
        f"api_protocol must be one of {', '.join(allowed)} when runner_type is {str(runner_type).strip()}"
    )
