from __future__ import annotations

import asyncio
import json
import logging
import os
import shutil
import subprocess
import time
from collections.abc import Awaitable, Callable
from pathlib import Path

import httpx
import litellm

from app.shared.infra.llm.text_backend import (
    FunctionTextBackend,
    TextBackendRegistry,
    resolve_text_backend_name,
)
from app.shared.infra.llm.text_backend import (
    TextRunner as PortTextRunner,
)
from config import get_config, normalize_llm_config
from llm.agent_backends.claude_cli import _resolve_claude_launcher
from llm.prompt_builder import append_global_ai_instructions

DEFAULT_CLAUDE_MODEL = "claude-sonnet-4-6"
DEFAULT_LITELLM_USER_AGENT = "AgentTheSpire/0.1.0"
CLI_COMPLETION_TIMEOUT_SECONDS = 180
CLI_WAIT_TIMEOUT_SECONDS = CLI_COMPLETION_TIMEOUT_SECONDS + 5
_LOG_TAIL_LIMIT = 1200
_CODEX_EXECUTION_PROVIDER_ID = "platform_openai_compatible"
logger = logging.getLogger(__name__)


class OpenAICompatibleFallbackError(RuntimeError):
    def __init__(
        self,
        message: str,
        *,
        initial_error: Exception,
        final_error: Exception,
        endpoint: str,
    ) -> None:
        super().__init__(message)
        self.initial_error = initial_error
        self.final_error = final_error
        self.endpoint = endpoint
        self.upstream_attempts = [
            {
                "stage": "litellm",
                "error": str(initial_error),
                "exception_type": type(initial_error).__name__,
            },
            {
                "stage": "openai_compatible_direct",
                "endpoint": endpoint,
                "error": str(final_error),
                "exception_type": type(final_error).__name__,
            },
        ]


def _decode_output(raw: bytes) -> str:
    for encoding in ("utf-8", "gbk", "cp936"):
        try:
            return raw.decode(encoding)
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", errors="replace")


def _tail_text(raw: bytes | str | None, limit: int = _LOG_TAIL_LIMIT) -> str:
    if raw is None:
        return ""
    text = _decode_output(raw) if isinstance(raw, bytes) else str(raw)
    text = text.replace("\r", "\\r").replace("\n", "\\n").strip()
    return text[-limit:] if len(text) > limit else text


def _safe_base_url_state(llm_cfg: dict) -> str:
    return "configured" if str(llm_cfg.get("base_url") or "").strip() else "empty"


def resolve_model(llm_cfg: dict) -> str:
    cfg = normalize_llm_config(llm_cfg)
    if cfg.get("model"):
        return cfg["model"]
    return DEFAULT_CLAUDE_MODEL


def resolve_litellm_model(llm_cfg: dict) -> str:
    cfg = normalize_llm_config(llm_cfg)
    model = resolve_model(cfg)
    provider = str(cfg.get("provider", "")).strip().lower()
    base_url = str(cfg.get("base_url", "")).strip()
    if provider == "openai" and base_url and "/" not in model:
        return f"openai/{model}"
    return model


def build_litellm_extra_headers(llm_cfg: dict) -> dict[str, str]:
    raw_headers = llm_cfg.get("extra_headers")
    headers = dict(raw_headers) if isinstance(raw_headers, dict) else {}
    if not any(str(key).lower() == "user-agent" for key in headers):
        headers["User-Agent"] = DEFAULT_LITELLM_USER_AGENT
    return {str(key): str(value) for key, value in headers.items()}


def _should_use_openai_compatible_direct_fallback(llm_cfg: dict, error: Exception) -> bool:
    cfg = normalize_llm_config(llm_cfg)
    if not str(cfg.get("base_url", "")).strip():
        return False
    message = str(error)
    lower_message = message.lower()
    has_model_dump_error = "model_dump" in lower_message and "str" in lower_message
    if not has_model_dump_error:
        return False

    provider = str(cfg.get("provider", "")).strip().lower()
    model = resolve_litellm_model(cfg)
    return "openai" in lower_message or provider == "openai" or model.startswith("openai/")


