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
- The old integrated `full` runtime, migration flags, and workflow compat bridge are retired.
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

powershell -ExecutionPolicy Bypass -File .\tools\install.ps1   # Windows 推荐入口：安装 .NET / Godot / ilspycmd / Python deps / frontend build
tools\install.bat                                           # 兼容入口，内部转发到 install.ps1

# Copy config.example.json → config.json, fill in your API keys and game path

tools\start.bat             # Opens http://localhost:7860 (default workstation runtime)
```

See [TUTORIAL.md](TUTORIAL.md) for full setup and configuration guide.

### Backend Runtime Modes

- `tools\start.bat`
  Starts the default `workstation` runtime on `http://localhost:7860`.
- `tools\start_workstation.bat`
  Starts `workstation-backend` only. This runtime serves the local workstation UI, local workflows, approvals, config, build, and deploy flows.
- `tools\start_web.bat`
  Starts `web-backend` only on `http://localhost:7870`. This runtime is for platform/auth/job/quota APIs and requires a valid `database.url`.

Current deployment guidance:

- Single-machine local use: prefer `workstation-backend`
- Server-side platform APIs: prefer `web-backend`

Current product behavior:

- The workstation shell remains the only frontend entry.
- `/auth/*`, `/me`, and `/me/jobs/:jobId` are now available in the same shell.
- Choosing server mode creates a draft platform job first, then asks for start confirmation before queueing.
- Local BYOK execution does not create platform history.

### Tool Scripts

- Core install/start/dev helpers live in `tools/`.
- `tools/kill-local.ps1` stops local `frontend` / `workstation` / `web` processes by discovered state/config first, with port arguments available as explicit overrides.
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
- **批量创建** — 描述整个 mod 主题，AI 规划并批量生成所有素材
- **同一入口 + 用户中心** — 同一前端入口同时承接本地工作站链路与平台链路，已补登录/注册/用户中心/任务详情页
- **统一执行分流** — 点击生成后可在“本机执行 / 服务器模式”之间分流；服务器模式先创建平台任务草稿，再确认开始

### 当前运行口径

- 截至 2026-04-10，后端只保留两个有效角色：`workstation` 与 `web`。
- 历史 `full` 一体化运行时、migration flags、workflow compat bridge 已收口，不再作为当前实现基线。
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
- 发行包会直接包含可查看、可编辑的运行时知识目录；应用运行时只读取这份目录，用户修改后会直接生效。
- 运行时知识目录默认位于：
  - `runtime/knowledge/knowledge-manifest.json`
  - `runtime/knowledge/game/`
  - `runtime/knowledge/baselib/`
  - `runtime/knowledge/resources/sts2/`
  - `runtime/knowledge/cache/`
- 仓库内 `backend/agents/*` 与 `backend/app/modules/knowledge/resources/sts2/*` 仅作为开发期/打包期种子来源，不再作为运行时并列真源。

### 快速开始

```bash
git clone https://github.com/yourname/AgentTheSpire.git
cd AgentTheSpire

powershell -ExecutionPolicy Bypass -File .\tools\install.ps1   # Windows 推荐入口：安装 .NET / Godot / ilspycmd / Python 依赖 / 前端构建
tools\install.bat                                           # 兼容入口，内部转发到 install.ps1

# 如果只想安装 .NET 9 + Godot 4.5.1 + ilspycmd：
powershell -ExecutionPolicy Bypass -File .\tools\install.ps1 -OnlyModDeps

# 复制 config.example.json → config.json，填入 API Key 和游戏路径

tools\start.bat             # 打开 http://localhost:7860（默认 workstation 运行时）
```

详细配置说明见 [TUTORIAL.md](TUTORIAL.md)。

### 后端运行形态

- `tools\start.bat`
  启动默认 `workstation` 运行时，监听 `http://localhost:7860`。
- `tools\start_workstation.bat`
  仅启动 `workstation-backend`。该运行时承接本地工作站 UI、本地工作流、审批、配置、构建与部署链路。
- `tools\start_split_local.bat`
  启动“独立前端 + 本地 workstation”双进程本地形态：前端静态站点由本地轻量服务托管，工作台 HTTP/WS 指向本机 `workstation-backend`，平台接口继续指向 `web-backend`。
- `tools\start_web.bat`
  仅启动 `web-backend`，监听 `http://localhost:7870`。该运行时承接平台任务、认证、配额、历史记录等 API，并要求 `config.json` 中存在有效的 `database.url`。

当前部署口径：

- 单机本地使用，优先 `workstation-backend`
- 服务器平台 API，优先 `web-backend`
- 用户侧正式推荐打包目标：`hybrid`
- 若要验证“独立前端 + 本地 workstation + 远端/本地 web”形态，优先使用 `tools\start_split_local.bat`

