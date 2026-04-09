from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from llm.stream_metadata import (
    build_stream_chunk_payload,
    resolve_agent_display_model,
    resolve_text_display_model,
)


def test_resolve_agent_display_model_prefers_explicit_codex_model():
    assert resolve_agent_display_model({"agent_backend": "codex", "model": "gpt-5.4"}) == "gpt-5.4"


def test_resolve_agent_display_model_falls_back_to_backend_default_label():
    assert resolve_agent_display_model({"agent_backend": "claude"}) == "Claude CLI 默认模型"


def test_resolve_agent_display_model_uses_claude_default_in_claude_api_mode():
    assert resolve_agent_display_model({"mode": "claude_api"}) == "claude-sonnet-4-6"


def test_resolve_text_display_model_uses_claude_default():
    assert resolve_text_display_model({"mode": "claude_api"}) == "claude-sonnet-4-6"


def test_build_stream_chunk_payload_keeps_model_source_and_channel():
    assert build_stream_chunk_payload(
        "chunk-1",
        source="analysis",
        channel="stderr",
        model="claude-sonnet-4-6",
    ) == {
        "chunk": "chunk-1",
        "source": "analysis",
        "channel": "stderr",
        "model": "claude-sonnet-4-6",
    }
