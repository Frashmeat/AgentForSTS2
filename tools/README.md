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
- `tools\start_web.bat`
  单独启动 Web 后端，承接平台 API；要求已配置数据库。
- `tools\start_dev.bat`
  启动开发模式，拉起后端热重载和 Vite 前端开发服务器。
- `tools\latest\`
  当前推荐使用的多目标打包与 Docker 部署脚本目录，支持 `full`、`workstation`、`frontend`、`web` 四种目标。

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
