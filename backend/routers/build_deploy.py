"""
Build & Deploy：通过 Code Agent 运行 dotnet publish（含 Godot .pck 导出），
成功后把产物复制到 STS2 Mods 文件夹。

为什么用 Code Agent 而不是直接 subprocess：
- dotnet publish 只产出 .dll
- .pck 需要 Godot headless export，Code Agent 的 prompt 里有完整流程
"""
from __future__ import annotations

import json
import shutil
from pathlib import Path

from fastapi import APIRouter, WebSocket

from app.modules.codegen.api import build_and_fix
from config import get_config

router = APIRouter()

_SKIP_DIRS = {"obj", "ref", ".godot"}


def _find_output_files(project_root: Path) -> list[Path]:
    """在 bin/ 下找最新的 .dll 和 .pck（跳过 obj/ref 中间产物）。"""
    results: dict[str, Path] = {}
    bin_dir = project_root / "bin"
    if not bin_dir.exists():
        return []
    for suffix in (".dll", ".pck"):
        candidates = [
            f for f in bin_dir.rglob(f"*{suffix}")
            if not any(p in _SKIP_DIRS for p in f.relative_to(bin_dir).parts)
        ]
        if candidates:
            results[suffix] = max(candidates, key=lambda f: f.stat().st_mtime)
    return list(results.values())


@router.websocket("/ws/build-deploy")
async def ws_build_deploy(ws: WebSocket):
    """
    WebSocket：Code Agent 构建 mod，成功后复制产物到 STS2 Mods 文件夹。

    客户端发送：{ "project_root": "E:/STS2mod/MyMod" }

    服务端推流：
    - stream:  { chunk }
    - done:    { success, deployed_to, files }
    - error:   { message }
    """
    await ws.accept()
    try:
        raw = await ws.receive_text()
        params = json.loads(raw)
        project_root = Path(params["project_root"])

        if not project_root.exists():
            await ws.send_text(json.dumps({
                "event": "error",
                "message": f"路径不存在：{project_root}"
            }))
            return

        cfg = get_config()
        sts2_path_str = cfg.get("sts2_path", "")
        sts2_mods = Path(sts2_path_str) / "Mods" if sts2_path_str else None

        async def send_chunk(chunk: str):
            await ws.send_text(json.dumps({"event": "stream", "chunk": chunk}))

        # ── Step 1: Code Agent 构建（dotnet publish + Godot .pck export）──
        await send_chunk("▶ Code Agent 开始构建...\n")
        success, _ = await build_and_fix(project_root, stream_callback=send_chunk)

        if not success:
            await ws.send_text(json.dumps({
                "event": "error",
                "message": "构建失败，请查看上方 Agent 输出"
            }))
            return

        await send_chunk("\n✓ 构建成功！\n")

        mod_name = project_root.name
        deployed_to: str | None = None
        file_names: list[str] = []

        if sts2_mods and sts2_mods.exists():
            target_dir = sts2_mods / mod_name
            # local.props 配置了 GameDir 时，dotnet publish 会直接输出到 Mods 文件夹
            # 检查是否已经自动部署过去了
            if target_dir.exists():
                existing = [f for f in target_dir.iterdir() if f.suffix in (".dll", ".pck")]
                if existing:
                    file_names = [f.name for f in existing]
                    deployed_to = str(target_dir)
                    await send_chunk(f"\n✓ 已通过 local.props 部署到 {target_dir}\n")
                    for f in existing:
                        await send_chunk(f"  ✓ {f.name}\n")

            # 如果 Mods 里没有，尝试从 bin/ 复制
            if not deployed_to:
                output_files = _find_output_files(project_root)
                if output_files:
                    target_dir.mkdir(parents=True, exist_ok=True)
                    await send_chunk(f"\n▶ 复制到 {target_dir}...\n")
                    for f in output_files:
                        shutil.copy2(f, target_dir / f.name)
                        await send_chunk(f"  ✓ {f.name}\n")
                    file_names = [f.name for f in output_files]
                    deployed_to = str(target_dir)
                    await send_chunk("\n✓ 部署完成！\n")
                else:
                    await send_chunk("\n⚠ 未找到构建产物（bin/ 和 Mods 均无 .dll/.pck）\n")
        elif not sts2_mods:
            await send_chunk("\n⚠ 未配置 STS2 游戏路径，跳过部署（可在设置中配置）\n")

        await ws.send_text(json.dumps({
            "event": "done",
            "success": True,
            "deployed_to": deployed_to,
            "files": file_names,
        }))

    except Exception as e:
        try:
            await ws.send_text(json.dumps({"event": "error", "message": str(e)}))
        except Exception:
            pass
