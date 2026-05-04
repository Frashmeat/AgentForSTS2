<p align="center">
  <img src="project_image/AgentTheSpire_icon.png" width="360" alt="AgentTheSpire" />
</p>

<h3 align="center">AI-powered mod generator for Slay the Spire 2</h3>
<h3 align="center">《杀戮尖塔 2》AI mod 生成器</h3>

---

## English

Describe your card, relic, or power in plain text — AgentTheSpire generates the C# code, creates the artwork, compiles the mod, and deploys it to your game automatically.

### Features

- **AI code generation** — Claude Code CLI or Codex CLI writes complete C# implementations from your description
- **AI image generation** — FLUX.2 / 即梦 / 通义万相 generate card art and relic icons, background removed and cropped automatically
- **One-click build & deploy** — `dotnet publish` + Godot PCK packaging, copied straight to your game's mods folder
- **Batch creation** — describe a full mod theme, AI plans and generates all assets in one go
- **Hybrid execution routing** — one frontend shell now supports local BYOK flows and platform-mode task creation with auth, user center, and job detail pages

### Runtime Status

- As of April 10, 2026, only two backend roles remain active: `workstation` and `web`.
- The old integrated `full` runtime, migration flags, and workflow bridge path are retired.
- If any historical section below still mentions `full`, treat it as archived background rather than the current baseline.

### Requirements

| Tool | Version | Required |
|------|---------|----------|
| Python | 3.11+ | Yes |
| Node.js | 18+ | Yes |
| .NET SDK | 9.x | Yes |
| Godot Mono | **4.5.1 exactly** | Yes |
| ilspycmd | 9.1.0.7988 | Recommended for knowledge refresh / decompile workflows |
| Slay the Spire 2 | latest | Yes |
| Claude Code CLI / Codex CLI | latest | Optional for `agent_cli` mode |
| LLM API Key | — | One of two LLM options |
| Image Gen API Key | — | Optional |

### Quick Start

```bash
git clone https://github.com/yourname/AgentTheSpire.git
cd AgentTheSpire

powershell -ExecutionPolicy Bypass -File .\tools\tools.ps1 install   # Windows 推荐入口：安装 .NET / Godot / ilspycmd / Python deps / frontend build

# 安装脚本会把 ~\.dotnet\tools 和项目内 runtime\tools 自动加入当前会话与用户 PATH

# Copy runtime/agentthespire.config.example.json → runtime/agentthespire.config.json, fill in your API keys and game path

powershell -ExecutionPolicy Bypass -File .\tools\tools.ps1 start workstation   # Opens http://localhost:7860
```

See [TUTORIAL.md](TUTORIAL.md) for full setup and configuration guide.

### App Deployment

The current deployment line is fixed:

- Local machine: `frontend` + `local-workstation`
- Docker: `web` + `web-workstation` + `postgres`

There is only one human-edited deployment config:

```text
runtime/agentthespire.config.json
```

Create it from:

```powershell
Copy-Item .\runtime\agentthespire.config.example.json .\runtime\agentthespire.config.json
```

Recommended commands:

```powershell
powershell -File .\tools\tools.ps1 package app
powershell -File .\tools\tools.ps1 deploy app -DryRun
powershell -File .\tools\tools.ps1 deploy app
powershell -File .\tools\tools.ps1 logs app
powershell -File .\tools\tools.ps1 stop app
```

`deploy app -DryRun` generates `runtime/generated/*` and prints the topology without starting local processes or Docker.
`logs app` shows local `frontend` / `local-workstation` log files and Docker `postgres` / `web-workstation` / `web` logs.

### Backend Runtime Modes

- `powershell -File .\tools\tools.ps1 start workstation`
  Starts `workstation-backend` only for local workstation workflows.
- `powershell -File .\tools\tools.ps1 start web`
  Starts `web-backend` only for platform/auth/job/quota APIs.
- `powershell -File .\tools\tools.ps1 deploy app`
  Starts the final local + Docker topology from `runtime/agentthespire.config.json`: local `frontend + local-workstation`, plus Docker `postgres + web-workstation + web`. Deployment fails if any Docker service is missing or not running after compose up.
  After compose up, deployment runs Alembic and reconciles the default admin account to `admin / admin@example.com / admin123456`.