def _openai_compatible_chat_completions_url(base_url: str) -> str:
    normalized = base_url.rstrip("/")
    if normalized.endswith("/chat/completions"):
        return normalized
    return f"{normalized}/chat/completions"


def _direct_openai_compatible_model(llm_cfg: dict) -> str:
    model = resolve_model(llm_cfg)
    if model.startswith("openai/"):
        return model[len("openai/") :]
    return model


def _extract_openai_compatible_content(payload: object) -> str:
    if not isinstance(payload, dict):
        raise RuntimeError("OpenAI-compatible response must be a JSON object")
    choices = payload.get("choices")
    if not isinstance(choices, list) or not choices:
        raise RuntimeError("OpenAI-compatible response missing choices")
    first_choice = choices[0]
    if not isinstance(first_choice, dict):
        raise RuntimeError("OpenAI-compatible response choice must be an object")
    message = first_choice.get("message")
    if not isinstance(message, dict):
        raise RuntimeError("OpenAI-compatible response choice missing message")
    content = message.get("content")
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, list):
        parts: list[str] = []
        for part in content:
            if isinstance(part, dict) and isinstance(part.get("text"), str):
                parts.append(part["text"])
            elif isinstance(part, str):
                parts.append(part)
        return "".join(parts).strip()
    raise RuntimeError("OpenAI-compatible response message content must be text")


def _parse_openai_compatible_json_response(response: httpx.Response) -> object:
    try:
        return response.json()
    except json.JSONDecodeError as error:
        content_type = response.headers.get("content-type", "")
        raise RuntimeError(
            "OpenAI-compatible direct response was not valid JSON "
            f"HTTP status {response.status_code} content_type={content_type or 'empty'} "
            f"body_tail={_tail_text(response.content)}"
        ) from error


def resolve_text_backend(llm_cfg: dict) -> str:
    cfg = normalize_llm_config(llm_cfg)
    return resolve_text_backend_name(cfg)


def _with_latest_runtime_custom_prompt(llm_cfg: dict) -> dict:
    runtime_llm_cfg = normalize_llm_config(get_config().get("llm"))
    merged = normalize_llm_config(llm_cfg)
    merged["custom_prompt"] = runtime_llm_cfg.get("custom_prompt", "")
    return merged


def build_text_prompt(prompt: str, llm_cfg: dict, use_runtime_config: bool = False) -> str:
    effective_cfg = _with_latest_runtime_custom_prompt(llm_cfg) if use_runtime_config else llm_cfg
    return append_global_ai_instructions(prompt, effective_cfg)


def build_system_prompt(system_prompt: str, llm_cfg: dict, use_runtime_config: bool = False) -> str:
    effective_cfg = _with_latest_runtime_custom_prompt(llm_cfg) if use_runtime_config else llm_cfg
    return append_global_ai_instructions(system_prompt, effective_cfg)


def _build_claude_cli_env(llm_cfg: dict) -> dict[str, str]:
    env = os.environ.copy()
    cfg = normalize_llm_config(llm_cfg)
    if cfg.get("api_key"):
        env["ANTHROPIC_AUTH_TOKEN"] = cfg["api_key"]
        env["ANTHROPIC_API_KEY"] = cfg["api_key"]
    if cfg.get("base_url"):
        env["ANTHROPIC_BASE_URL"] = cfg["base_url"]
    return env


def _toml_config_value(value: str | bool | int) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, int):
        return str(value)
    return json.dumps(value)


def _build_codex_cli_env(llm_cfg: dict) -> dict[str, str]:
    env = os.environ.copy()
    env.pop("OPENAI_BASE_URL", None)
    cfg = normalize_llm_config(llm_cfg)
    if cfg.get("api_key"):
        env["OPENAI_API_KEY"] = str(cfg["api_key"])
    return env


