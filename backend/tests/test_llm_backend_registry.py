import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from app.shared.infra.llm.text_backend import FunctionTextBackend, TextBackendRegistry
from llm.agent_runner import AgentRunner


class FakeAgentBackend:
    async def plan(self, prompt: str, project_root: Path, llm_cfg: dict, stream_callback=None) -> str:
        return f"plan:{prompt}"

    async def stream(self, prompt: str, project_root: Path, llm_cfg: dict, stream_callback=None) -> str:
        return f"stream:{prompt}"


async def _fake_complete(prompt: str, llm_cfg: dict, cwd: Path | None = None) -> str:
    return f"complete:{prompt}"


async def _fake_stream(system_prompt: str, user_prompt: str, llm_cfg: dict, on_chunk, cwd: Path | None = None) -> str:
    return f"{system_prompt}:{user_prompt}"


def test_agent_runner_uses_backend_port_instead_of_concrete_cli():
    backend = FakeAgentBackend()
    runner = AgentRunner(backend=backend)

    assert runner.backend is backend


def test_text_backend_registry_resolves_named_backend():
    registry = TextBackendRegistry()
    backend = FunctionTextBackend(complete_fn=_fake_complete, stream_fn=_fake_stream)

    registry.register("mock", backend)

    assert registry.get("mock") is backend
