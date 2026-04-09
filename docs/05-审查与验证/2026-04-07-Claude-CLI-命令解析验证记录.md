# 2026-04-07 Claude CLI 命令解析验证记录

## 1. 目标

验证“PowerShell 中 `Get-Command claude` 成功”并不能直接证明“工作站后端里的 Python `subprocess.Popen(['claude'])` 一定能成功启动”。

## 2. 本次范围

- `backend/llm/agent_backends/claude_cli.py`
- `backend/tests/test_claude_cli_resolution.py`

## 3. 验证点

- `Claude CLI` backend 当前确实直接使用字面量命令 `claude`
- 在 Windows 上，仅存在 `claude.ps1` 时：
  - `pwsh -NoProfile -Command "Get-Command claude"` 可以解析成功
  - Python `subprocess.run(["claude", "--version"])` 仍会抛 `FileNotFoundError`

## 4. 定向验证

执行：

```powershell
python -m pytest backend/tests/test_claude_cli_resolution.py -q
```

## 5. 修复策略

- 非 Windows：继续直接解析 `claude`
- Windows：优先解析 `claude.cmd` / `claude.exe` / `claude.bat`
- 若 Windows 上只解析到 `claude.ps1`，则改为通过 `pwsh -NoProfile -File <claude.ps1>` 启动

## 6. 结论

- 当前报错不能仅凭“终端里能敲 `claude`”排除命令解析问题
- 还需要区分 PowerShell 命令解析结果与 Python 子进程的可执行文件查找结果
- 修复后，工作站后端不再把 Windows 上的 `claude.ps1` 误当成可直接由 `subprocess.Popen(["claude"])` 启动的可执行文件

## 7. 2026-04-10 补充：鉴权失败分类

- 当前 `Claude CLI` 若因令牌无效返回 `401`，工作流路由会把错误分类为 `workflow_api_key_invalid`
- 若返回 `403`，工作流路由会把错误分类为 `workflow_api_key_forbidden`
- WebSocket 仍通过统一 `error` 事件返回，但 `code` 不再一律退化成 `workflow_runtime_error`
- 路由日志文案同步改为“工作流执行失败”，避免把已处理的鉴权失败误记为“未捕获异常”

## 8. 2026-04-10 补充：Claude CLI 认证环境变量

- 现场复核发现：用户手动可用的 Claude CLI 配置使用的是 `ANTHROPIC_AUTH_TOKEN`，而不是仅 `ANTHROPIC_API_KEY`
- 工作站后端原先只向 `Claude CLI` 子进程注入 `ANTHROPIC_API_KEY`，导致代理网关场景下可能出现手动 CLI 可用、后端 CLI 返回 `401` 的分叉现象
- 本轮修复后：
  - `agent_backends/claude_cli.py` 会优先注入 `ANTHROPIC_AUTH_TOKEN`
  - 同时保留 `ANTHROPIC_API_KEY` 作为兼容变量
  - `llm/text_runner.py` 走 `claude_cli` 文本补全时也会注入同样的认证环境变量与 `ANTHROPIC_BASE_URL`
