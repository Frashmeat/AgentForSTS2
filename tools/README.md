# tools 目录说明

本目录现在采用“一个统一入口 + 分层数字菜单 + 参数直达 + 功能子目录真实脚本”的结构。

## 推荐入口

- `powershell -File .\tools\tools.ps1`
  默认进入分层数字菜单，可直接用键盘选择分组、脚本和参数模板。

说明：

- 以上示例沿用 `powershell -File` 写法；如果你当前使用的是 `pwsh`，可直接替换为 `pwsh -File`。
- `tools.ps1` 现在会沿用当前 PowerShell 宿主执行子脚本，不再强制切回 Windows PowerShell 5.1。
- `tools.ps1` 与常用停止脚本按 Windows PowerShell 5.1 / PowerShell 7 都可解析的 UTF-8 BOM 保存；如编辑脚本，需保留该编码，避免中文帮助文本在 Windows PowerShell 5.1 下被错误解析。

菜单特性：

- 直接运行后进入主菜单，不需要手输完整命令
- 每一级都用数字键选择
- 支持 `B` 返回上一级
- 支持 `Q` 退出
- 执行前会显示真实脚本路径、参数模板和命令预览，再次确认后才执行

## 参数直达

如果你已经熟悉命令，也可以继续用参数直达模式：

常用示例：

```powershell
# 安装
powershell -File .\tools\tools.ps1 install
powershell -File .\tools\tools.ps1 install mod

# 启动
powershell -File .\tools\tools.ps1 start workstation
powershell -File .\tools\tools.ps1 start web
powershell -File .\tools\tools.ps1 start dev

# 独立前端 + 本地 workstation
powershell -File .\tools\tools.ps1 split start -DryRun
powershell -File .\tools\tools.ps1 split stop

# 停止 / 清理
powershell -File .\tools\tools.ps1 stop local
powershell -File .\tools\tools.ps1 stop deploy hybrid

# 开发辅助
powershell -File .\tools\tools.ps1 dev decompile

# latest 打包 / 部署
powershell -File .\tools\tools.ps1 latest package hybrid
powershell -File .\tools\tools.ps1 latest package workstation
powershell -File .\tools\tools.ps1 latest deploy hybrid -DeployLocalWeb
powershell -File .\tools\tools.ps1 latest deploy hybrid -WebBaseUrl https://your-web-api.example.com
powershell -File .\tools\tools.ps1 latest deploy web
powershell -File .\tools\tools.ps1 latest deploy web -ResetDb
powershell -File .\tools\tools.ps1 latest installer

# 停止 latest deploy 拉起的本地服务
pwsh -NoProfile -File .\tools\latest\stop-deploy.ps1 hybrid
```

## 菜单结构

主菜单当前包含：

1. 安装
2. 启动
3. 拆分运行时
4. 停止 / 清理
5. 开发辅助
6. 打包 / 部署

每个脚本项下面还会继续进入“参数模板”菜单，例如：

- 安装
  - 直接执行
  - 查看帮助
- split-local 启动
  - 默认启动
  - DryRun 预览
  - 启动但不打开浏览器
  - 自定义端口 / Web API 地址
- 停止 / 清理
  - 停止本机 frontend/workstation/web
  - 停止 latest deploy 本地服务
- latest 打包
  - 直接打包
  - 打包但不压缩
  - 查看帮助

## 目录结构

```text
tools/
├── tools.ps1                 # 统一入口
├── README.md                 # 唯一主说明文档
├── install/                  # 安装相关真实脚本
├── start/                    # 传统启动脚本
├── split-local/              # 独立前端 + 本地 workstation 启停脚本
├── stop/                     # 停止 / 清理相关真实脚本
├── dev/                      # 开发辅助脚本
├── latest/                   # 打包、混合部署、安装器脚本
└── archive/                  # 已归档历史脚本与产物
```

## 功能分组

### install

- `tools\install\install.ps1`
  Windows 主安装入口。统一安装或配置 .NET 9 SDK、Godot 4.5.1 Mono、ilspycmd、后端依赖、前端依赖与前端构建；支持 `-OnlyModDeps`。
  安装 `ilspycmd` 时会把 `~\.dotnet\tools` 和项目内 `runtime\tools` 同步加入当前会话与用户 PATH。
- `tools\install\install.sh`
  Linux / macOS / WSL 安装入口。
- `tools\install\setup_mod_deps.bat`
- `tools\install\setup_mod_deps.sh`
  只安装 Mod 开发依赖。

