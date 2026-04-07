## ADDED Requirements

### Requirement: Backends MUST Return a Stable Error Envelope for User-Facing Failures

The backend MUST expose a stable error envelope for user-facing HTTP failures so frontend clients can parse displayable messages without depending on raw JSON shape or Python exception text.

#### Scenario: Business error returns structured envelope

- **WHEN** 某个 HTTP 接口因为已知业务条件失败
- **THEN** 响应体必须包含稳定的错误对象
- **AND** 错误对象至少应提供 `code` 与 `message`
- **AND** 可以包含 `detail`、`request_id` 或其他受控扩展字段

#### Scenario: Unhandled exception uses safe fallback message

- **WHEN** 某个 HTTP 接口出现未捕获异常
- **THEN** 后端必须返回统一错误结构而不是框架默认格式
- **AND** 返回给用户的主文案不得直接暴露内部 traceback 或敏感实现细节

### Requirement: Workstation Long-Running Flows MUST Align Error Event Fields with the Shared Contract

Workstation WebSocket and long-running workflow error payloads MUST align with the shared error field set so frontend clients can reuse one parsing strategy across HTTP and long-running flows.

#### Scenario: Workflow error event contains displayable message

- **WHEN** 单资产、批量、构建部署、日志分析或 Mod 分析流程发出错误事件
- **THEN** 错误 payload 必须包含可直接展示给用户的 `message`
- **AND** 如有稳定错误码或补充信息，应使用与 HTTP 一致的字段名

#### Scenario: Legacy flow can incrementally adopt the shared fields

- **WHEN** 某条工作站流程尚未完全迁移到统一错误模型
- **THEN** 前端仍必须能从共享字段集合中读取错误主文案
- **AND** 不要求本轮同时重写非错误事件的整体协议
