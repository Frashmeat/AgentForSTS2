# 2026-04-23 hybrid 本地前端端口与 CORS 自适应修复计划

## 当前理解

- 当前 `tools/latest/deploy-docker.ps1 hybrid` 支持通过 `-FrontendPort` 启动任意本地前端端口，但写入 `runtime/workstation.config.json` 与 `runtime/web.config.json` 的 `runtime.*.cors_origins` 仍主要沿用静态模板值。
- 当本地前端改到 `4173`、`3000` 等非模板内端口时，浏览器对 `workstation-backend` 与 `web-backend` 的请求会被 CORS 拦截。
- 前端侧的直接表现是：
  - 设置页 `loadAppConfig()` 一直失败，页面停留在“加载中…”
  - `SessionProvider` 访问 `web` 的 `/api/auth/me` 失败后把认证状态判为 `unavailable`
  - 顶部显示“平台账号未启用”，服务器模式页误判“当前环境未接入独立 Web 平台服务”
- 这不是单点页面 bug，而是“部署参数、运行时配置、后端 CORS 策略”跨层脱节。

## 计划

- [x] 为 `deploy-docker.ps1` 增加“按真实本地端口自动补齐 CORS origins”的生成逻辑。
- [x] 为后端运行时增加“本地 loopback origin 兜底放行”能力，避免未来再次因端口漂移导致整条链路失效。
- [x] 保持正式 `web` 部署默认仍以显式白名单为主，不把本地便利策略误带到公网部署口径。
- [x] 补最小回归测试，覆盖：
  - 自定义 `-FrontendPort` 时 `runtime/*.config.json` 自动写入实际端口
  - 本地 loopback origin 兜底已启用
- [x] 同步更新工具文档，明确说明本地部署的端口自适应行为。

## 影响范围

- `tools/latest/deploy-docker.ps1`
- `backend/app_factory.py`
- `backend/app/shared/infra/config/settings.py`
- `backend/tests/test_deploy_docker_script.py`
- `backend/tests/test_web_runtime_config.py`
- `backend/tests/test_web_app_factory.py`
- `tools/README.md`

## 风险

- 若 loopback 兜底规则定义过宽，可能把本地便利策略误扩散到正式 `web` 运行时。
- 若部署脚本直接覆盖 `cors_origins`，可能抹掉用户已有手工补充的 origin；应采用“补齐且去重”，而不是“整体替换”。
- 现有前端与后端都已默认假设 `8080`/`5173`，修改后需要回归确认不影响原端口场景。

## 验收标准

- `hybrid -FrontendPort 4173 -DeployLocalWeb` 生成的 `runtime/workstation.config.json` 与 linked `web` 的 `runtime/web.config.json` 均包含 `localhost/127.0.0.1:4173`。
- 浏览器从任意本地 loopback 前端端口访问 `workstation` / 本机 linked `web` 时，不再因 CORS 被拦截。
- 仍保留正式 `web` 部署的显式白名单策略；未额外放宽公网来源。
- 设置页不再因切换前端端口而长期停留“加载中…”，平台认证状态不再被误判为未启用。
