from __future__ import annotations

import asyncio
import os
import shutil
import subprocess
from collections.abc import Awaitable, Callable
from pathlib import Path

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


def _decode_output(raw: bytes) -> str:
    for encoding in ("utf-8", "gbk", "cp936"):
        try:
            return raw.decode(encoding)
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", errors="replace")


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
    result = await asyncio.wait_for(
        loop.run_in_executor(
            None,
            lambda: subprocess.run(
                cmd,
                capture_output=True,
                timeout=180,
                cwd=str(cwd) if cwd else None,
                env=_build_claude_cli_env(llm_cfg),
            ),
        ),
        timeout=185,
    )
    return result.stdout.decode("utf-8", errors="replace").strip()


async def _complete_via_codex_cli(prompt: str, llm_cfg: dict, cwd: Path | None) -> str:
    env = os.environ.copy()
    if llm_cfg.get("api_key"):
        env["OPENAI_API_KEY"] = str(llm_cfg["api_key"])
    if llm_cfg.get("base_url"):
        env["OPENAI_BASE_URL"] = str(llm_cfg["base_url"])

    codex_exe = shutil.which("codex.cmd" if os.name == "nt" else "codex") or shutil.which("codex")
    if not codex_exe:
        raise RuntimeError("未找到 Codex CLI，请先安装并确保 codex 可执行文件在 PATH 中")

    cmd = [
        codex_exe,
        "exec",
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
    result = await asyncio.wait_for(
        loop.run_in_executor(
            None,
            lambda: subprocess.run(
                cmd,
                input=prompt.encode("utf-8", errors="replace"),
                capture_output=True,
                timeout=180,
                cwd=str(cwd) if cwd else None,
                env=env,
            ),
        ),
        timeout=185,
    )
    if result.returncode != 0:
        detail = _decode_output(result.stderr).strip()
        raise RuntimeError(f"Codex CLI 退出码 {result.returncode}\n{detail}")
    return _decode_output(result.stdout).strip()


async def _complete_via_litellm(prompt: str, llm_cfg: dict, cwd: Path | None = None) -> str:
    _ = cwd
    response = await litellm.acompletion(
        model=resolve_litellm_model(llm_cfg),
        messages=[{"role": "user", "content": prompt}],
        api_key=llm_cfg.get("api_key") or None,
        api_base=llm_cfg.get("base_url") or None,
        extra_headers=build_litellm_extra_headers(llm_cfg),
        temperature=0.2,
        max_tokens=2048,
    )
    return response.choices[0].message.content.strip()


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