三种当前相关形态的差异如下：

| 形态 | 启动入口 | 谁托管前端 | workstation 接口去向 | web 接口去向 | 适用场景 |
|------|----------|------------|----------------------|--------------|----------|
| 工作站托管态 | `tools\start_workstation.bat` | `workstation-backend` | 本机 `workstation-backend` | 通常不承接；需要平台接口时应另启 `web-backend` | 单机工作站、本地 BYOK、本机构建部署 |
| 正式部署目标 `hybrid` | `tools\latest\package-release.ps1 hybrid` | 独立静态前端 | `runtime-config.js` 指向本机或 LAN 可达 `workstation-backend` | 默认指向本机 `http://127.0.0.1:7870`，也可显式改为独立部署的 `web-backend` | 用户侧正式交付、“一个前端入口 + 两类后端能力” |
| 本地验证形态 `split-local` | `tools\start_split_local.bat` | 独立静态前端 | 指向本机 `workstation-backend` | 指向配置的 `web-backend` | 本地验证 `hybrid` 形态、开发联调 |

当前前后端边界补充：

- `workstation-backend` 继续承接本机工作流、配置、构建、部署与日志分析
- `web-backend` 默认承接 `/api/auth/*`、`/api/me/*`、平台任务与配额接口
- 用户中心只读取平台模式任务；BYOK / 本机执行不会进入服务器历史
- 服务器模式下，前端会先创建当前用户视角平台任务，再确认开始并跳转用户中心详情页

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

`hybrid` Docker 部署时，默认会联动本机 `web-backend` 并写入本机地址；只有显式传入 `-WebBaseUrl` 时才覆盖为其它地址：

```powershell
pwsh -NoProfile -File .\tools\latest\deploy-docker.ps1 hybrid
pwsh -NoProfile -File .\tools\latest\deploy-docker.ps1 hybrid -WebBaseUrl https://your-web-api.example.com
pwsh -NoProfile -File .\tools\latest\stop-deploy.ps1 hybrid
```

说明：

- `deploy-docker.ps1` 在 `hybrid` / `workstation` / `frontend` 目标下会把本地服务作为后台进程启动，并额外打开日志窗口。
- 关闭日志窗口只会停止日志查看，不会自动停止后台服务。
- 如需停止这些本地服务，请执行对应的 `stop-deploy.ps1`；脚本会读取 `release/runtime/local-deploy-state.json` 中记录的 PID。
- `deploy-docker.ps1 hybrid` 默认会从当前 hybrid release 的同级目录推导本机 `web release`，并在联动部署前自动刷新该 release；如实际目录不在同级，可显式传入 `-WebReleaseRoot`。
- 刷新默认推导出的本机 `web release` 前，脚本会先对固定的 `agentthespire-web-release` Compose 项目执行一次 `docker compose down --remove-orphans`，避免重复执行 `hybrid` 时直接改写仍被 Docker Compose 使用的 release 目录。
- Docker 构建默认会自动解析 `Python` 基础镜像，优先复用本机已有标签，并默认回退到 `m.daocloud.io/docker.io/library/python:3.11-slim`；如需手工指定，可传 `-PythonBaseImage`。
- `workstation` 本地 Python 运行时会缓存到 `release/runtime/python-runtime/workstation`；`requirements.txt` 与启动用 Python 未变化时，后续部署会直接复用该缓存，不再重复安装依赖。

默认文件位置：

- 源码开发：`frontend/public/runtime-config.js`
- 独立静态前端部署：站点根目录 `/runtime-config.js`
- 本地双进程 launcher：启动时会覆盖写入 `frontend/dist/runtime-config.js`

当前约束：

- 第一版只支持本机或 LAN 可达的 `workstation-backend`
- 不支持公网静态前端直接连接任意用户本机 workstation
- `hybrid` 用户侧发放内容推荐为“独立静态前端 + workstation-backend + launcher”
- `web-backend` 仍不打进 `hybrid` 用户包；默认 Docker 部署会按约定联动本机 `agentthespire-web-release`

### 工具脚本

- 当前安装、启动、开发辅助脚本统一放在 `tools/`。
- `tools\kill-local.ps1` 可停止本机 `frontend / workstation / web` 进程，并额外尝试停止当前仓库 `tools/latest/artifacts` 下默认 release 对应的 Docker `web` 服务；默认端口分别为 `5173 / 7860 / 7870`。
- `tools/latest/` 存放当前推荐使用的打包与 Docker 部署脚本。
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
│   ├── agents/                     # Legacy-compatible agent entrypoints
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