### start

- `tools\start\start.bat` / `tools\start\start.sh`
  历史启动包装脚本，日常请优先使用 `tools.ps1 start workstation`、`tools.ps1 start web` 或 `tools.ps1 start dev`。
- `tools\start\start_workstation.bat`
  启动 `workstation-backend`。
- `tools\start\start_web.bat`
  启动 `web-backend`。
- `tools\start\start_dev.bat`
  启动开发模式。

### split-local

- `tools\split-local\start_split_local.ps1`
- `tools\split-local\start_split_local.bat`
  启动“独立前端 + 本地 workstation”双进程。
- `tools\split-local\stop_split_local.ps1`
- `tools\split-local\stop_split_local.bat`
  停止 split-local 双进程，并清理 `runtime/split-local-state.json`。

### stop

- `tools.ps1 stop local`
  统一入口。对应 `tools\stop\kill-local.ps1`，优先按状态文件和配置发现端口并停止当前仓库识别出的本机 `frontend / workstation / web` 进程。
- `tools.ps1 stop deploy <target>`
  统一入口。对应 `tools\latest\stop-deploy.ps1`，按 `release\runtime\local-deploy-state.json` 停止 `deploy-docker.ps1` 拉起的本地进程。
  参数直达不传 `<target>` 时默认处理 `hybrid`；菜单模式中选择具体目标时只传所选目标，不再叠加默认目标。

### dev

- `tools\dev\decompile_sts2.py`
  反编译 `sts2.dll`，并把输出路径写入 `config.json`。不传参数时会默认读取 `config.json` 中的 `sts2_path`。

### 本地进程停止

- `tools\stop\kill-local.ps1`
  真实脚本入口；日常优先使用 `tools.ps1 stop local`。
  优先读取 `local-deploy-state.json`、`split-local-state.json`、`runtime/workstation.config.json` 发现端口和 PID，再停止本机 `frontend`、`workstation`、`web` 三类进程；命令行端口参数仅作为显式覆盖。
  同时会清理当前 PowerShell 会话中残留的日志镜像事件与 writer 句柄，避免 `runtime/logs/*.log` 被持续占用。
  如存在 Docker 化 `web` 服务，也会对当前仓库 `tools\latest\artifacts` 下可识别的 release 尝试执行 `docker compose down --remove-orphans`。
  对 `web` 端口会先校验进程归属，显式跳过 `com.docker.backend`、`wslrelay` 等 Docker Desktop / WSL 宿主代理进程，避免误杀后导致 Docker 命名管道失联。
  注意：`frontend` / `workstation` 的端口清理仍以“当前仓库本地服务进程”识别为前提；Docker 部分默认只处理当前仓库 `artifacts` 下能识别出的 release，不删除卷。

### latest