Current product behavior:

- The workstation shell remains the only frontend entry.
- `/auth/*`, `/me`, and `/me/jobs/:jobId` are now available in the same shell.
- Choosing server mode creates a draft platform job first, then asks for start confirmation before queueing.
- Local BYOK execution does not create platform history.

### Tool Scripts

- Core install/start/dev helpers live in `tools/`.
- `tools/stop/kill-local.ps1` stops local `frontend` / `workstation` / `web` processes by discovered state/config first, with port arguments available as explicit overrides; it also clears common release-resident processes whose command line, executable path, or current directory points to this repository's `tools/latest/artifacts`; Docker Desktop proxy processes on the `web` port are explicitly skipped.
- `tools/latest/` contains the current packaging and Docker deployment scripts.
- `tools/archive/` stores deprecated historical scripts. The old Windows Sandbox verification chain has been moved there and is no longer part of the primary workflow.
- `tools/latest/artifacts/` and generated `sandbox_test.wsb` files are local outputs and are ignored by Git.

### LLM Options

| Mode | Backend / Route |
|------|-----------------|
| `agent_cli` | `claude` or `codex` |
| `claude_api` | Claude-compatible API (`model + api_key + base_url`) |

### Image Generation Options

| Provider | Notes |
|----------|-------|
| `bfl` | FLUX.2 via Black Forest Labs API |
| `fal` | FLUX.2 via FAL.ai |
| `jimeng` | 即梦 via Volcengine — requires Access Key + Secret Key |
| `wanxiang` | 通义万相 via Aliyun |

---

## 中文

用自然语言描述你的卡牌、遗物、能力效果，AgentTheSpire 自动生成 C# 代码、生成配图、编译并部署到游戏。

### 功能

- **AI 写代码** — Claude Code CLI 或 Codex CLI 根据描述生成完整 C# 实现，编译报错自动修复
- **AI 生图** — FLUX.2 / 即梦 / 通义万相生成卡图/遗物图标，自动去背景裁剪
- **一键编译部署** — `dotnet publish` + Godot PCK 打包，自动复制到游戏 mods 目录
- **批量创建** — 描述整个 mod 主题，AI 先规划，再经过 Item 复核与执行策略决策两道确认后批量生成所有素材
- **同一入口 + 用户中心** — 同一前端入口同时承接本地工作站链路与平台链路，已补登录/注册/用户中心/任务详情页
- **统一执行分流** — 点击生成后可在“本机执行 / 服务器模式”之间分流；服务器模式先创建平台任务草稿，再确认开始

### 当前运行口径

- 截至 2026-04-10，后端只保留两个有效角色：`workstation` 与 `web`。
- 历史 `full` 一体化运行时、migration flags、workflow 过渡桥接路径已收口，不再作为当前实现基线。
- 若下文个别历史段落仍提到 `full`，请按“归档背景信息”理解，不再视为当前推荐方案。

### 知识库版本检查

- 工作站启动后会检查本地知识库状态，并区分 `fresh / stale / missing`
- 游戏知识库版本来源于当前自动检测到的 `sts2_path` 对应 Steam 安装版本文本
- Baselib 知识库版本来源于官方 latest release：
  <https://github.com/Alchyr/BaseLib-StS2/releases>
- 设置页现在提供：
  - `检查更新`
  - `更新知识库`
  - `查看知识库说明`
  - 工作区设置与服务器模式默认配置会在修改后自动保存，并通过右上角非阻塞提示显示状态；当前不可用的服务器执行配置只展示状态，不允许选为默认值
- 工作流头部右上角展示紧凑“知识库”标签，只保留状态、游戏版本和 Baselib 版本；点击标签可查看知识库说明，不再用风险提醒弹窗阻断本地执行。
- 发行包只初始化可查看、可编辑的运行时知识目录；不会再复制旧 seed 文件。应用运行时只读取这份目录，用户修改后会直接生效。
- `workstation` / `hybrid` 发行包也会直接包含当前实例自己的 `runtime/tools/`，用于承载 `ilspycmd` 等知识库更新工具及其完整依赖目录。
- 运行时知识目录默认位于：
  - `runtime/knowledge/knowledge-manifest.json`
  - `runtime/knowledge/game/`
  - `runtime/knowledge/baselib/`
  - `runtime/knowledge/resources/sts2/`
  - `runtime/knowledge/cache/`
