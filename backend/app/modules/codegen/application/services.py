from __future__ import annotations

from app.modules.codegen.domain.models import AssetCodegenRequest, AssetGroupRequest, CustomCodegenRequest, ModProjectRequest


class CodegenService:
    def __init__(self, prompt_assembler, agent_runner, artifact_writer=None, build_trigger=None) -> None:
        self.prompt_assembler = prompt_assembler
        self.agent_runner = agent_runner
        self.artifact_writer = artifact_writer
        self.build_trigger = build_trigger

    async def create_asset(self, request: AssetCodegenRequest, stream_callback=None) -> str:
        prompt = self.prompt_assembler.assemble_asset_prompt(request)
        return await self.agent_runner(prompt, request.project_root, stream_callback)

    async def create_custom_code(self, request: CustomCodegenRequest, stream_callback=None) -> str:
        prompt = self.prompt_assembler.assemble_custom_code_prompt(request)
        return await self.agent_runner(prompt, request.project_root, stream_callback)

    async def create_asset_group(self, request: AssetGroupRequest, stream_callback=None) -> str:
        prompt = self.prompt_assembler.assemble_asset_group_prompt(request)
        return await self.agent_runner(prompt, request.project_root, stream_callback)

    async def build_and_fix(self, project_root, max_attempts: int = 3, stream_callback=None) -> tuple[bool, str]:
        prompt = self.prompt_assembler.assemble_build_prompt(max_attempts)
        return await self.build_trigger.trigger(prompt, project_root, stream_callback)

    async def create_mod_project(self, request: ModProjectRequest, stream_callback=None):
        prompt = self.prompt_assembler.assemble_create_mod_project_prompt(request)
        await self.agent_runner(prompt, request.target_dir, stream_callback)
        return request.target_dir / request.project_name

    async def package_mod(self, project_root, stream_callback=None) -> bool:
        prompt = self.prompt_assembler.assemble_package_prompt()
        output = await self.agent_runner(prompt, project_root, stream_callback)
        return "Build succeeded" in output or "0 Error(s)" in output
