# Error Handling

> How errors are handled in this project.

---

## Overview

本项目目前没有全局统一的 REST 响应包裹层。HTTP 接口仍以 FastAPI 原生返回为主；WebSocket 工作流使用事件流表达进度、完成、失败和取消。

取消不是系统失败。工作站 WebSocket 与 Web 端平台任务都应把用户主动取消归入可预期终止，避免继续占用本地 CLI 进程、外部 AI key 或平台任务额度。

---

## Error Types

- `DomainError`：业务域错误基类，携带 `code/message/detail/status_code`。
- `WorkflowTermination`：工作流可预期终止错误，用于用户取消、客户端断开等非系统失败场景。
- `user_cancelled()`：生成 `code = "user_cancelled"` 的 `WorkflowTermination`。
- `client_disconnected()`：生成 `code = "client_disconnected"` 的 `WorkflowTermination`。
- `HTTPException`：仍是多数 HTTP 路由的直接错误返回方式。

---

## Error Handling Patterns

- WebSocket 工作流执行长耗时步骤时，应包裹为可取消任务；收到 `{"action":"cancel"}` 或连接断开时，应取消当前协程。
- 调用 Code Agent CLI 的 runner 在协程取消时必须终止子进程；Windows 下使用进程树终止，避免强退后 key 仍被后台进程继续使用。
- `WorkflowTermination` 应转换为 `cancelled` 事件，不应落入通用 `error` 事件或错误日志告警。
- 普通异常仍转换为 `error` 事件，并尽量携带 `code/message/traceback` 以便前端统一解析。
- 前端识别 `user_cancelled` 与 `client_disconnected` 时，应进入取消态，不展示为“执行失败”。
- Web 服务器模式调用外部模型时，上游内容安全或网关阻断应归类为明确错误码，例如 `upstream_request_blocked`，不要把 `litellm.APIError` / SDK 原始异常直接展示为最终用户错误。
- 面向游戏 Mod 生成的 Prompt 若包含“伤害 / 攻击 / 毒”等游戏机制词，应明确这些词属于虚构电子游戏内的数值规则，降低上游网关误判概率。

---

## API Error Responses

HTTP 当前返回风格：

- 成功：直接返回 JSON 对象或数组。
- 常见失败：FastAPI `HTTPException`，形如 `{"detail": "..."}`。
- 少数旧接口：可能返回 `200 + {"error": "..."}`，后续新接口不应继续扩散这种模式。

WebSocket 当前公共事件：

```json
{
  "event": "error",
  "stage": "error",
  "code": "optional_error_code",
  "message": "面向用户或开发者的错误说明",
  "traceback": "可选"
}
```

取消事件：

```json
{
  "event": "cancelled",
  "stage": "cancelled",
  "code": "user_cancelled",
  "message": "已取消当前生成"
}
```

客户端主动取消 WebSocket 工作流时发送：

```json
{
  "action": "cancel"
}
```

---

## Common Mistakes

- 把用户取消当作系统失败展示，导致用户误以为生成异常。
- 只关闭 WebSocket，不取消后端协程或 CLI 子进程，导致 key 仍被后台请求使用。
- 在长耗时步骤外层缺少取消检查，导致收到取消后仍继续执行后续生成、构建或审批步骤。
- HTTP 与 WebSocket 错误载荷字段不一致，前端只能靠字符串解析错误原因。
- 外部 AI 网关阻断时只记录原始 SDK 异常，导致用户看到不可行动的 `Your request was blocked`，也无法区分配置错误、内容误判和系统异常。
