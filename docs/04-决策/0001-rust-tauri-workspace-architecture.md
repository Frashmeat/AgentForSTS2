# ADR 0001 — Rust + Tauri 整体架构与技术栈

| | |
| --- | --- |
| 状态 | Accepted |
| 决议日期 | 2026-05-11 |
| 取代 | — |
| 详细背景 | [全栈重写计划 §2](../03-方案/全栈重写/进行中/2026-05-11-Rust+Tauri全栈重写计划.md) |

## Context

项目原为 Python (FastAPI + SQLAlchemy) + React (TS) + PowerShell 三栈。双角色（workstation / web）通过 `app_factory.create_app(role)` 同份代码 if 分支区分，逻辑解耦但物理混杂。决定整栈替换为 Rust + Tauri，同时**物理解耦**双角色。

## Decision

采用 **core + 双壳 + 共享前端** 的 Cargo workspace：

```
ats-core (业务逻辑核心，无 axum/tauri 依赖)
   ├── ats-web bin    (axum HTTP + rust-embed)
   └── src-tauri      (Tauri v2 + #[command] IPC)
       (前端 React/TS 共享一份 src/，编译期 __IS_TAURI__ 切 API 层)
```

技术栈关键选型：

| 维度 | 选型 | 备选 / 理由 |
| --- | --- | --- |
| 工作空间 | 3 crate（core/web/cli + src-tauri） | 单 core 编译快、依赖图清晰；mod 边界保证解耦 |
| Rust 通道 | stable | nightly 业务用不到 |
| edition | 2024 | 2024 stable 通道支持，async 改进 |
| async | tokio | axum/sqlx/reqwest 都依赖它 |
| Web 框架 | axum 0.8 | 与 tokio 同源、tower 生态 |
| HTTP | reqwest 0.12 (rustls) | 高层 API + 流式 + WebSocket |
| 数据库 | sqlx 0.8 + PostgreSQL | 编译期类型检查 |
| 序列化 | serde + serde_json | 互通必需 |
| 错误 | thiserror（库）+ anyhow（bin） | 社区一致 |
| 日志 | tracing + tracing-subscriber | async 友好 |
| 配置 | figment | 多源合并 |
| 内嵌资源 | rust-embed | AI-Session-Viewer 同款 |
| CLI 参数 | clap 4 | 社区一致 |
| WebSocket | axum::extract::ws | axum 内置 |
| 桌面壳 | Tauri v2 | 与 AI-Session-Viewer 同版本，不本地 HTTP |
| ONNX | ort 2.x | rembg 后续接 |

## Tauri v2 关键约定

- 插件：`tauri-plugin-shell` / `-fs` / `-dialog` / `-process` / `-updater`
- IPC 用 `#[tauri::command]`，**不**在桌面端启动本地 HTTP
- App state 通过 `.manage()` 注入

## 双角色差异落点

| 关注点 | Web 轨 | Desktop 轨 |
| --- | --- | --- |
| 入口 | `cargo run -p ats-web` | `npx tauri dev` |
| 通信 | HTTP/WS + Bearer | IPC，无网络 |
| 鉴权 | bearer + WS ticket | 同进程无需 |
| 长任务推送 | WebSocket | `emit_to` / `emit_all` |
| 文件系统 | 服务进程权限受限 | tauri-plugin-fs 走系统 |
| 队列 worker | 在 ats-web 启动 | 无 |
| 数据库 | sqlx + Postgres | 工程文件夹（[ADR-002](./0002-frontend-and-api-double-adapter.md) 未涵盖；见 Q1 决议） |

## Alternatives considered

- **保留 Python，只把瓶颈模块改 Rust**：跳过，用户明确整栈重写。
- **多 core crate**：增加 workspace 复杂度。单 core + mod 边界已足够。
- **frontend 用 Dioxus / Leptos**：用户最终选 React/TS（ADR-002）。
- **桌面端也跑本地 axum**：localhost 端口冲突、防火墙弹窗、双份鉴权、CORS。Tauri IPC 是同进程函数调用，无这些问题。

## Consequences

- ✅ 双壳逻辑物理解耦；ats-core 测试不依赖任何壳
- ✅ Tauri 桌面端无网络栈，无 CORS / 鉴权代码
- ⚠️ workspace 编译时间约 2-3 分钟全量；需要 sccache / nextest 加速
- ⚠️ LLM 生态较 Python litellm 弱：只支持 Anthropic + OpenAI 双协议（自带轮子）
- ⚠️ sqlx 编译期检查在 CI 需要 PG 实例或 `.sqlx/` offline 缓存

## References

- 战略层文档 §2、§4 模块映射
- AI-Session-Viewer 参考骨架：`E:/zuolan_lib/AI_Hub/AI-Session-Viewer`
