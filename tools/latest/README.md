# 最新脚本目录

本目录集中存放当前推荐使用的“打包 + Docker 部署”脚本。

## 文件说明

- `package-release.ps1`
  打包前后端，生成可部署的 release bundle，并输出 zip 包。
- `deploy-docker.ps1`
  使用 release bundle 启动 Docker 部署，支持显式重置数据库。
- `templates/Dockerfile`
  release bundle 使用的应用镜像构建模板。
- `templates/docker-compose.yml`
  release bundle 使用的 Docker Compose 模板。
- `templates/.dockerignore`
  应用镜像构建时的忽略规则模板。

## 默认产物

打包脚本默认输出到：

- `tools/latest/artifacts/agentthespire-release/`
- `tools/latest/artifacts/agentthespire-release.zip`

其中 release 目录是部署脚本的默认输入。

## 推荐流程

1. 运行 `pwsh -File .\tools\latest\package-release.ps1`
2. 检查 `tools/latest/artifacts/agentthespire-release/`
3. 准备好仓库根目录下的 `config.json`
4. 运行 `pwsh -File .\tools\latest\deploy-docker.ps1`

如需重置数据库：

```powershell
pwsh -File .\tools\latest\deploy-docker.ps1 -ResetDatabase
```

## 说明

- 当前脚本以 Windows PowerShell / PowerShell 7 为主。
- Docker 部署会自动把运行时配置写入 release bundle 的 `runtime/config.json`，不会直接覆盖仓库根目录的 `config.json`。
- Docker 镜像默认把 `rembg[gpu]` 降为 CPU 版 `rembg`，优先保证后端服务可启动。
- 数据库重置是危险操作，只会在显式传入 `-ResetDatabase` 时执行。
