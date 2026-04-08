# tools 目录说明

本目录现在采用“一个统一入口 + 分层数字菜单 + 参数直达 + 顶层兼容 wrapper”的结构。

## 推荐入口

- `powershell -File .\tools\tools.ps1`
  默认进入分层数字菜单，可直接用键盘选择分组、脚本和参数模板。

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
powershell -File .\tools\tools.ps1 start full
powershell -File .\tools\tools.ps1 start workstation
powershell -File .\tools\tools.ps1 start web
powershell -File .\tools\tools.ps1 start dev

# 独立前端 + 本地 workstation
powershell -File .\tools\tools.ps1 split start -DryRun
powershell -File .\tools\tools.ps1 split stop

# 开发辅助
powershell -File .\tools\tools.ps1 dev verify-install
powershell -File .\tools\tools.ps1 dev decompile

# latest 打包 / 部署
powershell -File .\tools\tools.ps1 latest package hybrid
powershell -File .\tools\tools.ps1 latest package workstation
powershell -File .\tools\tools.ps1 latest deploy hybrid
powershell -File .\tools\tools.ps1 latest deploy hybrid -WebBaseUrl https://your-web-api.example.com
powershell -File .\tools\tools.ps1 latest deploy full
powershell -File .\tools\tools.ps1 latest installer

# 停止 latest deploy 拉起的本地服务
pwsh -NoProfile -File .\tools\latest\stop-deploy.ps1 hybrid
```

## 菜单结构

主菜单当前包含：

1. 安装
2. 启动
3. 拆分运行时
4. 开发辅助
5. 打包 / 部署

每个脚本项下面还会继续进入“参数模板”菜单，例如：

- 安装
  - 直接执行
  - 查看帮助
- split-local 启动
  - 默认启动
  - DryRun 预览
  - 启动但不打开浏览器
  - 自定义端口 / Web API 地址
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
├── dev/                      # 开发辅助脚本
├── latest/                   # 打包、混合部署、安装器脚本
└── archive/                  # 已归档历史脚本与产物
```

## 顶层兼容入口

以下顶层脚本仍然保留，但现在只作为兼容 wrapper，内部会转发到对应子目录中的真实脚本：

- `tools\install.ps1`
- `tools\install.bat`
- `tools\install.sh`
- `tools\setup_mod_deps.bat`
- `tools\setup_mod_deps.sh`
- `tools\start.bat`
- `tools\start.sh`
- `tools\start_dev.bat`
- `tools\start_web.bat`
- `tools\start_workstation.bat`
- `tools\start_split_local.ps1`
- `tools\start_split_local.bat`
- `tools\stop_split_local.ps1`
- `tools\stop_split_local.bat`
- `tools\verify-install-bat.ps1`

后续日常使用建议优先改为 `tools.ps1`，避免直接依赖兼容层。

## 功能分组

### install

- `tools\install\install.ps1`
  Windows 主安装入口。统一安装或配置 .NET 9 SDK、Godot 4.5.1 Mono、后端依赖、前端依赖与前端构建；支持 `-OnlyModDeps`。
- `tools\install\install.sh`
  Linux / macOS / WSL 安装入口。
- `tools\install\setup_mod_deps.bat`
- `tools\install\setup_mod_deps.sh`
  只安装 Mod 开发依赖。

### start

- `tools\start\start.bat` / `tools\start\start.sh`
  启动兼容态 `full` 运行时。
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

### dev

- `tools\dev\decompile_sts2.py`
  反编译 `sts2.dll`，并把输出路径写入 `config.json`。不传参数时会默认读取 `config.json` 中的 `sts2_path`。
- `tools\dev\verify-install-bat.ps1`
  校验顶层安装 wrapper 与实际 `install\install.ps1` 的关键行为。

### latest