def _build_codex_cli_provider_config_args(llm_cfg: dict) -> list[str]:
    cfg = normalize_llm_config(llm_cfg)
    provider_key = f"model_providers.{_CODEX_EXECUTION_PROVIDER_ID}"
    config_pairs: list[tuple[str, str | bool | int]] = [
        ("model_provider", _CODEX_EXECUTION_PROVIDER_ID),
        (f"{provider_key}.name", _CODEX_EXECUTION_PROVIDER_ID),
        (f"{provider_key}.wire_api", "responses"),
        (f"{provider_key}.env_key", "OPENAI_API_KEY"),
        (f"{provider_key}.requires_openai_auth", False),
        (f"{provider_key}.supports_websockets", False),
    ]
    base_url = str(cfg.get("base_url") or "").strip()
    if base_url:
        config_pairs.append((f"{provider_key}.base_url", base_url))

    args: list[str] = []
    for key, value in config_pairs:
        args.extend(["-c", f"{key}={_toml_config_value(value)}"])
    return args


async def complete_text(
    prompt: str,
    llm_cfg: dict,
    cwd: Path | None = None,
) -> str:
    prompt = build_text_prompt(prompt, llm_cfg, use_runtime_config=True)
    runner = TextRunner(registry=_build_default_registry())
    return await runner.complete(prompt, normalize_llm_config(llm_cfg), cwd)


async def stream_text(
    system_prompt: str,
    user_prompt: str,
    llm_cfg: dict,
    on_chunk: Callable[[str], Awaitable[None]],
    cwd: Path | None = None,
) -> str:
    system_prompt = build_system_prompt(system_prompt, llm_cfg, use_runtime_config=True)
    runner = TextRunner(registry=_build_default_registry())
    return await runner.stream(system_prompt, user_prompt, normalize_llm_config(llm_cfg), on_chunk, cwd)


class TextRunner(PortTextRunner):
    pass


def _build_default_registry() -> TextBackendRegistry:
    registry = TextBackendRegistry()
    registry.register(
        "litellm",
        FunctionTextBackend(complete_fn=_complete_via_litellm, stream_fn=_stream_via_litellm),
    )
    registry.register(
        "codex_cli",
        FunctionTextBackend(complete_fn=_complete_via_codex_cli, stream_fn=_stream_via_cli_completion),
    )
    registry.register(
        "claude_cli",
        FunctionTextBackend(complete_fn=_complete_via_claude_cli, stream_fn=_stream_via_cli_completion),
    )
    return registry


async def _stream_via_cli_completion(
    system_prompt: str,
    user_prompt: str,
    llm_cfg: dict,
    on_chunk: Callable[[str], Awaitable[None]],
    cwd: Path | None = None,
) -> str:
    backend = resolve_text_backend(llm_cfg)
    full_prompt = f"{system_prompt}\n\n{user_prompt}"
    if backend == "codex_cli":
        full_text = await _complete_via_codex_cli(full_prompt, llm_cfg, cwd)
    else:
        full_text = await _complete_via_claude_cli(full_prompt, llm_cfg, cwd)

    chunk_size = 80
    for i in range(0, len(full_text), chunk_size):
        await on_chunk(full_text[i : i + chunk_size])
        await asyncio.sleep(0)
    return full_text


