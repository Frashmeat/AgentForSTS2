import asyncio
import importlib
import json
import shutil
import sys
import types
import uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))


class _DummyRouter:
    def websocket(self, _path):
        def decorator(func):
            return func

        return decorator


def _load_mod_analyzer():
    sys.modules["fastapi"] = types.SimpleNamespace(APIRouter=lambda: _DummyRouter(), WebSocket=object)
    sys.modules["config"] = types.SimpleNamespace(get_config=lambda: {"llm": {}})
    sys.modules["llm.stream"] = types.SimpleNamespace(stream_analysis=lambda *args, **kwargs: None)
    sys.modules["llm.stage_events"] = types.SimpleNamespace(build_stage_event=lambda *args, **kwargs: None)
    sys.modules.pop("routers.mod_analyzer", None)
    return importlib.import_module("routers.mod_analyzer")


def _make_temp_root() -> Path:
    root = Path(__file__).parent / ".tmp" / f"mod-analyzer-{uuid.uuid4().hex}"
    root.mkdir(parents=True)
    return root


def _normalized(value: str) -> str:
    return value.replace("\\", "/")


def test_scan_mod_files_prioritizes_gameplay_sources_and_localization():
    module = _load_mod_analyzer()
    root = _make_temp_root()
    project_root = root / "MyMod"
    cards_dir = project_root / "Cards"
    relics_dir = project_root / "Relics"
    localization_dir = project_root / "localization"
    misc_dir = project_root / "Utils"
    cards_dir.mkdir(parents=True)
    relics_dir.mkdir(parents=True)
    localization_dir.mkdir(parents=True)
    misc_dir.mkdir(parents=True)

    try:
        (cards_dir / "Strike.cs").write_text("class Strike {}", encoding="utf-8")
        (relics_dir / "EmberRelic.cs").write_text("class EmberRelic {}", encoding="utf-8")
        (misc_dir / "Helper.cs").write_text("class Helper {}", encoding="utf-8")
        (localization_dir / "cards.json").write_text('{"Strike_NAME":"Strike"}', encoding="utf-8")

        content, file_count = module._scan_mod_files(project_root)
        normalized = _normalized(content)

        assert file_count == 4
        assert "// Cards/Strike.cs" in normalized
        assert "// Relics/EmberRelic.cs" in normalized
        assert "// Utils/Helper.cs" in normalized
        assert "// Localization: localization/cards.json" in normalized
        assert normalized.index("// Cards/Strike.cs") < normalized.index("// Utils/Helper.cs")
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_scan_mod_files_skips_ignored_directories_and_marks_truncation():
    module = _load_mod_analyzer()
    root = _make_temp_root()
    project_root = root / "ScanMod"
    cards_dir = project_root / "Cards"
    skipped_dir = project_root / "bin"
    cards_dir.mkdir(parents=True)
    skipped_dir.mkdir(parents=True)

    try:
        (cards_dir / "LongCard.cs").write_text("A" * 4505, encoding="utf-8")
        (skipped_dir / "Ignored.cs").write_text("class Ignored {}", encoding="utf-8")

        content, file_count = module._scan_mod_files(project_root)
        normalized = _normalized(content)

        assert file_count == 1
        assert "Ignored.cs" not in normalized
        assert "// Cards/LongCard.cs" in normalized
        assert "// ... (truncated)" in normalized
    finally:
        shutil.rmtree(root, ignore_errors=True)


def test_mod_analyzer_prompt_templates_exist_for_real_loader():
    module = _load_mod_analyzer()

    system_template = module._PROMPT_LOADER.load("analyzer.mod_analyzer_system")
    user_template = module._PROMPT_LOADER.load("analyzer.mod_analyzer_user")

    assert "Slay the Spire 2 mod 开发专家" in system_template
    assert "{{ project_root }}" in user_template
    assert "{{ file_content }}" in user_template
    assert "请分析这个 mod 的内容" in user_template


def test_get_system_prompt_uses_prompt_loader():
    module = _load_mod_analyzer()

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
    assert loader.load_calls == [("analyzer.mod_analyzer_system", "")]


def test_build_prompt_uses_prompt_loader_for_user_template():
    module = _load_mod_analyzer()

    class FakePromptLoader:
        def __init__(self) -> None:
            self.render_calls: list[tuple[str, dict[str, object], str]] = []

        def render(self, template_name: str, variables: dict[str, object], *, fallback_template: str | None = None) -> str:
            self.render_calls.append((template_name, variables, fallback_template or ""))
            return f"rendered:{template_name}"

    loader = FakePromptLoader()
    module._PROMPT_LOADER = loader

    prompt = module._build_prompt(Path("E:/STS2mod/MyMod"), "// Cards/Strike.cs\nclass Strike {}")

    assert prompt == "rendered:analyzer.mod_analyzer_user"
    assert loader.render_calls == [
        (
            "analyzer.mod_analyzer_user",
            {
                "project_root": Path("E:/STS2mod/MyMod"),
                "file_content": "// Cards/Strike.cs\nclass Strike {}",
            },
            "",
        )
    ]


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