- 知识库更新所需的 `ilspycmd` 会优先从当前运行实例自己的 `runtime/tools/` 查找；无论是仓库直启还是 release 包运行，都不应再假定只从仓库根目录查工具。
- 游戏反编译必须使用 `ilspycmd --project --outputdir` 生成源码树；旧式单文件 `sts2.decompiled.cs` 不算完整游戏知识库。
- 平台知识库包是完整 `runtime/knowledge` 迁移包，必须同时包含 `game/**/*.cs` 游戏反编译源码、`baselib/BaseLib.decompiled.cs` 和必需的 `resources/sts2/*.md` 规则文档；任一部分缺失时，导出、上传或激活都会被拒绝。执行 Workstation “更新知识库”会把规则文档模板写入当前 runtime。Docker Workstation 场景下请先确保容器能访问 STS2 安装目录并成功执行“更新知识库”，再从本机工作站上传知识库包。
- Web 管理端可上传、激活、回滚和删除知识库包；删除当前激活包时会优先回退到上一个仍存在的知识库包，否则清空激活状态。
- Web 管理端激活知识库包时，会把包内容安装到当前 Web 实例自己的 `runtime/knowledge/`；上传包仓库 `runtime/knowledge/packs/` 只保留包文件与元数据，不作为运行时第二真源。
- 仓库内旧 seed 已删除，不再保留 `backend/agents/sts2_api_reference.md`、`backend/agents/baselib_src/` 或 `backend/app/modules/knowledge/resources/sts2/` 作为运行时来源；没有完整反编译结果时状态应为 `missing`。

### 快速开始

```bash
git clone https://github.com/yourname/AgentTheSpire.git
cd AgentTheSpire

powershell -ExecutionPolicy Bypass -File .\tools\tools.ps1 install   # Windows 推荐入口：安装 .NET / Godot / ilspycmd / Python 依赖 / 前端构建

# 安装脚本会把 ~\.dotnet\tools 和项目内 runtime\tools 自动加入当前会话与用户 PATH

# 如果只想安装 .NET 9 + Godot 4.5.1 + ilspycmd：
powershell -ExecutionPolicy Bypass -File .\tools\tools.ps1 install mod

# 复制 runtime/agentthespire.config.example.json → runtime/agentthespire.config.json，填入 API Key 和游戏路径
# 设置页“自动检测路径”只检查显式配置、仓库内 godot/、常见 Godot 安装目录、C/D/E:/tools 的固定文件或一级子目录，以及 PATH；不会递归扫描 LOCALAPPDATA 等用户目录。

powershell -ExecutionPolicy Bypass -File .\tools\tools.ps1 start workstation   # 打开 http://localhost:7860
```

详细配置说明见 [TUTORIAL.md](TUTORIAL.md)。

配置模板里的常用可选值：

- `llm.mode`：`agent_cli`、`claude_api`
- `llm.agent_backend`：`claude`、`codex`
- `image_gen.model`：`flux.2-pro`、`flux.2-flex`、`flux.2-klein`、`flux.2-max`、`flux.2-dev`、`flux.1.1-pro`
- `image_gen.provider`：`bfl`、`fal`、`volcengine`、`wanxiang`

### 后端运行形态

- `powershell -File .\tools\tools.ps1 start workstation`
  仅启动 `workstation-backend`，用于本机工作流、配置、知识库、构建与部署链路。
- `powershell -File .\tools\tools.ps1 start web`
  仅启动 `web-backend`，用于平台任务、认证、配额、历史记录等 API。
- `powershell -File .\tools\tools.ps1 deploy app`
  启动最终拓扑：本机 `frontend + local-workstation`，Docker `web + web-workstation + postgres`。

当前部署口径：

