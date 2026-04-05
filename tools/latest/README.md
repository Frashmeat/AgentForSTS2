# 最新脚本目录

本目录集中存放当前推荐使用的“按目标打包 + Docker 部署”脚本。

## 文件说明

- `package-release.ps1`
  按目标打包 release bundle，并输出 zip 包。
- `deploy-docker.ps1`
  按目标启动 Docker 部署，支持显式重置数据库和按需重建镜像。
- `templates/compose.*.yml`
  不同目标的 Docker Compose 模板。
- `templates/workstation/`
  前端 + 工作站服务模板。
- `templates/web/`
  Web 后端服务模板。
- `templates/frontend/`
  纯前端服务模板。

## 默认产物

打包脚本默认按目标输出到：

- `tools/latest/artifacts/agentthespire-workstation-release/`
- `tools/latest/artifacts/agentthespire-full-release/`
- `tools/latest/artifacts/agentthespire-frontend-release/`
- `tools/latest/artifacts/agentthespire-web-release/`

其中每个 release 目录都是部署脚本的默认输入。

## 支持目标

- `full`
  前端 + 工作站 + Web 后端
- `workstation`
  前端 + 工作站
- `frontend`
  前端
- `web`
  Web 后端

## 推荐流程

1. 运行 `pwsh -File .\tools\latest\package-release.ps1 -Target workstation`
2. 检查 `tools/latest/artifacts/agentthespire-workstation-release/`
3. 准备好仓库根目录下的 `config.json`
4. 运行 `pwsh -File .\tools\latest\deploy-docker.ps1 -Target workstation`

常用示例：

```powershell
# 给最终用户：前端 + 工作站
pwsh -File .\tools\latest\package-release.ps1 -Target workstation
pwsh -File .\tools\latest\deploy-docker.ps1 -Target workstation

# 只打前端静态站
pwsh -File .\tools\latest\package-release.ps1 -Target frontend
pwsh -File .\tools\latest\deploy-docker.ps1 -Target frontend

# 自己服务器：Web 后端
pwsh -File .\tools\latest\package-release.ps1 -Target web
pwsh -File .\tools\latest\deploy-docker.ps1 -Target web

# 同机验证完整组合
pwsh -File .\tools\latest\package-release.ps1 -Target full
pwsh -File .\tools\latest\deploy-docker.ps1 -Target full
```

如需强制重新构建镜像：

```powershell
pwsh -File .\tools\latest\deploy-docker.ps1 -Target workstation -RebuildImages
```

如需重置数据库：

```powershell
pwsh -File .\tools\latest\deploy-docker.ps1 -Target web -ResetDatabase
```

## 说明

- 当前脚本以 Windows PowerShell / PowerShell 7 为主。
- `package-release.ps1` 在前端目标打包前会检查 `frontend/package.json`、`package-lock.json` 和 `node_modules`；若检测到依赖缺失或锁文件落后，会先执行一次 `npm install` 再构建前端。
- Docker 部署会自动把运行时配置写入 release bundle 的 `runtime/` 目录，不会直接覆盖仓库根目录的 `config.json`。
- Docker 部署默认不会在每次执行时强制重建镜像；只有显式传入 `-RebuildImages` 时，才会重新进入镜像构建和依赖安装阶段。
- `web` / `full` 目标会优先复用本机已存在的 Postgres 镜像；如需指定数据库镜像，可传入 `-PostgresImage`。
- `full` 目标默认会先删除数据库卷并重建数据库；`web` 目标仍需显式传入 `-ResetDatabase`。
- Docker 镜像默认把 `rembg[gpu]` 降为 CPU 版 `rembg`，优先保证后端服务可启动。
- 工作站包会剔除 `platform_admin.py`、`platform_jobs.py` 和 `main_web.py`。
- Web 包会剔除工作站路由和 `main_workstation.py`。
- 数据库重置是危险操作；其中 `full` 目标会默认执行，`web` 目标只会在显式传入 `-ResetDatabase` 时执行。
