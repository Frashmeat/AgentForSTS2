## Why

当前前端对 HTTP 与 WebSocket 错误缺少统一解析与展示协议，后端也没有统一错误出口，导致一部分页面会直接把原始 JSON 或底层异常文本暴露给用户。需要先建立一套前后端共享的错误契约，让后端稳定提供“可展示错误信息 + 调试信息槽位”，前端统一解析、展示并提供可靠兜底。

## What Changes

- 为后端新增统一错误响应契约，覆盖可预期业务错误与未捕获异常的输出格式。
- 为后端新增全局异常处理出口，统一 `HTTPException`、通用异常和工作站端关键运行时错误的响应结构。
- 为前端新增统一错误解析层，优先读取结构化字段，避免直接渲染原始 JSON。
- 为前端补充统一展示与兜底策略，确保 HTTP 与 WebSocket 错误都能得到一致的用户可见文案。
- 约定本 change 的落地顺序为：先统一错误契约，再优先接入 HTTP 主链路，随后让 WebSocket 错误事件对齐同一字段集合。

## Capabilities

### New Capabilities
- `runtime-error-contract`: 定义后端 HTTP/工作站长流程错误的统一字段、语义边界和兼容要求。
- `frontend-error-presentation`: 定义前端对结构化错误的解析、展示和兜底要求，避免向用户暴露原始 JSON。

### Modified Capabilities
- 无

## Impact

- 后端应用装配与全局异常处理
- 后端 API 路由层与关键工作站流程错误输出
- 前端 `shared/api/http.ts`、`shared/error.ts` 与错误展示入口
- 单资产、批量、构建部署、日志分析、用户中心等使用统一错误消息的页面/控制器
