# tools 目录说明

本目录集中存放 AgentTheSpire 当前仍维护的安装、启动、开发辅助脚本，以及按目标打包/部署脚本。

## 当前推荐入口

- `tools\install.ps1`
  Windows 推荐安装入口。统一安装或配置 .NET 9 SDK、Godot 4.5.1 Mono、后端依赖、前端依赖与前端构建；支持 `-OnlyModDeps` 只安装 Mod 开发依赖。
- `tools\install.bat` / `./tools/install.sh`
  兼容入口。`install.bat` 现在只负责转发到 `install.ps1`，避免原 bat 安装链路中途闪退；`install.sh` 继续用于 Linux / macOS / WSL。
- `tools\setup_mod_deps.bat` / `./tools/setup_mod_deps.sh`
  Mod 开发依赖入口。Windows 下的 `setup_mod_deps.bat` 现在转发到 `install.ps1 -OnlyModDeps`；`setup_mod_deps.sh` 继续用于 Linux / macOS / WSL。
- `tools\start.bat` / `./tools/start.sh`
  启动历史兼容态 `full` 运行时。`start.sh` 会在缺少 `frontend/dist` 时自动构建，并优先使用 `backend/.venv` 中的 Python。
- `tools\start_workstation.bat`
  单独启动工作站后端，承接本地 UI、本地工作流、配置、构建与部署链路。
- `tools\start_split_local.bat` / `tools\start_split_local.ps1`
  启动“独立前端 + 本地 workstation”双进程形态。脚本会生成 `frontend/dist/runtime-config.js`、拉起本地静态前端服务和 `workstation-backend`，并默认打开本地前端地址。
- `tools\stop_split_local.bat` / `tools\stop_split_local.ps1`
  停止由 `start_split_local` 拉起的本地双进程，并清理 `runtime/split-local-state.json`。
- `tools\start_web.bat`
  单独启动 Web 后端，承接平台 API；要求已配置数据库。
- `tools\start_dev.bat`
  启动开发模式，拉起后端热重载和 Vite 前端开发服务器。
- `tools\latest\`
  当前推荐使用的多目标打包与 Docker 部署脚本目录，支持 `full`、`workstation`、`frontend`、`web` 四种目标。

## 三种运行形态对比

| 形态 | 推荐入口 | 前端托管方 | 适合谁 |
| --- | --- | --- | --- |
| 兼容态 `full` | `tools\start.bat` | `full` 后端 | 需要保留历史行为或做同机联调的开发者 |
| 工作站托管态 | `tools\start_workstation.bat` | `workstation-backend` | 单机本地创作、BYOK、本机构建部署 |
| 独立前端态 | `tools\start_split_local.bat` | 独立静态前端 | 需要单独发布前端站点，同时接入 `workstation-backend` 和 `web-backend` 的部署场景 |

拆分运行时的接口边界：

- `workstation-backend` 承接 `/api/config`、`/api/plan`、`/api/approvals/*` 与工作台 WebSocket
- `web-backend` 承接 `/api/auth/*`、`/api/me/*`、`/api/admin/*` 与平台任务接口
- 第一版只支持本机或 LAN 可达的 `workstation-backend`
- 用户侧若需要单安装包，推荐打包内容是“独立静态前端 + workstation-backend + launcher”，`web-backend` 继续独立部署

## 开发辅助

- `tools\decompile_sts2.py`
  反编译 `sts2.dll`，并把输出路径写入 `config.json`。
- `tools\verify-install-bat.ps1`
  校验 `tools\install.bat` wrapper 与 `tools\install.ps1` 的关键安装行为。

## 已归档脚本

- `tools\archive\`
  已不再作为当前主流程维护的历史脚本目录。
- `tools\archive\sandbox\`
  旧的 Windows Sandbox 安装验证链路，包含 `generate_sandbox_wsb.bat`、`sandbox_setup.bat` 和 `sandbox_test.wsb.template`。这组脚本仅保留为历史参考，不再作为顶层推荐入口。

## 生成物与说明

- `tools\latest\artifacts\`
  是 `tools\latest\package-release.ps1` 的默认本地产物输出目录，属于生成内容，不提交到仓库。
- `sandbox_test.wsb`
  是旧 Sandbox 脚本生成的本地文件，包含绝对路径，不提交到仓库。
- 所有脚本都按“脚本目录的上一级是仓库根目录”处理路径。
- `godot/`、`backend/`、`frontend/`、`config.json` 仍保留在仓库根目录，不随脚本移动。
- `tools\start_workstation.bat` 面向本地工作站运行。
- `tools\start_web.bat` 面向独立 `web-backend` 运行，不负责前端静态文件托管。
- 若采用拆分运行形态，通常是：
  - `workstation-backend` 承接前端壳、本地工作流与 WebSocket
  - `web-backend` 承接 `/api/auth/*`、`/api/me/*` 与平台任务查询/创建接口
- `tools\latest\package-release.ps1 workstation` 现在会把本地 launcher 一并复制到 release 目录的 `launcher/` 下，便于后续安装器或用户包复用。

## 独立前端运行时配置

- 前端独立部署时，默认从站点根目录加载 `runtime-config.js`。
- 默认源码文件位于 `frontend/public/runtime-config.js`，构建后会进入静态站点根目录。
- 当前约定的运行时配置键：
  - `window.__AGENT_THE_SPIRE_API_BASES__.workstation`
  - `window.__AGENT_THE_SPIRE_API_BASES__.web`
  - `window.__AGENT_THE_SPIRE_WS_BASES__.workstation`
- 建议把该文件视为部署期配置文件，而不是前端源码的一部分；更换 `workstation` 或 `web` 地址时优先覆盖此文件，不重新构建前端。
- `workstation` 地址应配置为本机或 LAN 可达地址，不应配置为公网用户本机地址。