async def _complete_via_claude_cli(prompt: str, llm_cfg: dict, cwd: Path | None) -> str:
    cmd = [*_resolve_claude_launcher(), "--print"]
    model = normalize_llm_config(llm_cfg).get("model")
    if model:
        cmd.extend(["--model", model])
    cmd.extend(["-p", prompt])

    loop = asyncio.get_event_loop()
    started_at = time.monotonic()
    logger.info(
        "text cli start backend=claude_cli model=%s base_url=%s cwd=%s prompt_len=%d timeout_seconds=%d",
        model or "",
        _safe_base_url_state(llm_cfg),
        str(cwd) if cwd else "",
        len(prompt),
        CLI_COMPLETION_TIMEOUT_SECONDS,
    )
    try:
        result = await asyncio.wait_for(
            loop.run_in_executor(
                None,
                lambda: subprocess.run(
                    cmd,
                    capture_output=True,
                    timeout=CLI_COMPLETION_TIMEOUT_SECONDS,
                    cwd=str(cwd) if cwd else None,
                    env=_build_claude_cli_env(llm_cfg),
                ),
            ),
            timeout=CLI_WAIT_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as error:
        elapsed_ms = int((time.monotonic() - started_at) * 1000)
        logger.warning(
            "text cli timeout backend=claude_cli model=%s base_url=%s cwd=%s prompt_len=%d timeout_seconds=%d "
            "elapsed_ms=%d stdout_tail=%s stderr_tail=%s",
            model or "",
            _safe_base_url_state(llm_cfg),
            str(cwd) if cwd else "",
            len(prompt),
            CLI_COMPLETION_TIMEOUT_SECONDS,
            elapsed_ms,
            _tail_text(error.stdout),
            _tail_text(error.stderr),
        )
        raise
    elapsed_ms = int((time.monotonic() - started_at) * 1000)
    stdout = getattr(result, "stdout", b"") or b""
    stderr = getattr(result, "stderr", b"") or b""
    returncode = getattr(result, "returncode", None)
    logger.info(
        "text cli finished backend=claude_cli model=%s returncode=%s elapsed_ms=%d stdout_len=%d stderr_len=%d stderr_tail=%s",
        model or "",
        returncode,
        elapsed_ms,
        len(stdout),
        len(stderr),
        _tail_text(stderr),
    )
    return stdout.decode("utf-8", errors="replace").strip()


async def _complete_via_codex_cli(prompt: str, llm_cfg: dict, cwd: Path | None) -> str:
    codex_exe = shutil.which("codex.cmd" if os.name == "nt" else "codex") or shutil.which("codex")
    if not codex_exe:
        raise RuntimeError("未找到 Codex CLI，请先安装并确保 codex 可执行文件在 PATH 中")

    cmd = [
        codex_exe,
        "exec",
        *_build_codex_cli_provider_config_args(llm_cfg),
        "--ignore-user-config",
        "--full-auto",
        "--color",
        "never",
        "--skip-git-repo-check",
        "-",
    ]
    if cwd:
        cmd[2:2] = ["-C", str(cwd)]
    model = normalize_llm_config(llm_cfg).get("model")
    if model:
        cmd[2:2] = ["-m", model]

    loop = asyncio.get_event_loop()
    started_at = time.monotonic()
    logger.info(
        "text cli start backend=codex_cli model=%s base_url=%s cwd=%s prompt_len=%d timeout_seconds=%d codex_path=%s",
        model or "",
        _safe_base_url_state(llm_cfg),
        str(cwd) if cwd else "",
        len(prompt),
        CLI_COMPLETION_TIMEOUT_SECONDS,
        codex_exe,
    )
    try:
        result = await asyncio.wait_for(
            loop.run_in_executor(
                None,
                lambda: subprocess.run(
                    cmd,
                    input=prompt.encode("utf-8", errors="replace"),
                    capture_output=True,
                    timeout=CLI_COMPLETION_TIMEOUT_SECONDS,
                    cwd=str(cwd) if cwd else None,
                    env=_build_codex_cli_env(llm_cfg),
                ),
            ),
            timeout=CLI_WAIT_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired as error:
        elapsed_ms = int((time.monotonic() - started_at) * 1000)
        logger.warning(
            "text cli timeout backend=codex_cli model=%s base_url=%s cwd=%s prompt_len=%d timeout_seconds=%d "
            "elapsed_ms=%d codex_path=%s stdout_tail=%s stderr_tail=%s",
            model or "",
            _safe_base_url_state(llm_cfg),
            str(cwd) if cwd else "",
            len(prompt),
            CLI_COMPLETION_TIMEOUT_SECONDS,
            elapsed_ms,
            codex_exe,
            _tail_text(error.stdout),
            _tail_text(error.stderr),
        )
        raise
    elapsed_ms = int((time.monotonic() - started_at) * 1000)
    stdout = getattr(result, "stdout", b"") or b""
    stderr = getattr(result, "stderr", b"") or b""
    returncode = getattr(result, "returncode", None)
    logger.info(
        "text cli finished backend=codex_cli model=%s returncode=%s elapsed_ms=%d stdout_len=%d stderr_len=%d stderr_tail=%s",
        model or "",
        returncode,
        elapsed_ms,
        len(stdout),
        len(stderr),
        _tail_text(stderr),
    )
    if returncode != 0:
        detail = _decode_output(stderr).strip()
        raise RuntimeError(f"Codex CLI 退出码 {returncode}\n{detail}")
    return _decode_output(stdout).strip()


async def _complete_via_litellm(prompt: str, llm_cfg: dict, cwd: Path | None = None) -> str:
    _ = cwd
    try:
        response = await litellm.acompletion(
            model=resolve_litellm_model(llm_cfg),
            messages=[{"role": "user", "content": prompt}],
            api_key=llm_cfg.get("api_key") or None,
            api_base=llm_cfg.get("base_url") or None,
            extra_headers=build_litellm_extra_headers(llm_cfg),
            temperature=0.2,
            max_tokens=2048,
        )
    except Exception as error:
        if not _should_use_openai_compatible_direct_fallback(llm_cfg, error):
            raise
        logger.warning(
            "litellm openai-compatible response parsing failed; retrying direct http model=%s base_url=%s error=%s",
            resolve_model(llm_cfg),
            _safe_base_url_state(llm_cfg),
            str(error)[:160],
        )
        try:
            return await _complete_openai_compatible_direct(prompt, llm_cfg)
        except Exception as fallback_error:
            endpoint = _openai_compatible_chat_completions_url(str(llm_cfg.get("base_url") or "").strip())
            raise OpenAICompatibleFallbackError(
                (
                    "OpenAI-compatible fallback failed after LiteLLM response parsing error; "
                    f"initial_error={error}; final_error={fallback_error}"
                ),
                initial_error=error,
                final_error=fallback_error,
                endpoint=endpoint,
            ) from fallback_error
    return response.choices[0].message.content.strip()


async def _complete_openai_compatible_direct(prompt: str, llm_cfg: dict) -> str:
    base_url = str(llm_cfg.get("base_url") or "").strip()
    if not base_url:
        raise RuntimeError("OpenAI-compatible base_url is required for direct fallback")

    headers = build_litellm_extra_headers(llm_cfg)
    api_key = str(llm_cfg.get("api_key") or "").strip()
    if api_key and not any(str(key).lower() == "authorization" for key in headers):
        headers["Authorization"] = f"Bearer {api_key}"

    async with httpx.AsyncClient(timeout=180) as client:
        response = await client.post(
            _openai_compatible_chat_completions_url(base_url),
            headers=headers,
            json={
                "model": _direct_openai_compatible_model(llm_cfg),
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.2,
                "max_tokens": 2048,
            },
        )
        response.raise_for_status()
        return _extract_openai_compatible_content(_parse_openai_compatible_json_response(response))


async def _stream_via_litellm(
    system_prompt: str,
    user_prompt: str,
    llm_cfg: dict,
    on_chunk: Callable[[str], Awaitable[None]],
    cwd: Path | None = None,
) -> str:
    _ = cwd
    stream = await litellm.acompletion(
        model=resolve_litellm_model(llm_cfg),
        messages=[
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
        api_key=llm_cfg.get("api_key") or None,
        api_base=llm_cfg.get("base_url") or None,
        extra_headers=build_litellm_extra_headers(llm_cfg),
        temperature=0.2,
        max_tokens=2048,
        stream=True,
    )

    full_text: list[str] = []
    async for chunk in stream:
        delta = chunk.choices[0].delta.content or ""
        if delta:
            full_text.append(delta)
            await on_chunk(delta)

    return "".join(full_text)