def test_ws_analyze_mod_emits_minimal_event_sequence(monkeypatch):
    module = _load_mod_analyzer()
    project_root = Path("E:/STS2mod/MyMod")
    ws = _FakeWebSocket({"project_root": str(project_root)})

    monkeypatch.setattr(Path, "exists", lambda self: self == project_root)
    monkeypatch.setattr(module, "_scan_mod_files", lambda root: ("// Cards/Strike.cs", 1))
    monkeypatch.setattr(module, "_build_prompt", lambda root, content: f"prompt:{root}:{content}")
    monkeypatch.setattr(module, "_get_system_prompt", lambda: "system-prompt")
    monkeypatch.setattr(module, "get_config", lambda: {"llm": {"provider": "fake"}})
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    async def fake_stream_analysis(system_prompt, prompt, llm_cfg, send_chunk):
        assert system_prompt == "system-prompt"
        assert prompt == f"prompt:{project_root}:// Cards/Strike.cs"
        assert llm_cfg == {"provider": "fake"}
        await send_chunk("chunk-1")
        await send_chunk("chunk-2")
        return "full-result"

    monkeypatch.setattr(module, "stream_analysis", fake_stream_analysis)

    asyncio.run(module.ws_analyze_mod(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == [
        "stage_update",
        "scan_info",
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
    assert ws.messages[1] == {"event": "scan_info", "files": 1}
    assert ws.messages[-1] == {"event": "done", "full": "full-result"}


def test_ws_analyze_mod_emits_error_when_scan_finds_no_sources(monkeypatch):
    module = _load_mod_analyzer()
    project_root = Path("E:/STS2mod/EmptyMod")
    ws = _FakeWebSocket({"project_root": str(project_root)})

    monkeypatch.setattr(Path, "exists", lambda self: self == project_root)
    monkeypatch.setattr(module, "_scan_mod_files", lambda root: ("", 0))
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    asyncio.run(module.ws_analyze_mod(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == ["stage_update", "error"]
    assert ws.messages[0]["stage"] == "reading_input"
    assert ws.messages[1] == {"event": "error", "message": f"未在 {project_root} 找到任何 .cs 源码文件"}


def test_ws_analyze_mod_emits_reading_stage_before_missing_path_error(monkeypatch):
    module = _load_mod_analyzer()
    project_root = Path("E:/STS2mod/MissingMod")
    ws = _FakeWebSocket({"project_root": str(project_root)})

    monkeypatch.setattr(Path, "exists", lambda self: False)
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    asyncio.run(module.ws_analyze_mod(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == ["stage_update", "error"]
    assert ws.messages[0]["stage"] == "reading_input"
    assert ws.messages[1] == {"event": "error", "message": f"路径不存在：{project_root}"}


def test_ws_analyze_mod_falls_back_to_error_when_stream_analysis_raises(monkeypatch):
    module = _load_mod_analyzer()
    project_root = Path("E:/STS2mod/MyMod")
    ws = _FakeWebSocket({"project_root": str(project_root)})

    monkeypatch.setattr(Path, "exists", lambda self: self == project_root)
    monkeypatch.setattr(module, "_scan_mod_files", lambda root: ("// Cards/Strike.cs", 1))
    monkeypatch.setattr(module, "_build_prompt", lambda root, content: f"prompt:{root}:{content}")
    monkeypatch.setattr(module, "_get_system_prompt", lambda: "system-prompt")
    monkeypatch.setattr(module, "get_config", lambda: {"llm": {"provider": "fake"}})
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    async def fake_stream_analysis(system_prompt, prompt, llm_cfg, send_chunk):
        assert system_prompt == "system-prompt"
        assert prompt == f"prompt:{project_root}:// Cards/Strike.cs"
        assert llm_cfg == {"provider": "fake"}
        raise RuntimeError("boom-mod")

    monkeypatch.setattr(module, "stream_analysis", fake_stream_analysis)

    asyncio.run(module.ws_analyze_mod(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == [
        "stage_update",
        "scan_info",
        "stage_update",
        "stage_update",
        "error",
    ]
    assert [message["stage"] for message in ws.messages if message["event"] == "stage_update"] == [
        "reading_input",
        "preparing_prompt",
        "ai_running",
    ]
    assert ws.messages[1] == {"event": "scan_info", "files": 1}
    assert ws.messages[-1] == {"event": "error", "message": "boom-mod"}


def test_ws_analyze_mod_preserves_stream_events_before_error(monkeypatch):
    module = _load_mod_analyzer()
    project_root = Path("E:/STS2mod/MyMod")
    ws = _FakeWebSocket({"project_root": str(project_root)})

    monkeypatch.setattr(Path, "exists", lambda self: self == project_root)
    monkeypatch.setattr(module, "_scan_mod_files", lambda root: ("// Cards/Strike.cs", 1))
    monkeypatch.setattr(module, "_build_prompt", lambda root, content: f"prompt:{root}:{content}")
    monkeypatch.setattr(module, "_get_system_prompt", lambda: "system-prompt")
    monkeypatch.setattr(module, "get_config", lambda: {"llm": {"provider": "fake"}})
    monkeypatch.setattr(
        module,
        "build_stage_event",
        lambda scope, stage, message: {"scope": scope, "stage": stage, "message": message},
    )

    async def fake_stream_analysis(system_prompt, prompt, llm_cfg, send_chunk):
        assert system_prompt == "system-prompt"
        assert prompt == f"prompt:{project_root}:// Cards/Strike.cs"
        assert llm_cfg == {"provider": "fake"}
        await send_chunk("partial-mod")
        raise RuntimeError("boom-after-mod-stream")

    monkeypatch.setattr(module, "stream_analysis", fake_stream_analysis)

    asyncio.run(module.ws_analyze_mod(ws))

    assert ws.accepted is True
    assert _event_names(ws.messages) == [
        "stage_update",
        "scan_info",
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
    assert [message["chunk"] for message in ws.messages if message["event"] == "stream"] == ["partial-mod"]
    assert ws.messages[-1] == {"event": "error", "message": "boom-after-mod-stream"}
