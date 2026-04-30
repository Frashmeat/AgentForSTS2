"""
Mod 分析器：扫描 mod 项目 .cs 源码和 localization 文件，交给 LLM 分析内容。
"""

from __future__ import annotations

import json
from pathlib import Path

from fastapi import APIRouter, WebSocket

from app.shared.infra.ws_errors import send_ws_error
from app.shared.prompting import PromptLoader
from config import get_config
from llm.stage_events import build_stage_event
from llm.stream import stream_analysis
from llm.stream_metadata import build_stream_chunk_payload, resolve_text_display_model

router = APIRouter()

_SKIP_DIRS = {"bin", "obj", ".godot", "packages", ".git", ".vs", "__pycache__"}

_PROMPT_LOADER = PromptLoader()
_MOD_ANALYZER_SYSTEM_PROMPT_KEY = "analyzer.mod_analyzer_system"
_MOD_ANALYZER_USER_PROMPT_KEY = "analyzer.mod_analyzer_user"


async def _send_stage(ws: WebSocket, scope: str, stage: str, message: str):
    payload = build_stage_event(scope, stage, message)
    if payload:
        await ws.send_text(json.dumps({"event": "stage_update", **payload}))


def _get_system_prompt() -> str:
    return _PROMPT_LOADER.load(_MOD_ANALYZER_SYSTEM_PROMPT_KEY)


def _build_prompt(project_root: Path, file_content: str) -> str:
    return _PROMPT_LOADER.render(
        _MOD_ANALYZER_USER_PROMPT_KEY,
        {
            "project_root": project_root,
            "file_content": file_content,
        },
    )


def _scan_mod_files(project_root: Path) -> tuple[str, int]:
    """扫描 mod 项目的 .cs 和 localization JSON 文件，返回 (合并内容, 文件数)。"""
    parts: list[str] = []
    file_count = 0
    total_chars = 0
    MAX_TOTAL = 80_000  # 约 20k token，留足 LLM 回复空间

    # .cs 文件（按路径排序，Cards/Relics/Powers 优先）
    cs_files = sorted(
        (f for f in project_root.rglob("*.cs") if not any(p in _SKIP_DIRS for p in f.parts)),
        key=lambda f: (0 if any(p in {"Cards", "Relics", "Powers", "Patches"} for p in f.parts) else 1, str(f)),
    )

    for f in cs_files:
        if total_chars >= MAX_TOTAL:
            break
        try:
            content = f.read_text(encoding="utf-8", errors="replace")
            rel = str(f.relative_to(project_root))
            snippet = f"// {rel}\n{content[:4000]}"
            if len(content) > 4000:
                snippet += "\n// ... (truncated)"
            parts.append(snippet)
            total_chars += len(snippet)
            file_count += 1
        except Exception:
            pass

    # localization JSON
    for f in sorted(project_root.rglob("*.json")):
        if "localization" not in f.parts:
            continue
        if total_chars >= MAX_TOTAL:
            break
        try:
            content = f.read_text(encoding="utf-8", errors="replace")
            rel = str(f.relative_to(project_root))
            snippet = f"// Localization: {rel}\n{content[:2000]}"
            if len(content) > 2000:
                snippet += "\n// ... (truncated)"
            parts.append(snippet)
            total_chars += len(snippet)
            file_count += 1
        except Exception:
            pass

    return "\n\n".join(parts), file_count


@router.websocket("/ws/analyze-mod")
async def ws_analyze_mod(ws: WebSocket):
    """
    WebSocket 端点：扫描 mod 项目源码，流式返回 LLM 分析结果。

    客户端发送：{ "project_root": "E:/STS2mod/MyMod" }

    服务端推流：
    - scan_info: { files: N }
    - stream:    { chunk }
    - done:      { full }
    - error:     { message }
    """
    await ws.accept()
    try:
        raw = await ws.receive_text()
        params = json.loads(raw)
        project_root = Path(params.get("project_root", ""))

        await _send_stage(ws, "text", "reading_input", _PROMPT_LOADER.load("analyzer.mod_reading_stage").strip())

        if not project_root.exists():
            message = _PROMPT_LOADER.render("analyzer.mod_path_missing_message", {"project_root": project_root}).strip()
            await send_ws_error(
                ws,
                code="project_root_missing",
                message=message,
                detail=message,
            )
            return

        file_content, file_count = _scan_mod_files(project_root)

        if not file_content.strip():
            message = _PROMPT_LOADER.render(
                "analyzer.mod_source_missing_message", {"project_root": project_root}
            ).strip()
            await send_ws_error(
                ws,
                code="source_files_missing",
                message=message,
                detail=message,
            )
            return

        await ws.send_text(json.dumps({"event": "scan_info", "files": file_count}))

        await _send_stage(ws, "text", "preparing_prompt", _PROMPT_LOADER.load("analyzer.mod_preparing_stage").strip())
        prompt = _build_prompt(project_root, file_content)

        cfg = get_config()
        llm_cfg = cfg["llm"]
        display_model = resolve_text_display_model(llm_cfg)
        streamed = False

        async def send_chunk(chunk: str):
            nonlocal streamed
            if not streamed:
                streamed = True
                await _send_stage(
                    ws, "text", "ai_streaming", _PROMPT_LOADER.load("analyzer.mod_streaming_stage").strip()
                )
            await ws.send_text(
                json.dumps(
                    {
                        "event": "stream",
                        **build_stream_chunk_payload(
                            chunk,
                            source="analysis",
                            model=display_model,
                        ),
                    }
                )
            )

        await _send_stage(ws, "text", "ai_running", _PROMPT_LOADER.load("analyzer.mod_running_stage").strip())
        full_text = await stream_analysis(_get_system_prompt(), prompt, llm_cfg, send_chunk)
        await _send_stage(ws, "text", "done", _PROMPT_LOADER.load("analyzer.mod_done_stage").strip())
        await ws.send_text(json.dumps({"event": "done", "full": full_text}))

    except Exception as e:
        try:
            await send_ws_error(
                ws,
                code="mod_analysis_failed",
                message=str(e),
                detail=str(e),
            )
        except Exception:
            pass