`tools\latest\` 继续保留为发布脚本目录，当前不改名，避免打断现有打包与发布链路。

- `tools\latest\package-release.ps1`
  按目标打包 release bundle，并可输出 zip。
  `workstation` / `hybrid` release 会同步带上 `runtime\tools\`，确保 `ilspycmd` 这类运行时工具及其 `.store` 依赖目录一并进入发布目录。
  `workstation` 相关目标在传入 `-Debug` 时只会沿用已有 `runtime\workstation.config.json`，不再从旧 `services\workstation\config.json` 迁移设置。
  当前会保留已有 release 的 `runtime\.env`，避免 `web` 部署已生成的 secret 在重新打包同名 release 时漂移。
- `tools\latest\deploy-docker.ps1`
  按目标部署 mixed release。
  `web` 继续使用 Docker；`workstation` 与 `frontend` 改为本机启动；`hybrid` 会本机启动 `workstation + frontend`。
  未显式传入 `-ConfigPath` 时，脚本只会优先读取 release 内 `runtime\*.config.json`，再回退到服务目录内 `config.example.json`。
  `config.example.json` 现仅保留真实配置字段，不再内嵌 `_注释_*` 伪字段；常用可选值说明如下：
  `llm.mode`：`agent_cli`、`claude_api`
  `llm.agent_backend`：`claude`、`codex`
  `image_gen.model`：`flux.2-pro`、`flux.2-flex`、`flux.2-klein`、`flux.2-max`、`flux.2-dev`、`flux.1.1-pro`
  `image_gen.provider`：`bfl`、`fal`、`volcengine`、`wanxiang`
  若自动发现的 `runtime\workstation.config.json` / `runtime\web.config.json` 已损坏无法解析，脚本会告警并自动回退到服务目录内的 `config.example.json`，随后重写新的 runtime 配置。
  当显式传入 `-FrontendPort` 时，脚本会把实际前端端口同步补到 `runtime\workstation.config.json` / linked `web` 的 `runtime\web.config.json` 的 `cors_origins`，避免独立静态前端换端口后被本地 CORS 拦截。
  本地 `workstation` 与 linked `web` 运行时还会额外开启 loopback origin 兜底，允许 `localhost` / `127.0.0.1` / `::1` 下的任意本地端口访问；该兜底只用于本机部署，不改变正式公网 `web` 的显式白名单口径。
  默认会基于当前 release 重新 `build` 需要 Docker 的目标镜像；只有显式传入 `-ReuseImages` 时才会复用已有镜像。
  `web` 目标会在 `runtime\.env` 中自动生成并持久化 `SPIREFORGE_AUTH_SESSION_SECRET` 与 `SPIREFORGE_SERVER_CREDENTIAL_SECRET`，随后以环境变量注入容器；`runtime\web.config.json` 不再继续写入 `auth.session_secret`。
  `web -ResetDb` 会在部署前删除 Docker 数据卷并重建 Postgres 数据库；统一入口菜单中以“部署 web（重置数据库）”单独暴露，避免和普通部署混淆。
  `web` 目标部署前还会把 release 内的 `docker-compose.yml` 刷新为仓库模板，避免继续沿用旧 release 遗留的 Compose 环境注入方式。
  `frontend` 未显式传入 `-WebBaseUrl` 时会默认写入本机 `http://127.0.0.1:7870`；`hybrid` 需显式传入 `-WebBaseUrl`，或改用 `-DeployLocalWeb`。
  `hybrid` 传入 `-DeployLocalWeb`（可配合 `-WebReleaseRoot`）时，会从当前 hybrid release 的同级目录推导本机 `web release`，并在联动部署前自动刷新该 release，确保跟随当前仓库模板；如目录不在同级，可显式传入 `-WebReleaseRoot`。若未显式传入 `-ConfigPath`，联动部署的 `web` 目标会继续沿用 release 内 `runtime\web.config.json` / `config.example.json` 的默认回退链。
  Docker 构建默认会自动解析 `Python` 基础镜像，优先复用本机已有标签，并默认回退到 `m.daocloud.io/docker.io/library/python:3.11-slim`；如需手工指定，可传 `-PythonBaseImage`。
  若默认 `runtime\logs\*.log` 正被旧进程占用，脚本会自动回退到带时间戳后缀的新日志文件，避免本机部署直接失败。
  `package-release.ps1` 与 `deploy-docker.ps1` 现已兼容 Windows PowerShell 5.1 与 PowerShell 7，不再依赖 `ConvertFrom-Json -AsHashtable`，`package-release.ps1` 的相对路径处理也不再依赖 `System.IO.Path.GetRelativePath`。
- `tools\latest\stop-deploy.ps1`
  真实脚本入口；日常优先使用 `tools.ps1 stop deploy <target>`。
  停止 `deploy-docker.ps1` 以本机模式拉起的 `workstation` / `frontend` 进程，并关闭对应日志窗口。
  状态文件位于 `release\runtime\local-deploy-state.json`；只关闭日志窗口并不会自动停止服务。
- `tools\latest\build-workstation-installer.ps1`
  构建 Windows 工作站安装器。
