## ADDED Requirements

### Requirement: Frontend MUST Resolve Workstation and Web Endpoints Independently at Runtime

The frontend MUST resolve `workstation` and `web` endpoint targets independently at runtime and MUST route HTTP requests according to capability boundaries instead of defaulting to the current page origin.

#### Scenario: Independent frontend routes platform requests to web backend

- **WHEN** 前端由独立静态站点托管，且运行时提供了 `web` API 入口
- **THEN** 认证、用户中心和平台任务相关请求必须发送到 `web-backend`
- **AND** 这些请求不得回退到前端静态站点来源

#### Scenario: Independent frontend routes workstation HTTP requests to workstation backend

- **WHEN** 前端由独立静态站点托管，且运行时提供了 `workstation` API 入口
- **THEN** 工作台工作流、配置、日志分析、Mod 分析和构建部署相关请求必须发送到 `workstation-backend`
- **AND** 这些请求不得默认发送到前端静态站点来源

#### Scenario: Compatibility mode still supports same-origin workstation hosting

- **WHEN** 前端由 `workstation-backend` 或兼容态入口同源托管
- **THEN** 工作台请求可以继续使用同源访问
- **AND** 平台接口仍可以独立解析到 `web-backend`

### Requirement: Frontend MUST Resolve Workstation WebSocket Endpoints Explicitly

The frontend MUST resolve workstation WebSocket targets through a unified runtime rule and MUST NOT derive workstation WebSocket connections directly from `location.host` in independent-frontend mode.

#### Scenario: Independent frontend opens workflow socket against workstation backend

- **WHEN** 用户从独立前端启动单资产、批量、日志分析、Mod 分析或构建部署等长流程
- **THEN** 前端必须使用配置的 `workstation` WebSocket 入口建立连接
- **AND** 不得把 WebSocket 建立到前端静态站点来源

#### Scenario: Missing workstation endpoint fails loudly

- **WHEN** 前端运行在独立部署模式且缺少 `workstation` HTTP 或 WS 入口配置
- **THEN** 前端必须向用户报告工作台连接配置错误
- **AND** 不得静默退回错误来源继续请求
