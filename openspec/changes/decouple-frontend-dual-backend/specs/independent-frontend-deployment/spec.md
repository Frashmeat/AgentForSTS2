## ADDED Requirements

### Requirement: Deployment MUST Support One Independent Frontend Serving Both Backend Roles

The deployment model MUST support one independently hosted frontend serving both `workstation-backend` and `web-backend` while preserving clear responsibility boundaries.

#### Scenario: Static frontend deployment injects runtime endpoint configuration

- **WHEN** 前端以静态资源方式部署
- **THEN** 部署产物必须提供运行时方式注入 `workstation` 与 `web` 的 endpoint 配置
- **AND** 不要求通过重新构建前端源码才能切换后端地址

#### Scenario: Frontend template remains backend-agnostic

- **WHEN** 前端部署到不同环境
- **THEN** 静态站模板只负责托管前端资源与注入运行时配置
- **AND** 不得把工作台与 Web 的固定路由分流规则强耦合写死在前端模板中

### Requirement: Backends MUST Explicitly Allow Configured Frontend Origins

`workstation-backend` and `web-backend` MUST explicitly allow configured frontend origins for the HTTP capabilities they expose, and `workstation-backend` MUST also permit the corresponding WebSocket handshake origins for workstation flows.

#### Scenario: Independent frontend accesses web backend

- **WHEN** 独立前端向 `web-backend` 发起认证、用户中心或平台任务请求
- **THEN** `web-backend` 必须允许已配置的前端来源跨域访问

#### Scenario: Independent frontend accesses workstation backend

- **WHEN** 独立前端向 `workstation-backend` 发起工作台 HTTP 或 WebSocket 请求
- **THEN** `workstation-backend` 必须允许已配置的前端来源访问
- **AND** 第一版部署边界仅要求 `workstation-backend` 对本机或 LAN 可达

### Requirement: Web Session Flow MUST Remain Usable from the Independent Frontend

The web session flow MUST remain usable from the independent frontend for login, logout, session restore, and `/api/me/*` access within the first-phase deployment constraints.

#### Scenario: Login establishes a reusable session

- **WHEN** 用户在独立前端上登录成功
- **THEN** 后续从同一前端来源发起的会话查询和用户中心请求必须能够复用该会话

#### Scenario: Session mode stays within first-phase deployment constraints

- **WHEN** 部署环境超出第一版约束的同站或受控子域范围
- **THEN** 该环境必须被标记为超出当前 change 的正式支持范围
- **AND** 不得在本 change 中默认为其提供跨站认证保证