- 开发状态和最终部署状态使用同一拓扑。
- 人工配置唯一真实源是 `runtime/agentthespire.config.json`。
- 生成物位于 `runtime/generated/*`，可删除重建。
- 前端只连接本机 `local-workstation` 与 `web`。
- `web` 只通过 Docker 内网连接 `web-workstation`。
- 直接部署会在 Docker `web` 容器内执行 Alembic 迁移，并把默认管理员账号收敛为 `admin / admin@example.com / admin123456`；普通部署不清空业务数据，`-ResetDb` 才会删除 Docker Postgres 卷并重建。

固定拓扑如下：

| 组件 | 运行位置 | 默认地址 | 职责 |
|------|----------|----------|------|
| `frontend` | 本机进程 | `http://127.0.0.1:8080` | 静态前端入口 |
| `local-workstation` | 本机进程 | `http://127.0.0.1:7860` | 本机 STS2、Godot、Mods、CLI、本地工作流 |
| `web` | Docker | `http://127.0.0.1:7870` | 平台 API、用户、任务、配额、管理端 |
| `web-workstation` | Docker 内网 | `http://web-workstation:7860` | Web 服务器执行面 |
| `postgres` | Docker | 宿主默认 `55432` | 平台数据库 |

当前前后端边界补充：

- `workstation-backend` 继续承接本机工作流、配置、构建、部署与日志分析
- `web-backend` 默认承接 `/api/auth/*`、`/api/me/*`、平台任务与配额接口
- 用户中心只读取平台模式任务；BYOK / 本机执行不会进入服务器历史
- 服务器模式下，前端会先创建当前用户视角平台任务，再确认开始并跳转用户中心详情页
- 执行方式选择流程中的服务器模式错误、无可用服务器配置、平台任务创建失败和延后执行提示统一走右上角非阻塞状态提示，不再使用浏览器原生 `alert`

独立前端形态补充：

- 前端默认会先加载 `runtime-config.js`
- 该文件用于注入运行时地址，而不是重新构建前端
- 默认支持的键：
  - `window.__AGENT_THE_SPIRE_API_BASES__.workstation`
  - `window.__AGENT_THE_SPIRE_API_BASES__.web`
  - `window.__AGENT_THE_SPIRE_WS_BASES__.workstation`

示例：

```js
window.__AGENT_THE_SPIRE_API_BASES__ = {
  workstation: "http://127.0.0.1:7860",
  web: "https://api.example.com",
};

window.__AGENT_THE_SPIRE_WS_BASES__ = {
  workstation: "ws://127.0.0.1:7860",
};
```

App 主线命令：

```powershell
powershell -File .\tools\tools.ps1 package app
powershell -File .\tools\tools.ps1 deploy app -DryRun
powershell -File .\tools\tools.ps1 deploy app
powershell -File .\tools\tools.ps1 logs app
powershell -File .\tools\tools.ps1 stop app
```

默认文件位置：

- 源码开发：`frontend/public/runtime-config.js`
- 独立静态前端部署：站点根目录 `/runtime-config.js`
- 本地双进程 launcher：启动时会覆盖写入 `frontend/dist/runtime-config.js`

当前约束：

- 前端 runtime config 不写入 `web-workstation` 地址。
- Docker `web-workstation` 不暴露宿主端口。
- `web` 的 `platform_execution.workstation_url` 由部署脚本生成到 `http://web-workstation:7860`。
- 旧 `hybrid`、根目录双 compose、`.env.web/.env.workstation` 和 `tools/docker/*` 不再作为当前部署主线。

### 工具脚本

- 当前安装、启动、开发辅助、打包和部署脚本统一放在 `tools/`。
- 日常统一入口优先使用 `tools.ps1`。直接运行会进入五个一级菜单：环境部署、开发、Kill / 停止本机服务、打包、部署。
- 参数直达入口示例：
  - `powershell -File .\tools\tools.ps1 stop`
  - `powershell -File .\tools\tools.ps1 stop local`
  - `powershell -File .\tools\tools.ps1 stop app`
  - `powershell -File .\tools\tools.ps1 package app`
  - `powershell -File .\tools\tools.ps1 deploy app -DryRun`
  - `powershell -File .\tools\tools.ps1 test backend/tests/test_tools_entry_script.py -q`
