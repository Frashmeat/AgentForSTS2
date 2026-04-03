"""
游戏日志分析器：读取 STS2 的 godot.log，提取报错信息，交给 LLM 分析。
支持 WebSocket 流式返回分析结果。
"""
from __future__ import annotations

import json
import os
import re
from pathlib import Path

from fastapi import APIRouter, WebSocket

from app.shared.prompting import PromptLoader
from config import get_config
from llm.stream import stream_analysis
from llm.stage_events import build_stage_event

router = APIRouter()

# STS2 日志路径（Windows）
_LOG_PATH = Path(os.environ.get("APPDATA", "")) / "SlayTheSpire2" / "logs" / "godot.log"

# 提取 log 时只保留最后这么多行（避免 token 爆炸）
_MAX_LINES = 300

_PROMPT_LOADER = PromptLoader()
_LOG_ANALYZER_SYSTEM_PROMPT_KEY = "analyzer.log_analyzer_system"
_LOG_ANALYZER_USER_PROMPT_KEY = "analyzer.log_analyzer_user"
_LOG_ANALYZER_EXTRA_CONTEXT_PROMPT_KEY = "analyzer.log_analyzer_extra_context"


async def _send_stage(ws: WebSocket, scope: str, stage: str, message: str):
    payload = build_stage_event(scope, stage, message)
    if payload:
        await ws.send_text(json.dumps({"event": "stage_update", **payload}))


def _read_log() -> tuple[str, bool]:
    """
    读取游戏日志，提取最后 _MAX_LINES 行以及所有包含 ERROR/Exception/CRITICAL 的行。
    返回 (提取内容, 日志是否存在)。
    """
    if not _LOG_PATH.exists():
        return "", False

    text = _LOG_PATH.read_text(encoding="utf-8", errors="replace")
    lines = text.splitlines()

    # 取最后 _MAX_LINES 行
    tail = lines[-_MAX_LINES:]

    # 额外捞出全文中的 ERROR/Exception/CRITICAL 行（可能在 tail 之前）
    error_pattern = re.compile(r"error|exception|critical|crash|fail", re.IGNORECASE)
    extra = [l for l in lines[:-_MAX_LINES] if error_pattern.search(l)]

    combined = []
    if extra:
        combined.append(_PROMPT_LOADER.load("analyzer.log_excerpt_header").strip())
        combined.extend(extra[-100:])   # 最多 100 条
        combined.append(_PROMPT_LOADER.load("analyzer.log_tail_header").strip())
    combined.extend(tail)

    return "\n".join(combined), True


def _get_system_prompt() -> str:
    return _PROMPT_LOADER.load(_LOG_ANALYZER_SYSTEM_PROMPT_KEY)


def _build_prompt(extra_context: str) -> str:
    log_content, exists = _read_log()
    if not exists:
        return _PROMPT_LOADER.render("analyzer.log_missing_message", {"log_path": _LOG_PATH}).strip()

    extra_context_block = ""
    if extra_context:
        extra_context_block = _PROMPT_LOADER.render(
            _LOG_ANALYZER_EXTRA_CONTEXT_PROMPT_KEY,
            {
                "extra_context": extra_context,
            },
        )

    return _PROMPT_LOADER.render(
        _LOG_ANALYZER_USER_PROMPT_KEY,
        {
            "extra_context_block": extra_context_block,
            "log_content": log_content,
            "log_path": _LOG_PATH,
        },
    )


@router.websocket("/ws/analyze-log")
async def ws_analyze_log(ws: WebSocket):
    """
    WebSocket 端点：流式返回日志分析结果。

    客户端发送：
    {
        "context": "黑屏了，刚加了 MyMod"   // 可选，用户补充描述
    }

    服务端推流事件：
    - log_info: { lines: N }              // 读到了多少行
    - stream:   { chunk: "..." }          // LLM 流式文本片段
    - done:     { full: "完整分析结果" }
    - error:    { message: "..." }
    """
    await ws.accept()
    try:
        raw = await ws.receive_text()
        params = json.loads(raw)
        extra_context = params.get("context", "")

        # 读日志
        await _send_stage(ws, "text", "reading_input", _PROMPT_LOADER.load("analyzer.log_reading_stage").strip())
        log_content, exists = _read_log()
        if not exists:
            await ws.send_text(json.dumps({
                "event": "error",
                "message": _PROMPT_LOADER.render("analyzer.log_missing_message", {"log_path": _LOG_PATH}).strip()
            }))
            return

        line_count = log_content.count("\n") + 1
        await ws.send_text(json.dumps({"event": "log_info", "lines": line_count}))

        # 构建 prompt
        await _send_stage(ws, "text", "preparing_prompt", _PROMPT_LOADER.load("analyzer.log_preparing_stage").strip())
        prompt = _build_prompt(extra_context)

        cfg = get_config()
        llm_cfg = cfg["llm"]
        streamed = False

        async def send_chunk(chunk: str):
            nonlocal streamed
            if not streamed:
                streamed = True
                await _send_stage(ws, "text", "ai_streaming", _PROMPT_LOADER.load("analyzer.log_streaming_stage").strip())
            await ws.send_text(json.dumps({"event": "stream", "chunk": chunk}))

        await _send_stage(ws, "text", "ai_running", _PROMPT_LOADER.load("analyzer.log_running_stage").strip())
        full_text = await stream_analysis(_get_system_prompt(), prompt, llm_cfg, send_chunk)
        await _send_stage(ws, "text", "done", _PROMPT_LOADER.load("analyzer.log_done_stage").strip())
        await ws.send_text(json.dumps({"event": "done", "full": full_text}))

    except Exception as e:
        try:
            await ws.send_text(json.dumps({"event": "error", "message": str(e)}))
        except Exception:
            pass
