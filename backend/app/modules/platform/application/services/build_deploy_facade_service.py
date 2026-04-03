from __future__ import annotations

import json
import shutil
from pathlib import Path

from agents.code_agent import build_and_fix
from app.shared.prompting import PromptLoader
from config import get_config


class BuildDeployFacadeService:
    def __init__(self) -> None:
        self._text_loader = PromptLoader()
        self._skip_dirs = {"obj", "ref", ".godot"}

    def _find_output_files(self, project_root: Path) -> list[Path]:
        results: dict[str, Path] = {}
        bin_dir = project_root / "bin"
        if not bin_dir.exists():
            return []
        for suffix in (".dll", ".pck"):
            candidates = [
                file for file in bin_dir.rglob(f"*{suffix}")
                if not any(part in self._skip_dirs for part in file.relative_to(bin_dir).parts)
            ]
            if candidates:
                results[suffix] = max(candidates, key=lambda file: file.stat().st_mtime)
        return list(results.values())

    async def handle_ws_build_deploy(self, ws) -> None:
        await ws.accept()
        try:
            raw = await ws.receive_text()
            params = json.loads(raw)
            project_root = Path(params["project_root"])

            if not project_root.exists():
                await ws.send_text(json.dumps({
                    "event": "error",
                    "message": self._text_loader.render(
                        "runtime_workflow.build_project_root_missing",
                        {"project_root": project_root},
                    ),
                }))
                return

            cfg = get_config()
            sts2_path_str = cfg.get("sts2_path", "")
            sts2_mods = Path(sts2_path_str) / "Mods" if sts2_path_str else None

            async def send_chunk(chunk: str):
                await ws.send_text(json.dumps({"event": "stream", "chunk": chunk}))

            await send_chunk(f"{self._text_loader.load('runtime_workflow.build_agent_build_start').strip()}\n")
            success, _ = await build_and_fix(project_root, stream_callback=send_chunk)

            if not success:
                await ws.send_text(json.dumps({
                    "event": "error",
                    "message": self._text_loader.load("runtime_workflow.build_build_failed").strip(),
                }))
                return

            await send_chunk(f"\n{self._text_loader.load('runtime_workflow.build_build_succeeded').strip()}\n")

            mod_name = project_root.name
            deployed_to: str | None = None
            file_names: list[str] = []

            if sts2_mods and sts2_mods.exists():
                target_dir = sts2_mods / mod_name
                if target_dir.exists():
                    existing = [file for file in target_dir.iterdir() if file.suffix in (".dll", ".pck")]
                    if existing:
                        file_names = [file.name for file in existing]
                        deployed_to = str(target_dir)
                        await send_chunk(
                            f"\n{self._text_loader.render('runtime_workflow.build_deployed_via_local_props', {'target_dir': target_dir}).strip()}\n"
                        )
                        for file in existing:
                            await send_chunk(
                                f"{self._text_loader.render('runtime_workflow.build_file_item', {'file_name': file.name})}\n"
                            )

                if not deployed_to:
                    output_files = self._find_output_files(project_root)
                    if output_files:
                        target_dir.mkdir(parents=True, exist_ok=True)
                        await send_chunk(
                            f"\n{self._text_loader.render('runtime_workflow.build_copying_to_target', {'target_dir': target_dir}).strip()}\n"
                        )
                        for file in output_files:
                            shutil.copy2(file, target_dir / file.name)
                            await send_chunk(
                                f"{self._text_loader.render('runtime_workflow.build_file_item', {'file_name': file.name})}\n"
                            )
                        file_names = [file.name for file in output_files]
                        deployed_to = str(target_dir)
                        await send_chunk(f"\n{self._text_loader.load('runtime_workflow.build_deploy_finished').strip()}\n")
                    else:
                        await send_chunk(f"\n{self._text_loader.load('runtime_workflow.build_output_missing').strip()}\n")
            elif not sts2_mods:
                await send_chunk(f"\n{self._text_loader.load('runtime_workflow.build_game_path_missing').strip()}\n")

            await ws.send_text(json.dumps({
                "event": "done",
                "success": True,
                "deployed_to": deployed_to,
                "files": file_names,
            }))
        except Exception as exc:
            try:
                await ws.send_text(json.dumps({"event": "error", "message": str(exc)}))
            except Exception:
                pass
