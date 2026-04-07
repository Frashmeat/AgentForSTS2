import asyncio
import importlib
import json
import sys
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))


class _DummyRouter:
    def websocket(self, _path):
        def decorator(func):
            return func

        return decorator


def _load_log_analyzer():
    sys.modules["fastapi"] = types.SimpleNamespace(APIRouter=lambda: _DummyRouter(), WebSocket=object)
    sys.modules["config"] = types.SimpleNamespace(get_config=lambda: {"llm": {}})
    sys.modules["llm.stream"] = types.SimpleNamespace(stream_analysis=lambda *args, **kwargs: None)
    sys.modules["llm.stage_events"] = types.SimpleNamespace(build_stage_event=lambda *args, **kwargs: None)
    sys.modules.pop("routers.log_analyzer", None)
    return importlib.import_module("routers.log_analyzer")


def test_log_analyzer_prompt_templates_exist_for_real_loader():
    module = _load_log_analyzer()

    system_template = module._PROMPT_LOADER.load("analyzer.log_analyzer_system")
    user_template = module._PROMPT_LOADER.load("analyzer.log_analyzer_user")
    extra_context_template = module._PROMPT_LOADER.load("analyzer.log_analyzer_extra_context")

    assert "Slay the Spire 2 mod 开发专家" in system_template
    assert "{{ log_path }}" in user_template
    assert "{{ log_content }}" in user_template
    assert "{{ extra_context_block }}" in user_template
    assert "{{ extra_context }}" in extra_context_template


def test_get_system_prompt_uses_prompt_loader():
    module = _load_log_analyzer()

    class FakePromptLoader:
        def __init__(self) -> None:
            self.load_calls: list[tuple[str, str]] = []

        def load(self, template_name: str, *, fallback_template: str | None = None) -> str:
            self.load_calls.append((template_name, fallback_template or ""))
            return "rendered-system-prompt"

    loader = FakePromptLoader()
    module._PROMPT_LOADER = loader

    prompt = module._get_system_prompt()

    assert prompt == "rendered-system-prompt"
    assert loader.load_calls == [("analyzer.log_analyzer_system", "")]


def test_build_prompt_uses_prompt_loader_for_user_and_extra_context(monkeypatch):
    module = _load_log_analyzer()

    class FakePromptLoader:
        def __init__(self) -> None:
            self.render_calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.render_calls.append((template_name, variables, fallback_template or ""))
            return f"rendered:{template_name}"

    loader = FakePromptLoader()
    module._PROMPT_LOADER = loader
    monkeypatch.setattr(module, "_read_log", lambda: ("line1\nline2", True))

    prompt = module._build_prompt("黑屏了，刚加了 MyMod")

    assert prompt == "rendered:analyzer.log_analyzer_user"
    assert len(loader.render_calls) == 2

    extra_name, extra_variables, extra_fallback = loader.render_calls[0]
    assert extra_name == "analyzer.log_analyzer_extra_context"
    assert extra_variables == {"extra_context": "黑屏了，刚加了 MyMod"}
    assert extra_fallback == ""

    user_name, user_variables, user_fallback = loader.render_calls[1]
    assert user_name == "analyzer.log_analyzer_user"
    assert user_variables["log_path"] == module._LOG_PATH
    assert user_variables["log_content"] == "line1\nline2"
    assert user_variables["extra_context_block"] == "rendered:analyzer.log_analyzer_extra_context"
    assert user_fallback == ""


def test_build_prompt_returns_missing_log_message_when_log_missing(monkeypatch):
    module = _load_log_analyzer()
    monkeypatch.setattr(module, "_read_log", lambda: ("", False))

    prompt = module._build_prompt("ignored")

    assert str(module._LOG_PATH) in prompt
    assert "请确认游戏已运行过至少一次" in prompt


class _FakeWebSocket:
    def __init__(self, payload: dict[str, object]) -> None:
        self._payload = json.dumps(payload)
        self.accepted = False
        self.messages: list[dict[str, object]] = []

    async def accept(self):
        self.accepted = True

    async def receive_text(self) -> str:
        return self._payload

    async def send_text(self, text: str):
        self.messages.append(json.loads(text))


def _event_names(messages: list[dict[str, object]]) -> list[str]:
    return [str(message["event"]) for message in messages]


