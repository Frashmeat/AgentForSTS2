# runtime/

容器运行时配置与生成物目录。整个目录被 `.gitignore`，本说明文件通过白名单显式纳入。

## 必需文件

| 角色 | 文件 | 来源 |
|------|------|------|
| 桌面端 | `runtime/workstation.config.json` | 由 `tools/docker/workstation.ps1` 自动从 `config.example.json` 复制；可手工调整 |
| Web 端 | `runtime/web.config.json` | 由 `tools/docker/web.ps1` 自动从 `config.example.json` 复制；至少补上 `database.url` 与 `auth.session_secret` |

## 容器内挂载

- Workstation 容器：`runtime/workstation.config.json` → `/app/config.json:ro`
- Web 容器：`runtime/web.config.json` → `/app/config.json:ro`

backend 启动时通过 `_resolve_runtime_config_path()` 读取 `/app/config.json`（见 `backend/app/shared/infra/config/settings.py`）。

## 持久化卷

容器侧的运行时产物（mod 工程、知识库缓存、上传的素材等）写入容器内 `/app/backend/runtime`，对应宿主侧 docker named volume：

- `ats_workstation_runtime`
- `ats_web_runtime`

需要清空时执行 `docker volume rm <name>` 或 `tools/docker/web.ps1 reset-db`（仅删 Postgres）。
