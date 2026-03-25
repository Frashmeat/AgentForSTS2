from __future__ import annotations


class BuildTrigger:
    def __init__(self, agent_runner) -> None:
        self.agent_runner = agent_runner

    async def trigger(self, prompt: str, project_root, stream_callback=None) -> tuple[bool, str]:
        output = await self.agent_runner(prompt, project_root, stream_callback)
        success = "Build succeeded" in output or "0 Error(s)" in output or "publish succeeded" in output.lower()
        return success, output
