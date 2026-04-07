## ADDED Requirements

### Requirement: Frontend MUST Parse Structured Errors Before Falling Back to Raw Text

The frontend MUST parse structured backend error fields before falling back to raw text so user-visible errors are readable and stable across HTTP and long-running flows.

#### Scenario: HTTP error prefers structured message

- **WHEN** HTTP 响应返回统一错误 envelope
- **THEN** 前端必须优先读取结构化 `message`
- **AND** 不得把整个原始 JSON 直接展示给用户

#### Scenario: Frontend preserves compatibility with older detail-only responses

- **WHEN** 后端仍返回旧格式的 `detail` 字段或非 envelope 错误
- **THEN** 前端必须继续尽量提取可读错误文本
- **AND** 在无法提取结构化信息时提供统一兜底文案

### Requirement: Frontend MUST Provide a Consistent Fallback Message for Unknown Failures

Frontend presentation layers MUST provide a consistent fallback message for unknown failures instead of exposing implementation-oriented payloads directly to end users.

#### Scenario: Unknown error falls back to friendly message

- **WHEN** 前端拿到未知异常、空响应文本或不可解析错误对象
- **THEN** 前端必须展示统一的中文兜底文案
- **AND** 不得把 `"[object Object]"`、完整 JSON 字符串或空白错误直接展示给用户

#### Scenario: WebSocket error display uses shared parser

- **WHEN** 页面消费工作站长流程的 `error` 事件
- **THEN** 页面必须复用共享错误解析逻辑
- **AND** 不得为不同页面散落重复的 `detail/message/stringify` 分支
