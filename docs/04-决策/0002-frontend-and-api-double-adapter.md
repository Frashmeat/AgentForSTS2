# ADR 0002 — 前端方案与 API 双适配

| | |
| --- | --- |
| 状态 | Accepted |
| 决议日期 | 2026-05-11 |
| 详细背景 | [全栈重写计划 §3](../03-方案/全栈重写/进行中/2026-05-11-Rust+Tauri全栈重写计划.md) |

## Context

[ADR-001](./0001-rust-tauri-workspace-architecture.md) 已经决定 React/TS 前端。本 ADR 决定**同一份前端代码如何同时跑在 Tauri webview 和浏览器里**。

## Decision

### 选型

| 维度 | 选型 |
| --- | --- |
| UI 框架 | React 19 |
| 构建 | Vite 6 |
| 样式 | Tailwind 3 + CSS 变量 |
| 路由 | react-router-dom 7（HashRouter，Tauri / Web 通用） |
| 状态管理 | zustand 5 |
| 图标 | lucide-react |
| 类型 | TypeScript 5.6 |

### 双适配机制

Vite 的 `define` 在编译期注入 `__IS_TAURI__` 布尔常量；前端 `services/api.ts` 据此动态 import 不同实现：

```ts
// vite.config.ts
define: { __IS_TAURI__: JSON.stringify(!!process.env.TAURI_ENV_PLATFORM) }

// src/services/api.ts
const apiModulePromise: Promise<ApiModule> = __IS_TAURI__
  ? import("./tauriApi")
  : import("./webApi");
export const api = new Proxy({} as ApiModule, { /* 异步代理 */ });
```

`TAURI_ENV_PLATFORM` 由 `tauri dev` / `tauri build` 启动 Vite 时设置；纯 `vite build` 时为空 → 走 webApi。

### API parity 约定

- **`tauriApi.ts` 是签名权威** —— 所有 API 函数签名以它为准
- **`webApi.ts` 必须实现 `typeof TauriApi` 同名同签名的所有函数**（TS 编译期校验）
- 类型定义（请求 / 响应 model）放 `src/types/`（当前在 `services/tauriApi.ts` 内联，未来抽出），两端共用
- 长任务进度统一：
  - Tauri: `app_handle.emit_to(window, "xxx:progress", payload)` → `listen("xxx:progress", ...)`
  - Web: 返回 jobId → 前端开 WS `/ws/jobs/:id` 订阅
  - `api.ts` 暴露统一 `onJobProgress(jobId, cb)` 把差异收敛

### 路由约定

`HashRouter`（非 `BrowserRouter`），因为：
- Tauri webview 用自定义协议，无后端 fallback 路由
- 静态部署到 nginx / cdn 时 hash 路由不需要 server-side rewrite

## Alternatives considered

| 备选 | 拒绝理由 |
| --- | --- |
| Dioxus / Leptos | Rust SSR 框架，但生态比 React 弱十倍以上，组件库 / 工具链都要重做 |
| 单源 codebase 跑 Tauri，浏览器版纯弃 | 用户场景含 SaaS 网页版（未来），不能砍 |
| 两套独立前端 | 同样的 plan / 单资产 / batch 流程要写两次，违反 DRY |
| BrowserRouter | Tauri 不友好；静态部署 fallback 复杂 |

## Consequences

- ✅ 单源 codebase 跑两壳
- ✅ TS 类型系统强制 webApi.ts 与 tauriApi.ts 形状一致
- ✅ 业务逻辑层不需要感知运行环境
- ⚠️ 行为漂移风险：`webApi` 用 fetch（自然支持 abort signal），`tauriApi` 用 invoke（无 abort）。需要在 Proxy 层兜底
- ⚠️ 部分 Tauri 专属 API（如 `currentProject` / `auditAppend`）在 webApi 里返回 `desktopOnly` 错误，前端组件需要 `__IS_TAURI__` 守卫

## Stage 6 前端实现状态

| 页面 | 状态 |
| --- | --- |
| Dashboard（首页 8 卡片栈） | ✅ |
| ModEditor | ✅ |
| BatchGeneration | ✅ |
| LogAnalysis | ✅ |
| SingleAssetWorkflow（Card 形式） | ✅ |
| Auth / UserCenter / Admin | ⏳ 中转站方案后启动 |

## References

- 战略层文档 §3
- [Q1-Q10 决议汇总](../03-方案/全栈重写/进行中/2026-05-11-Q决议汇总.md)