`tools\latest\` 继续保留为发布脚本目录，当前不改名，避免打断现有打包与发布链路。

- `tools\latest\package-release.ps1`
  按目标打包 release bundle，并可输出 zip。
- `tools\latest\deploy-docker.ps1`
  按目标部署 mixed release。
  `web` 继续使用 Docker；`workstation` 与 `frontend` 改为本机启动；`full` 会本机启动 `workstation` 并只用 Docker 部署 `web`；`hybrid` 会本机启动 `workstation + frontend`，默认还会联动部署本机 `web-backend`。
  默认会基于当前 release 重新 `build` 需要 Docker 的目标镜像；只有显式传入 `-ReuseImages` 时才会复用已有镜像。
  `hybrid` / `frontend` 未显式传入 `-WebBaseUrl` 时会默认写入本机 `http://127.0.0.1:7870`；`hybrid` 此时还会联动部署本机 `web-backend`。
- `tools\latest\stop-deploy.ps1`
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
| 兼容态 `full` | `tools.ps1 start full` | `full` 后端 | 保留历史行为或做同机联调 |
| 工作站托管态 | `tools.ps1 start workstation` | `workstation-backend` | 单机本地创作、BYOK、本机构建部署 |
| 正式部署目标 `hybrid` | `tools.ps1 latest package hybrid` / `tools.ps1 latest deploy hybrid` | 独立静态前端 | 用户侧正式交付，前端独立发布并接入 `workstation-backend`；默认联动本机 `web-backend`，也可显式改为远端 |
| 本地验证形态 `split-local` | `tools.ps1 split start` | 独立静态前端 | 本地验证 `hybrid` 形态与开发联调 |

拆分运行时的接口边界：

- `workstation-backend` 承接 `/api/config`、`/api/plan`、`/api/approvals/*` 与工作台 WebSocket
- `web-backend` 承接 `/api/auth/*`、`/api/me/*`、`/api/admin/*` 与平台任务接口
- 第一版只支持本机或 LAN 可达的 `workstation-backend`
- `hybrid` 用户侧若需要单安装包，推荐打包内容是“独立静态前端 + workstation-backend + launcher”，`web-backend` 继续独立部署

## 其它说明

- 所有真实脚本都已经迁入功能目录，顶层旧脚本只保留兼容层职责。
- `tools.ps1` 现在默认优先提供菜单式选择，适合日常本地使用；参数直达模式更适合脚本化或熟悉命令后的快速调用。
- `tools\latest\package-release.ps1 workstation` 仍会把 launcher 脚本复制到 release 目录下的 `launcher/` 中。
- `tools\latest\package-release.ps1 hybrid` 会同时整理 `frontend` 与 `workstation` 两类用户侧服务，并附带 launcher。
- `tools\latest\deploy-docker.ps1 workstation` 会在本机拉起 `workstation-backend`，并把配置写到 `services\workstation\config.json` 与 `runtime\workstation.config.json`。
- `tools\latest\deploy-docker.ps1 frontend` 会在本机拉起静态前端服务，并把 `runtime-config.js` 写入 release 内的前端 `dist/`。
- `tools\latest\deploy-docker.ps1 full` 会在本机拉起 `workstation-backend`，同时只对 `web` 服务执行 Docker 部署。
- `tools\latest\deploy-docker.ps1 hybrid` 默认会联动部署本机 `web-backend`，并把前端 `web` 地址写成 `http://127.0.0.1:7870`。
- `tools\latest\deploy-docker.ps1 hybrid -WebBaseUrl https://your-web-api.example.com` 会改为指向显式传入的地址，此时不再默认覆盖为本机地址。
- `tools\latest\deploy-docker.ps1` 拉起的本地服务 PID 会写入 `runtime\local-deploy-state.json`，供 `tools\latest\stop-deploy.ps1` 读取并停止。
- `deploy-docker.ps1` 打开的日志终端只是查看窗口；关闭窗口不会自动停止后台服务进程。
- `hybrid` / `workstation` / `full` 形态下，工作站配置会同时写入 `services\workstation\config.json` 与 `runtime\workstation.config.json`，方便运行时读取和排查。
- `runtime-config.js` 仍属于部署期配置文件；更换 `workstation` 或 `web` 地址时优先覆盖该文件，不重新构建前端。
- `workstation` 地址应配置为本机或 LAN 可达地址，不应配置为公网用户本机地址。
