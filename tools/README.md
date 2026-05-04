# tools 目录说明

> 文档定位：本文说明当前工具脚本入口、部署拓扑和配置真实源。
>
> 事实依据：基于 2026-05-04 打包部署收口后的 `tools/tools.ps1` 与 `tools/latest/*-app.ps1`。
>
> 最后更新：2026-05-04

## 当前结论

打包与部署只保留一条主线：

```powershell
powershell -File .\tools\tools.ps1 package app
powershell -File .\tools\tools.ps1 deploy app
powershell -File .\tools\tools.ps1 deploy app -DryRun
powershell -File .\tools\tools.ps1 stop app
```

人工只维护一个配置文件：

```text
runtime/agentthespire.config.json
```

可从示例复制：

```powershell
Copy-Item .\runtime\agentthespire.config.example.json .\runtime\agentthespire.config.json
```

部署脚本会生成：

```text
runtime/generated/local-workstation.config.json
runtime/generated/web.config.json
runtime/generated/web-workstation.config.json
runtime/generated/runtime-config.js
runtime/generated/docker.env
```

这些文件是派生物，可删除重建，不承载人工配置真源。

## 固定拓扑

| 组件 | 运行位置 | 说明 |
| --- | --- | --- |
| `frontend` | 本机进程 | 托管已构建静态前端 |
| `local-workstation` | 本机进程 | 本机 STS2、Godot、Mods、CLI 与本地工作流 |
| `web` | Docker | 平台 API、用户、任务、配额、管理端 |
| `web-workstation` | Docker | Web 服务器执行面，只接受 `web` 内网调度 |
| `postgres` | Docker | 平台数据库 |

边界规则：

- 前端只连接本机 `local-workstation` 与 `web`。
- 前端不会直接连接 `web-workstation`。
- `web` 通过 Docker 内网访问 `http://web-workstation:7860`。
- `web-workstation` 在 compose 中只 `expose` 容器端口，不暴露宿主端口。

## 真实脚本

| 统一入口 | 真实脚本 |
| --- | --- |
| `tools.ps1 package app` | `tools/latest/package-app.ps1` |
| `tools.ps1 deploy app` | `tools/latest/deploy-app.ps1` |
| `tools.ps1 stop app` | `tools/latest/stop-app.ps1` |
| `tools.ps1 stop local` | `tools/stop/kill-local.ps1` |
| `tools.ps1 test ...` | `tools/test/run-pytest.ps1` |

`tools/latest/templates/compose.app.yml` 是唯一当前 Docker Compose 模板，包含 `postgres`、`web`、`web-workstation`。

## 常用命令

```powershell
# 查看菜单
powershell -File .\tools\tools.ps1 help

# 安装依赖
powershell -File .\tools\tools.ps1 install
powershell -File .\tools\tools.ps1 install mod

# 开发启动
powershell -File .\tools\tools.ps1 start workstation
powershell -File .\tools\tools.ps1 start web
powershell -File .\tools\tools.ps1 start dev
powershell -File .\tools\tools.ps1 split start -DryRun

# 测试
powershell -File .\tools\tools.ps1 test backend/tests/test_tools_entry_script.py -q

# App 主线
powershell -File .\tools\tools.ps1 package app
powershell -File .\tools\tools.ps1 deploy app -DryRun
powershell -File .\tools\tools.ps1 deploy app
powershell -File .\tools\tools.ps1 stop app
```

## 部署命令说明

### `package app`

生成固定拓扑 release：

```text
tools/latest/artifacts/agentthespire-app-release/
```

默认会执行前端构建；如已确认 `frontend/dist` 最新，可传：

```powershell
powershell -File .\tools\tools.ps1 package app -SkipFrontendBuild
```

如只需要目录、不生成 zip：

```powershell
powershell -File .\tools\tools.ps1 package app -NoZip
```

### `deploy app`

读取 `runtime/agentthespire.config.json`，生成 `runtime/generated/*`，再启动：

- 本机 `local-workstation`
- 本机 `frontend`
- Docker `postgres + web-workstation + web`

默认“直接部署”会执行 Docker compose build/up，并校验 `postgres`、`web-workstation`、`web` 三个服务都已处于 `running`；任一服务缺失或未运行会直接报错，不继续伪装部署成功。

轻量预览：

```powershell
powershell -File .\tools\tools.ps1 deploy app -DryRun
```

`-DryRun` 只生成配置并打印拓扑，不启动进程或 Docker，也不要求 `frontend/dist` 已存在。

### `stop app`

停止 `deploy app` 拉起的本机进程，并对当前 app compose 项目执行 `docker compose down --remove-orphans`。

## 目录结构

```text
tools/
├── tools.ps1
├── README.md
├── install/
├── start/
├── split-local/
├── stop/
├── test/
├── dev/
├── latest/
│   ├── package-app.ps1
│   ├── deploy-app.ps1
│   ├── stop-app.ps1
│   └── templates/compose.app.yml
└── archive/
```

旧的 `tools/docker/*`、根目录双 compose、`package-release.ps1`、`deploy-docker.ps1`、`stop-deploy.ps1`、`build-workstation-installer.ps1` 已退出当前主线。