- `tools\stop\kill-local.ps1` 位于交互菜单一级入口 `Kill / 停止本机服务`，可停止当前仓库识别出的本机 `frontend / workstation / web` 进程，并额外清理命令行、可执行路径或当前工作目录明确指向当前仓库 `tools/latest/artifacts` 的常见 release 残留进程，同时尝试停止该目录下默认 release 对应的 Docker `web` 服务；对 `7870` 上的 Docker Desktop / WSL 代理进程会显式跳过，避免误杀 Docker 后端链路。
- `tools\test\run-pytest.ps1` 是当前推荐 pytest 入口，固定通过唯一项目 Python 环境 `backend\.venv\Scripts\python.exe -m pytest` 运行，避免误用全局 Python。
- `tools/latest/` 存放当前 app 主线打包、部署与停止脚本；交互菜单中打包和部署已经拆成两个一级入口。
- `tools/archive/` 存放已归档的历史脚本；旧的 Windows Sandbox 验证链路已经迁入该目录，不再作为主流程维护。
- `tools/latest/artifacts/` 与生成出来的 `sandbox_test.wsb` 都属于本地产物，默认不会提交到 Git。

---

## What it can do / 已验证场景

<p align="center">
  <img src="project_image/Neow_fire.png" width="340" alt="Neow: 开除速度一定要快" />
</p>

| # | Asset | Description | Difficulty |
|---|-------|-------------|------------|
| S01 | Attack Card | Fixed-cost single-target damage card with upgrade | ⭐ |
| S02 | Relic | Combat-start trigger relic (e.g. gain Block) | ⭐ |
| S03 | Power | Multi-turn buff that decrements each turn and auto-removes at 0 | ⭐⭐ |
| S04 | Card (X-cost) | X-energy AoE attack scaling with energy spent | ⭐⭐ |
| S05 | Relic (counter) | Counter relic with `ShowCounter` + reward at threshold | ⭐⭐ |
| S06 | Custom mechanic | Harmony patch with no image asset | ⭐⭐ |
| S07 | Batch (2 assets) | Card + Power with dependency ordering | ⭐⭐⭐ |
| S08 | Card (end-of-turn) | Card that triggers when held in hand at turn end | ⭐⭐⭐ |
| S09 | Full mini-mod | 5-asset mod — mixed types, batch image generation | ⭐⭐⭐⭐ |
| S10 | Full mod (complex) | 4-asset pack with three-level dependency chain | ⭐⭐⭐⭐⭐ |

---

## Project Structure

```
AgentTheSpire/
├── backend/                         # Python FastAPI backend
│   ├── app/
│   │   ├── modules/                # approval / codegen / image / planning / workflow
│   │   ├── shared/prompting/       # PromptLoader and prompt rendering utilities
│   │   └── shared/resources/
│   │       └── prompts/            # Unified runtime prompt bundles (*.md)
│   ├── agents/                     # Agent entrypoints and packaged prompt sources
│   ├── approval/                   # Approval flow adapters
│   ├── image/                      # Image generation pipeline
│   ├── llm/                        # Unified agent/text runner backends
│   ├── routers/                    # API routes
│   └── tests/                      # Backend test suite
├── frontend/                       # React + TypeScript UI
├── mod_template/                   # C#/.NET Godot mod template
└── tools/                          # Install/start helpers, latest packaging scripts, archived historical scripts
```

## Runtime Prompt Bundles

Runtime prompts are now consolidated under:

- `backend/app/shared/resources/prompts/planning.md`
- `backend/app/shared/resources/prompts/approval.md`
- `backend/app/shared/resources/prompts/llm.md`
- `backend/app/shared/resources/prompts/analyzer.md`
- `backend/app/shared/resources/prompts/codegen.md`
- `backend/app/shared/resources/prompts/image.md`

These bundles are loaded by `backend/app/shared/prompting/prompt_loader.py` using `bundle.key` lookups such as `planning.planner_prompt` or `codegen.asset_prompt`.

## License

MIT






