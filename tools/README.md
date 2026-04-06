# tools 目录说明

本目录现在采用“一个统一入口 + 按功能分目录 + 顶层兼容 wrapper”的结构。

## 推荐入口

- `powershell -File .\tools\tools.ps1`
  默认显示可用脚本目录，并支持参数直达。

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
powershell -File .\tools\tools.ps1 latest package workstation
powershell -File .\tools\tools.ps1 latest deploy full
powershell -File .\tools\tools.ps1 latest installer
```

## 目录结构

```text
tools/
├── tools.ps1                 # 统一入口
├── README.md                 # 唯一主说明文档
├── install/                  # 安装相关真实脚本
├── start/                    # 传统启动脚本
├── split-local/              # 独立前端 + 本地 workstation 启停脚本
├── dev/                      # 开发辅助脚本
├── latest/                   # 打包、Docker 部署、安装器脚本
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
  反编译 `sts2.dll`，并把输出路径写入 `config.json`。
- `tools\dev\verify-install-bat.ps1`
  校验顶层安装 wrapper 与实际 `install\install.ps1` 的关键行为。

### latest

`tools\latest\` 继续保留为发布脚本目录，当前不改名，避免打断现有打包与 Docker 部署链路。

- `tools\latest\package-release.ps1`
  按目标打包 release bundle，并可输出 zip。
- `tools\latest\deploy-docker.ps1`
  按目标部署 Docker release。
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
| 独立前端态 | `tools.ps1 split start` | 独立静态前端 | 前端独立发布，同时接入 `workstation-backend` 和 `web-backend` |

拆分运行时的接口边界：

- `workstation-backend` 承接 `/api/config`、`/api/plan`、`/api/approvals/*` 与工作台 WebSocket
- `web-backend` 承接 `/api/auth/*`、`/api/me/*`、`/api/admin/*` 与平台任务接口
- 第一版只支持本机或 LAN 可达的 `workstation-backend`
- 用户侧若需要单安装包，推荐打包内容是“独立静态前端 + workstation-backend + launcher”，`web-backend` 继续独立部署

## 其它说明

- 所有真实脚本都已经迁入功能目录，顶层旧脚本只保留兼容层职责。
- `tools\latest\package-release.ps1 workstation` 仍会把 launcher 脚本复制到 release 目录下的 `launcher/` 中。
- `runtime-config.js` 仍属于部署期配置文件；更换 `workstation` 或 `web` 地址时优先覆盖该文件，不重新构建前端。
- `workstation` 地址应配置为本机或 LAN 可达地址，不应配置为公网用户本机地址。