def test_ws_analyze_log_emits_minimal_event_sequence(monkeypatch):
    module = _load_log_analyzer()
    ws = _FakeWebSocket({"context": "黑屏了"})

    monkeypatch.setattr(module, "_read_log", lambda: ("line1\nline2", True))
    monkeypatch.setattr(module, "_build_prompt", lambda extra_context: f"prompt:{extra_context}")
    monkeypatch.setattr(module, "_get_system_prompt", lambda: "system-prompt")
    monkeypatch.setattr(module, "get_config", lambda: {"llm": {"provider": "fake"}})
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    async def fake_stream_analysis(system_prompt, prompt, llm_cfg, send_chunk):
        assert system_prompt == "system-prompt"
        assert prompt == "prompt:黑屏了"
        assert llm_cfg == {"provider": "fake"}
        await send_chunk("chunk-1")
        await send_chunk("chunk-2")
        return "full-result"

    monkeypatch.setattr(module, "stream_analysis", fake_stream_analysis)

    asyncio.run(module.ws_analyze_log(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == [
        "stage_update",
        "log_info",
        "stage_update",
        "stage_update",
        "stage_update",
        "stream",
        "stream",
        "stage_update",
        "done",
    ]
    assert [message["stage"] for message in ws.messages if message["event"] == "stage_update"] == [
        "reading_input",
        "preparing_prompt",
        "ai_running",
        "ai_streaming",
        "done",
    ]
    assert [message["chunk"] for message in ws.messages if message["event"] == "stream"] == ["chunk-1", "chunk-2"]
    assert ws.messages[1] == {"event": "log_info", "lines": 2}
    assert ws.messages[-1] == {"event": "done", "full": "full-result"}


def test_ws_analyze_log_emits_error_when_log_missing(monkeypatch):
    module = _load_log_analyzer()
    ws = _FakeWebSocket({"context": ""})

    monkeypatch.setattr(module, "_read_log", lambda: ("", False))
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    asyncio.run(module.ws_analyze_log(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == ["stage_update", "error"]
    assert ws.messages[0]["stage"] == "reading_input"
    assert ws.messages[1] == {
        "event": "error",
        "code": "log_file_missing",
        "message": f"游戏日志文件不存在：{module._LOG_PATH}\n请确认游戏已运行过至少一次。",
        "detail": f"游戏日志文件不存在：{module._LOG_PATH}\n请确认游戏已运行过至少一次。",
    }


def test_ws_analyze_log_falls_back_to_error_when_stream_analysis_raises(monkeypatch):
    module = _load_log_analyzer()
    ws = _FakeWebSocket({"context": "黑屏了"})

    monkeypatch.setattr(module, "_read_log", lambda: ("line1\nline2", True))
    monkeypatch.setattr(module, "_build_prompt", lambda extra_context: f"prompt:{extra_context}")
    monkeypatch.setattr(module, "_get_system_prompt", lambda: "system-prompt")
    monkeypatch.setattr(module, "get_config", lambda: {"llm": {"provider": "fake"}})
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    async def fake_stream_analysis(system_prompt, prompt, llm_cfg, send_chunk):
        assert system_prompt == "system-prompt"
        assert prompt == "prompt:黑屏了"
        assert llm_cfg == {"provider": "fake"}
        raise RuntimeError("boom-log")

    monkeypatch.setattr(module, "stream_analysis", fake_stream_analysis)

    asyncio.run(module.ws_analyze_log(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == [
        "stage_update",
        "log_info",
        "stage_update",
        "stage_update",
        "error",
    ]
    assert [message["stage"] for message in ws.messages if message["event"] == "stage_update"] == [
        "reading_input",
        "preparing_prompt",
        "ai_running",
    ]
    assert ws.messages[1] == {"event": "log_info", "lines": 2}
    assert ws.messages[-1] == {"event": "error", "message": "boom-log"}


def test_ws_analyze_log_preserves_stream_events_before_error(monkeypatch):
    module = _load_log_analyzer()
    ws = _FakeWebSocket({"context": "黑屏了"})

    monkeypatch.setattr(module, "_read_log", lambda: ("line1\nline2", True))
    monkeypatch.setattr(module, "_build_prompt", lambda extra_context: f"prompt:{extra_context}")
    monkeypatch.setattr(module, "_get_system_prompt", lambda: "system-prompt")
    monkeypatch.setattr(module, "get_config", lambda: {"llm": {"provider": "fake"}})
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    async def fake_stream_analysis(system_prompt, prompt, llm_cfg, send_chunk):
        assert system_prompt == "system-prompt"
        assert prompt == "prompt:黑屏了"
        assert llm_cfg == {"provider": "fake"}
        await send_chunk("partial-log")
        raise RuntimeError("boom-after-log-stream")

    monkeypatch.setattr(module, "stream_analysis", fake_stream_analysis)

    asyncio.run(module.ws_analyze_log(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == [
        "stage_update",
        "log_info",
        "stage_update",
        "stage_update",
        "stage_update",
        "stream",
        "error",
    ]
    assert [message["stage"] for message in ws.messages if message["event"] == "stage_update"] == [
        "reading_input",
        "preparing_prompt",
        "ai_running",
        "ai_streaming",
    ]
    assert [message["chunk"] for message in ws.messages if message["event"] == "stream"] == ["partial-log"]
    assert ws.messages[-1] == {"event": "error", "message": "boom-after-log-stream"}
