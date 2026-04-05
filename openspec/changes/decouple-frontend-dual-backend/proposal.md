## Why

当前前端代码已经具备“同一个 SPA 同时承接工作台页面与平台页面”的结构，但部署形态仍默认依赖 `workstation-backend` 托管前端壳，导致独立静态前端站点无法稳定同时访问工作台与 Web 平台能力。需要把“单前端入口”从“单一托管入口”中解耦出来，形成可独立部署、可显式分流的前端接入模型。

## What Changes

- 将前端访问后端的方式从“工作台默认同源、Web 显式分流”调整为“工作台与 Web 都支持显式运行时目标地址”。
- 为工作台 WebSocket 长连接引入独立于 `location.host` 的运行时地址解析规则，避免独立前端站点误连自身来源。
- 为独立前端部署补充运行时配置注入、跨域允许清单与部署模板约束。
- 明确用户分发形态采用“单安装包 + 两个本地进程”，同一发行物内包含本地静态前端服务、`workstation-backend` 与启动器。
- 明确第一版边界：
  - 独立前端站点可以同时访问 `web-backend` 与 `workstation-backend`
  - `workstation-backend` 第一版限定为本机或 LAN 可达
  - 不把“公网前端稳定直连任意用户本机 workstation”纳入第一版目标

## Capabilities

### New Capabilities

- `frontend-runtime-endpoints`: 前端在运行时显式解析 workstation 与 web 的 HTTP/WS 入口，并按能力边界分流请求。
- `independent-frontend-deployment`: 独立静态前端部署形态下，前端、workstation-backend、web-backend 之间具备明确的配置、跨域和会话接入约束。

### Modified Capabilities

- 无

## Impact

- 前端基础访问层与 WebSocket 客户端
- 工作台工作流、分析、构建部署等使用同源假设的调用点
- 后端 `runtime.*.cors_origins` 与认证会话策略
- Docker / 静态站部署模板与运行时配置注入方式
- 用户发行包结构、本地启动器与进程编排脚本