- `tools\latest\templates\`
  发布模板目录。
- `tools\latest\artifacts\`
  本地产物目录，不提交到仓库。

### archive

- `tools\archive\`
  历史脚本和归档产物目录。
- `tools\archive\sandbox\`
  旧的 Windows Sandbox 验证链路。
- `tools\archive\sandbox_test.wsb`
  旧 Sandbox 相关本地产物样例。

## 运行形态对比

| 形态 | 推荐命令 | 前端托管方 | 适合谁 |
| --- | --- | --- | --- |
| 工作站托管态 | `tools.ps1 start workstation` | `workstation-backend` | 单机本地创作、BYOK、本机构建部署 |
| 正式部署目标 `hybrid` | `tools.ps1 latest package hybrid` / `tools.ps1 latest deploy hybrid` | 独立静态前端 | 用户侧正式交付，前端独立发布并接入 `workstation-backend`；需显式传 `-WebBaseUrl` 或 `-DeployLocalWeb` |
| 本地验证形态 `split-local` | `tools.ps1 split start` | 独立静态前端 | 本地验证 `hybrid` 形态与开发联调 |

拆分运行时的接口边界：

- `workstation-backend` 承接 `/api/config`、`/api/plan`、`/api/approvals/*` 与工作台 WebSocket
- `web-backend` 承接 `/api/auth/*`、`/api/me/*`、`/api/admin/*` 与平台任务接口
- 第一版只支持本机或 LAN 可达的 `workstation-backend`
- `hybrid` 用户侧若需要单安装包，推荐打包内容是“独立静态前端 + workstation-backend + launcher”，`web-backend` 继续独立部署

## 其它说明

- 日常入口统一收敛到 `tools.ps1`；如需绕过菜单，可直接调用功能子目录下的真实脚本。
- `tools.ps1` 现在默认优先提供菜单式选择，适合日常本地使用；参数直达模式更适合脚本化或熟悉命令后的快速调用。
- `tools\latest\package-release.ps1 workstation` 仍会把 launcher 脚本复制到 release 目录下的 `launcher/` 中。
- `tools\latest\package-release.ps1 hybrid` 会同时整理 `frontend` 与 `workstation` 两类用户侧服务，并附带 launcher。
- `tools\latest\deploy-docker.ps1 workstation` 会在本机拉起 `workstation-backend`，并以 `runtime\workstation.config.json` 作为工作站应用配置真源。
- `tools\latest\deploy-docker.ps1 frontend` 会在本机拉起静态前端服务，并把 `runtime-config.js` 写入 release 内的前端 `dist/`。
- `tools\latest\deploy-docker.ps1 hybrid -DeployLocalWeb` 会联动部署本机 `web-backend`，并把前端 `web` 地址写成 `http://127.0.0.1:7870`。
- 默认推导出的本机 `web release` 会在 `deploy-docker.ps1 hybrid` 联动部署前自动调用 `package-release.ps1 web` 刷新，避免继续复用旧 release。
- 刷新默认推导出的本机 `web release` 前，脚本会先对固定的 `agentthespire-web-release` Compose 项目执行一次 `docker compose down --remove-orphans`，避免重复执行 `hybrid` 时在仍被 Compose 使用的 release 目录上直接重写文件。
- `tools\latest\deploy-docker.ps1 hybrid -WebReleaseRoot <path>` 可显式指定要联动部署的本机 `web release` 目录，避免依赖默认同级路径。
- `tools\latest\deploy-docker.ps1 ... -PythonBaseImage <image>` 可显式指定 Docker 构建使用的 Python 基础镜像；不传时默认优先复用本机已有标签，并回退到 `m.daocloud.io/docker.io/library/python:3.11-slim`。
- `tools\latest\deploy-docker.ps1 hybrid -WebBaseUrl https://your-web-api.example.com` 会改为指向显式传入的地址，此时不再默认覆盖为本机地址。
- `tools\latest\deploy-docker.ps1` 拉起的本地服务 PID 会写入 `runtime\local-deploy-state.json`，供 `tools\latest\stop-deploy.ps1` 读取并停止。
- `deploy-docker.ps1` 打开的日志终端只是查看窗口；关闭窗口不会自动停止后台服务进程。
- `workstation` 本地 Python 运行时缓存位于 `runtime\python-runtime\workstation`；只要 `services\workstation\backend\requirements.txt` 与启动所用 Python 没变化，后续部署会优先复用该缓存。
- `package-release.ps1` 生成 zip 时会默认排除 `runtime\python-runtime` 本地缓存，避免把本机 `.venv` 与依赖缓存一并打进发布归档；release 目录本身仍会保留该缓存供本地部署复用。
- `hybrid` / `workstation` 形态下，工作站应用配置统一收敛到 `runtime\workstation.config.json`；`services\workstation\config.json` 不再作为独立真源。
- 后端进程直启时，运行时配置默认只读取 `runtime\workstation.config.json` / `runtime\web.config.json`；仓库根 `config.json` 不再作为运行时回退来源，如需自定义路径请显式设置 `SPIREFORGE_CONFIG_PATH`。
- `runtime-config.js` 仍属于部署期配置文件；更换 `workstation` 或 `web` 地址时优先覆盖该文件，不重新构建前端。
- `workstation` 地址应配置为本机或 LAN 可达地址，不应配置为公网用户本机地址。
