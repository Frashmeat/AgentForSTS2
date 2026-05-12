# 2026-05-11 — Rust + Tauri 全栈重写计划

| 项 | 值 |
| --- | --- |
| 分支 | `rust` |
| 起始日期 | 2026-05-11 |
| 当前阶段 | 2.4 — codegen prompt 装配完成；stage 3/4 待启动 |
| 参考骨架 | `E:/zuolan_lib/AI_Hub/AI-Session-Viewer` |
| 策略 | Greenfield，仓库根新建 `rust/` 子目录，与既有 `backend/` / `frontend/` / `tools/` 长期并存到 stage 8 一次性切换 |
| **执行计划** | [2026-05-11-Rust重写后续执行计划.md](./2026-05-11-Rust重写后续执行计划.md) ← 实时进度 + 剩余工作拆解（commit / LOC / 验收门） |

> 本文档是**战略层**：架构 ADR、模块映射、风险目录。日常推进按上面链接的"后续执行计划"走，那里有当前 commit 索引、子阶段拆解、决策清单与时间表。
>
> **路径说明**：本文档行文沿用规划时的 `rust/` 子目录前缀（如 `rust/Cargo.toml`、
> `rust/src-tauri/`）。实际仓库在 commit `3603911` 已平铺到根，所有 `rust/<x>`
> 在当前仓库就读作 `<x>`。下文保留原写法仅为还原设计时的上下文。

---

## 1. 总览

### 1.1 重写动机

项目目前是 Python (FastAPI + SQLAlchemy) backend + React (TS) frontend + PowerShell 部署脚本三栈，工作站/Web 双角色通过 `app_factory.create_app(role)` 在同份代码里用 if 分支区分。逻辑解耦但物理混杂——workstation 端被迫携带 Web 才需要的依赖（queue worker、credential admin 等），Web 端运行时还要 spawn 一个 workstation 子进程，结构嵌套。

整栈替换为 Rust + Tauri 的同时**物理解耦**双角色：

| 当前 | 重写后 |
| --- | --- |
| Python FastAPI（双角色 in-process） | Rust core + 两个独立可执行 |
| React/TS（共享 UI，根据 cookie/host 切角色） | React/TS（共享 UI，编译时 `__IS_TAURI__` 切 API 层） |
| PowerShell deploy/package/logs | Rust CLI（clap） |
| LLM via litellm（Python） | Rust 直连 Anthropic/OpenAI |
| rembg via ONNX（Python torch） | Rust 直调 ONNX Runtime（`ort` crate） |

### 1.2 架构（仿 AI-Session-Viewer）

```
                ┌──────────────────────────────────┐
                │       ats-core (Cargo crate)     │
                │  纯领域逻辑 + LLM + Image + DB    │
                │   无 axum / 无 tauri 依赖         │
                └──────────────┬───────────────────┘
                               │
                ┌──────────────┴───────────────────┐
                │                                  │
       ┌────────▼────────┐                ┌────────▼────────┐
       │   ats-web bin   │                │   src-tauri     │
       │   axum HTTP +   │                │   Tauri v2 +    │
       │   rust-embed    │                │  #[command] IPC │
       └────────▲────────┘                └────────▲────────┘
                │                                  │
                │  fetch / WS                      │  invoke()
                │                                  │
       ┌────────┴──────────────────────────────────┴────────┐
       │              src/ (React/TS frontend)              │
       │  services/api.ts → __IS_TAURI__ ? tauriApi : webApi│
       └────────────────────────────────────────────────────┘
```

### 1.3 两个壳的差异

| 维度 | ats-web | src-tauri |
| --- | --- | --- |
| 入口 | `cargo run -p ats-web` | `npx tauri dev` |
| 通信 | HTTP/WS + Bearer token | IPC，无网络 |
| 鉴权 | bearer + WS ticket | 同进程，无需鉴权 |
| 部署 | 单二进制 + 内嵌前端（rust-embed） | 桌面安装包（MSI/NSIS/DMG/DEB） |
| 长任务 | tokio task + WebSocket 推送 | tokio task + Tauri event emit |
| 文件系统 | 受限于服务进程权限 | 走 tauri-plugin-fs 调系统 |

### 1.4 目录约定（已搭好）

```
rust/
├── Cargo.toml                    # workspace 根
├── rust-toolchain.toml           # stable
├── package.json / vite.config.ts / tailwind.config.js / postcss.config.js
├── index.html / tsconfig.json
├── src/                          # React/TS 前端
│   ├── App.tsx / main.tsx / index.css / env.d.ts
│   └── services/
│       ├── api.ts                # Proxy 入口，按 __IS_TAURI__ 切换
│       ├── tauriApi.ts           # invoke() 实现
│       └── webApi.ts             # fetch + WS 实现
├── crates/
│   ├── ats-core/                 # 业务逻辑核心
│   ├── ats-web/                  # axum HTTP 服务器
│   └── ats-cli/                  # 部署/打包/日志 CLI
└── src-tauri/                    # Tauri v2 桌面壳
```

---

## 2. ADR-001 — 整体架构与技术栈

**状态**：已接受。

### 2.1 决策

采用 **core + 双壳 + 共享前端** 的 Cargo workspace。

| 维度 | 选型 | 备选 | 理由 |
| --- | --- | --- | --- |
| 工作空间布局 | 3 个 crate：`ats-core` + `ats-web` + `src-tauri`，另加 `ats-cli` 替代 PS1 | 多 core crate 按模块拆分 | 单 core 工作空间编译更快、依赖图清晰；模块边界靠 mod 而非 crate 保证。若编译时间或循环依赖出问题再拆。 |
| Rust 通道 | stable（rust-toolchain.toml 锁定） | nightly | 业务代码不需要 nightly 特性 |
| edition | 2024 | 2021 | 2024 在 stable，async 改进若干 |
| async runtime | tokio | async-std、smol | 生态默认，axum/sqlx/reqwest 都依赖 |
| Web 框架 | axum 0.8 | actix-web、warp、rocket | 与 tokio 同源、tower 生态、类型安全、稳定 |
| HTTP 客户端 | reqwest 0.12（rustls） | hyper 裸调 | 高层 API + 流式 + WebSocket |
| 数据库 | sqlx 0.8 + PostgreSQL | SeaORM、Diesel | sqlx 编译期类型检查、迁移工具自带、与 SQLAlchemy 解耦最干净 |
| 迁移工具 | sqlx migrate | refinery、Atlas | 与 sqlx 同源、一份 `.sql` 文件即可 |
| 序列化 | serde + serde_json | rkyv（性能场景） | 与外界 JSON 互通必需 |
| 错误 | thiserror（库层） + anyhow（binary 层） | snafu、eyre | 与社区一致 |
| 日志 | tracing + tracing-subscriber | log + env_logger | async 友好，axum 标配 |
| 配置 | figment | config-rs、自己解析 | TOML/JSON/env 多源合并 |
| 静态资源内嵌 | rust-embed | include_dir | 与 AI-Session-Viewer 一致 |
| CLI 参数 | clap 4 | argh、structopt | 与社区一致 |
| WebSocket | axum::extract::ws | tokio-tungstenite | axum 内置 |
| ONNX | ort 2.x | tract、candle | rembg 模型 ONNX 格式，ort 包装最完善 |

### 2.2 Tauri

- **Tauri v2**（与 AI-Session-Viewer 同版本）
- 插件：`tauri-plugin-shell`、`tauri-plugin-fs`、`tauri-plugin-dialog`、`tauri-plugin-process`、`tauri-plugin-updater`
- IPC 通信使用 `#[tauri::command]`，**不**在桌面端启动本地 HTTP
- App 状态通过 `.manage()` 注入，业务调用 `ats-core` 公开 API

### 2.3 双角色差异落点

| 关注点 | 落点 |
| --- | --- |
| 路由 / IPC 命令定义 | `ats-web/src/routes/*.rs` 和 `src-tauri/src/commands/*.rs` 平行编写，调用同一份 `ats-core` |
| 鉴权 | `ats-web/src/auth.rs`（Bearer + WS Ticket）；`src-tauri` 同进程无需 |
| 长任务进度推送 | Web: WebSocket；Tauri: `emit_to`/`emit_all` |
| 文件系统访问 | Web: 受服务进程权限限制；Tauri: 通过 plugin-fs/dialog 走系统 API |
| 队列 worker（platform） | 只在 `ats-web` 启动 |
| Image postprocess prewarm | 只在 `src-tauri` 启动（用户侧本地化） |
| 数据库 | Web 必需；Tauri 可选——见未决问题 Q1 |

### 2.4 备选与放弃理由

- **保留 Python，只把瓶颈模块改成 Rust**：用户明确要求整栈重写，跳过。
- **多 core crate（按模块拆分）**：增加 workspace 复杂度，单 core 通过模块边界已能强制解耦。等真出现编译/依赖问题再拆。
- **frontend 用 Dioxus/Leptos**：用户在采用本骨架后改回 React/TS（与骨架一致），见 §3。
- **桌面端也跑本地 axum**：会引入 localhost 端口冲突、防火墙弹窗、双份鉴权代码、CORS 配置。Tauri IPC 是同进程函数调用，无这些问题。

---

## 3. ADR-002 — 前端方案与 API 双适配

**状态**：已接受。

### 3.1 选型

| 维度 | 选型 |
| --- | --- |
| UI 框架 | React 19 |
| 构建 | Vite 6 |
| 样式 | Tailwind 3 + CSS 变量 |
| 路由 | react-router-dom 7 |
| 状态管理 | zustand 5 |
| 图标 | lucide-react |
| 类型 | TypeScript 5.6 |

### 3.2 双适配机制

核心：Vite 的 `define` 在编译期注入 `__IS_TAURI__` 布尔常量，前端 `services/api.ts` 据此动态 import 不同实现。

```ts
// vite.config.ts
define: {
  __IS_TAURI__: JSON.stringify(!!process.env.TAURI_ENV_PLATFORM),
}

// src/services/api.ts
const apiModulePromise: Promise<ApiModule> = __IS_TAURI__
  ? import("./tauriApi")
  : import("./webApi");

export const api = new Proxy({} as ApiModule, { /* 异步代理 */ });
```

`TAURI_ENV_PLATFORM` 由 `tauri dev` / `tauri build` 启动 Vite 时设置；纯 `vite build`/`npm run build:web` 时为空，得 `false`。

### 3.3 约定

- `tauriApi.ts` 是**签名权威**——所有 API 函数签名以它为准
- `webApi.ts` 必须实现 `typeof TauriApi` 同名同签名的所有函数（TS 编译期校验）
- 类型定义（请求/响应 model）放 `src/types/`，两端共用
- 长任务进度：
  - Tauri: `app_handle.emit_to(window, "xxx:progress", payload)` → `listen("xxx:progress", ...)`
  - Web: 返回 jobId → 前端开 WS `/ws/jobs/:id` 订阅
  - `api.ts` 暴露统一 `onJobProgress(jobId, cb)` 把差异收敛

### 3.4 与 AI-Session-Viewer 偏差

| 点 | AI-Session-Viewer | 本项目 |
| --- | --- | --- |
| zustand store 数量 | 2 | 估计 5+（platform、knowledge、planning、auth、settings...），按 feature 切分 |
| 路由 | 单页扁平 | 多层（admin/、user-center/、batch-generation/ 等） |
| 双数据源 | source=claude/codex 参数 | 双角色用编译期切换，无需运行期 source 参数 |
| 自动更新 | tauri-plugin-updater | Stage 7 决定，暂不接 |

---

## 4. 模块映射表

口径：1:1 映射为主，必要时合并/拆分。所有 Python 代码最终归宿到 `ats-core`、`ats-web`、`src-tauri`、`ats-cli` 四个 crate 之一。

### 4.1 backend/app/modules/ → ats-core/src/

| Python 模块 | Rust 落点 | 备注 |
| --- | --- | --- |
| `platform/contracts/` | `ats-core::platform::contracts` | DTO/命令/查询，serde 序列化 |
| `platform/domain/` | `ats-core::platform::domain` | 实体 + repository trait |
| `platform/application/services/` | `ats-core::platform::services` | use case |
| `platform/application/workstation_*` | `ats-core::platform::workstation` | workstation 子进程封装；Tauri 端用不到（直接同进程调用），Web 端保留 |
| `platform/infra/persistence/models/` | `ats-core::platform::infra::models` | sqlx::FromRow |
| `platform/infra/persistence/repositories/` | `ats-core::platform::infra::repositories` | sqlx 实现 |
| `platform/runner/` | `ats-core::platform::runner` | job handler 集合 |
| `platform/errors/` | `ats-core::platform::errors` | thiserror |
| `knowledge/application/` | `ats-core::knowledge::application` | facade |
| `knowledge/infra/` | `ats-core::knowledge::infra` | runtime + provider |
| `knowledge/templates/sts2/` | `ats-core/templates/sts2/` | 资源文件（`include_str!`） |
| `planning/application/` | `ats-core::planning::application` | services + bundles + graph |
| `planning/domain/models.py` | `ats-core::planning::domain::models` | |
| `codegen/application/prompt_assembler.py` | `ats-core::codegen::prompt_assembler` | |
| `build/` | `ats-core::build` | |
| `image/` | `ats-core::image_proc` | rembg via ort，**重命名避免与 `image` crate 冲突** |
| `approval/` | `ats-core::approval` | |
| `auth/` | `ats-core::auth` | JWT/session |
| `workflow/` | `ats-core::workflow` | |

### 4.2 backend/app/shared/ → ats-core/src/

| Python | Rust |
| --- | --- |
| `shared/infra/config/settings.py` | `ats-core::config::Settings` |
| `shared/infra/http_errors.py` | 拆：error 类型放 `ats-core::errors`，HTTP 转换放 `ats-web::error` |
| `shared/kernel/` | `ats-core::kernel` |
| `shared/prompting/` | `ats-core::prompting` |
| `shared/resources/prompts/*.md` | `ats-core/prompts/*.md`（include_str!） |

### 4.3 backend/llm 和 backend/image → ats-core

| Python | Rust |
| --- | --- |
| `backend/llm/text_runner.py` | `ats-core::llm` |
| `backend/image/postprocess.py` | `ats-core::image_proc::postprocess` |
| `backend/agents/sts2_guidance.py` | `ats-core::knowledge::sts2_guidance` |
| `backend/project_utils.py` | `ats-core::project_utils` |

### 4.4 backend/routers/ → ats-web/src/routes/ + src-tauri/src/commands/

每个 router 拆成两份**平行**实现，调用同一份 `ats-core` 服务：

| Python router | ats-web | src-tauri |
| --- | --- | --- |
| `approval_router.py` | `routes::approval` | `commands::approval` |
| `auth_router.py` | `routes::auth` | （桌面端无登录） |
| `batch_workflow.py` | `routes::batch_workflow` | `commands::batch_workflow` |
| `build_deploy.py` | `routes::build_deploy` | `commands::build_deploy` |
| `config_router.py` | `routes::config` | `commands::config` |
| `knowledge_router.py` | `routes::knowledge` | `commands::knowledge` |
| `log_analyzer.py` | `routes::log_analyzer` | `commands::log_analyzer` |
| `me_router.py` | `routes::me` | （桌面端无登录） |
| `mod_analyzer.py` | `routes::mod_analyzer` | `commands::mod_analyzer` |
| `platform_admin.py` | `routes::platform_admin` | （桌面端不暴露 admin） |
| `platform_jobs.py` | `routes::platform_jobs` | `commands::platform_jobs` |
| `workflow.py` | `routes::workflow` | `commands::workflow` |
| `workstation_capabilities.py` | `routes::workstation_capabilities` | `commands::workstation_capabilities` |
| `workstation_platform.py` | `routes::workstation_platform`（远端 workstation 客户端代理） | （桌面端直接走 IPC） |

### 4.5 backend/main.py / app_factory.py → 两个 binary 的 main

| Python | Rust |
| --- | --- |
| `app_factory._create_base_app` | 拆到 `ats-web::main` 和 `src-tauri::lib::run` |
| `_register_web_queue_worker_lifecycle` | `ats-web::main` 启动期 `tokio::spawn` |
| `_register_web_workstation_runtime_lifecycle` | `ats-web::main` 启动期 spawn 子进程（若保留远端模式） |
| `_register_workstation_image_postprocess_lifecycle` | `src-tauri::lib::run` setup 钩子内 spawn |
| `_mount_frontend` | `ats-web::static_files` 用 rust-embed |

### 4.6 backend/migrations/ → ats-core/migrations/

| Python (alembic) | Rust (sqlx) |
| --- | --- |
| `versions/*.py` 各迁移 | `migrations/{timestamp}_{name}.sql` |
| alembic env/script | 删除，由 `sqlx::migrate!()` 宏接管 |

**注意**：schema 不变以便切换时直接复用现有 PostgreSQL 实例。所有 `.sql` 从最终态 dump 重新生成，不还原 alembic 历史。

### 4.7 tools/latest/ → ats-cli/src/

| PowerShell | Rust |
| --- | --- |
| `deploy-app.ps1` | `ats-cli::deploy` |
| `package-app.ps1` | `ats-cli::package` |
| `logs-app.ps1` | `ats-cli::logs` |
| `stop-app.ps1` | `ats-cli::stop` |
| `templates/compose.app.yml` | `ats-cli/templates/compose.app.yml`（include_str!） |
| `tools/stop/kill-local.ps1` | `ats-cli::stop::kill_local` |
| `tools/tools.ps1` | `ats-cli::main` 顶层调度 |

### 4.8 frontend/src/ → rust/src/

| 现 frontend | 新 rust/src |
| --- | --- |
| `App.tsx` / `main.tsx` / `index.css` | 同名 |
| `components/` | `components/`（重写，保留 props 结构） |
| `features/` | `features/` |
| `pages/` | `pages/` |
| `shared/api/*.ts` | `services/`（拆为 api.ts + tauriApi.ts + webApi.ts） |
| `shared/types/workflow.ts` | `types/` |

### 4.9 不迁移

- `.trellis/` — Trellis 工作流配置，留在 `main`
- `docs/` 历史归档目录（`docs/90-归档/` 等） — 不动

---

## 5. 迁移路线图

总原则：rust 分支与 main 长期并存。每个阶段产物在 rust 分支可独立编译、可运行（即使功能不全），不阻塞 Python 端开发。

### 阶段 0 — 骨架 + 设计文档（本计划完成时）

**产物**（已落地）：
- `rust/Cargo.toml` workspace 根
- `crates/ats-core/`、`crates/ats-web/`、`crates/ats-cli/`、`src-tauri/` 四 crate 最小 stub
- `rust/src/`、`vite.config.ts`、`package.json` 前端骨架
- `rust/src-tauri/tauri.conf.json`
- 本计划文档

**验收**：
- `cd rust && cargo check --workspace` 通过
- `cd rust && npm install && npm run build` 通过

### 阶段 1 — 端到端最小通路（≈1 周）

目标：一个最简模块（health + 配置读取）同时在 Tauri 桌面和 Web 浏览器跑通。

产物：
- `ats-core::config::Settings` 实现（figment 多源加载）
- `ats-web` 暴露 `GET /api/health`
- `src-tauri` 暴露 `get_health` command
- 前端 `src/services/api.ts` + `tauriApi.ts` + `webApi.ts` 三件齐
- 一个最简前端页面展示 health 数据

### 阶段 2 — knowledge / planning / codegen（≈3 周）

顺序（依赖从弱到强）：
1. `knowledge` — 文件读取 + 模板渲染
2. `planning` — 依赖 knowledge
3. `codegen` — 依赖 planning + knowledge

每个模块完整迁移包含：contracts、application service、infra provider、单元测试、两壳 API、前端 feature 页。

### 阶段 3 — platform（最大，≈6 周）

子阶段（按依赖顺序）：
1. **DB 层**：sqlx 连接池、`ats-core::db`、migrations 从当前 schema 重新生成、`cargo sqlx prepare`
2. **domain + contracts**：所有实体、repository trait
3. **infra/repositories**：sqlx 实现 + 单测
4. **application/services**：use case 层（job_application_service、execution_orchestrator、server_credential_admin...）
5. **runner 模块**：长任务 handler，Web 端 tokio task，Tauri 端 emit 推送
6. **routers/commands**：两壳并行暴露

### 阶段 4 — LLM / image-proc（≈2 周）

- `ats-core::llm`：Anthropic Messages API、OpenAI Chat Completions API、流式响应（reqwest stream + tokio mpsc）、重试/速率限制
- `ats-core::image_proc`：rembg ONNX 模型（u2net）通过 `ort` 加载、Tauri startup 预热钩子

### 阶段 5 — Workstation 与 Web binary 装配完成（≈1 周）

- `src-tauri::lib::run` 注册所有 command
- `ats-web::main` 注册所有 route + middleware（auth、CORS、error）
- `ats-web` rust-embed 内嵌前端 dist
- 两端冒烟测试

### 阶段 6 — Frontend 全部页面迁移（≈4 周，与 2-5 部分并行）

- features/auth、batch-generation、log-analysis、mod-editor、platform-run、single-asset、user-center、workspace
- pages/admin/* (Audit, CredentialHealth, ExecutionProfiles, Executions, KnowledgePacks, Layout, Overview, Refunds, Runtime, ServerCredentials, Users)
- pages/BatchMode、SettingsPage、AdminRuntimeAuditPage
- 共享组件 + zustand store

### 阶段 7 — Deploy CLI（≈1 周）

- `ats-cli::deploy` 替代 deploy-app.ps1（docker compose 调度）
- `ats-cli::package` 替代 package-app.ps1
- `ats-cli::logs` 替代 logs-app.ps1
- `ats-cli::stop` 替代 stop-app.ps1
- 编译时内嵌 compose.app.yml 模板

### 阶段 8 — 切换 main、归档 Python（1 天）

1. 在 `rust` 分支冒烟所有路径
2. 给 Python 代码打 tag `python-final`
3. `git mv rust/* .` + 删除 `backend/` `frontend/` `tools/`
4. 合并 `rust` → `main` 通过 PR
5. CI 切换到 cargo + npm 流水线

### 时间合计

约 **18 周** 单人全职。考虑兼职/学习成本，**6 个月** 是更现实目标。可压缩方向：跳过 admin 页面、跳过部分长尾 router、保留部分 PowerShell 脚本不重写。

---

## 6. 风险与未决问题

### 6.1 风险

#### R1 — 工期估算乐观
**问题**：路线图给出 18 周（单人全职），但 Rust 学习成本 + 项目复杂度 + 兼职现实，落地可能 6-12 个月。
**影响**：切换 main 前 Python 端持续接收业务变更，rust 分支需周期性 rebase，diff 越拖越大。
**缓解**：每完成一个阶段都合并一次（通过 PR 把对应模块挪到 main 子目录，逐步压缩 diff）。

#### R2 — LLM 生态弱
**问题**：Python litellm 一个库切多家厂商；Rust 端 Anthropic 没有官方 SDK，OpenAI 有 `async-openai` 但其它厂商各自为政。
**缓解**：先只支持 Anthropic + OpenAI（实际在用的），其它厂商等真用到再说。流式响应是重点测试场景。

#### R3 — rembg / ONNX 加载
**问题**：rembg Python 包内部下载 u2net 模型；Rust `ort` 需要自己处理：模型文件分发（嵌入 vs 首启下载）、ONNX Runtime 动态库（Windows 需 onnxruntime.dll 同目录或环境变量）。
**影响**：桌面安装包体积可能 30MB → 200MB（含模型）；首启延迟（若选下载）。
**缓解**：先用首启下载方案（小安装包 + 模型缓存在 `app_data_dir`），失败再切内嵌。

#### R4 — Tauri v2 + Windows + WebView2
**问题**：WebView2 在 Win7/8 不可用；Win11 自带；Win10 需 Edge runtime。
**缓解**：可接受。tauri.conf.json 配 `webviewInstallMode = "downloadBootstrapper"`，安装时自动拉。

#### R5 — sqlx 编译期检查 vs 离线开发
**问题**：sqlx 宏 `query!` 在编译期连数据库验证查询。无 DB 环境（如 CI）需要 `cargo sqlx prepare` 生成 `.sqlx/` 缓存。
**缓解**：CI/本地脚本接入 prepare 步骤，缓存文件提交到仓库。

#### R6 — Tauri 与 Web 行为漂移
**问题**：`tauriApi.ts` 和 `webApi.ts` 双实现，签名 TS 能保证，**行为差异**靠人盯。例如 Web 端 fetch 自然支持 abort signal，Tauri invoke 不支持。
**缓解**：① 写对比测试套件，同脚本驱动两端跑相同断言；② ADR 明确"签名权威是 tauriApi"，差异向 Web 端能力对齐。

#### R7 — 鉴权双模型
**问题**：Web 端有用户登录/JWT/me_router；Tauri 端单用户无登录。前端会出现 `if (__IS_TAURI__)` 跳过登录页/admin 入口。
**缓解**：前端在 `App.tsx` 路由层用 `__IS_TAURI__` 区分挂载哪些路由；feature 内部不再判断。

#### R8 — pytest 测试迁移成本高
**问题**：`backend/tests/` 数千行 pytest（含 fixture、mock、SQLite-in-memory）。
**缓解**：先迁数据库 schema 测试 + 端到端冒烟，单元测试覆盖率短期接受倒退。

#### R9 — 中文路径
**问题**：`backend/project_utils.py` 处理 Windows 中文路径有微妙逻辑；Rust 端 PathBuf 通常 OK 但 stringify 时要小心。
**缓解**：阶段 1 health 模块先跑通中文路径读写。

#### R10 — workspace 编译时长
**问题**：rust workspace 全量编译几分钟起步；CI 缓存策略要早规划。
**缓解**：① sccache；② cargo nextest；③ feature flag 控制可选依赖（如 `ort` 默认 off）。

### 6.2 未决问题

按是否阻塞下一阶段排序。

#### Q1 — 桌面端是否需要数据库？✅ 已决（2026-05-11）
- **决议**：**B+ "工程文件夹"模式** —— 桌面端无 DB，每个 mod 项目是一个自包含目录（`project.json` / `plan.json` / `items/` / `artifacts/` / `history/` / `.ats/`），可整体 zip / git / 复制粘贴。App 级状态（凭据 / recent_projects / 反编译知识缓存）落在 OS app data dir。Web 端继续 Postgres。
- **影响**：Stage 3.1 拆双轨：3.1a Web sqlx + 3.1b Desktop 工程文件夹存储；Stage 3.3 Repository 实现双轨（sqlx + file）；Stage 6 前端新增"项目管理"页（新建/打开/最近列表）。
- **取舍**：相比 SQLite 多了 ~3 天工作量，换取桌面端零 DB 依赖 + 项目可整体携带。
- **详见**：[后续执行计划 §2.3.0 工程文件夹布局规范](./2026-05-11-Rust重写后续执行计划.md#230-工程文件夹布局规范)。

#### Q2 — 桌面端是否暴露 admin/management 路由？
当前 Web 端有完整 admin 子站（用户管理、credential 健康、refund 等）。桌面端是单用户工具，admin 概念可能不存在。决定哪些 router 要在 src-tauri 注册命令、哪些前端路由在 `__IS_TAURI__` 下隐藏。

#### Q3 — 工作站对外暴露 HTTP 给 Web 端的能力是否保留？
当前 `workstation_runtime_service` 让 Web 端可以 spawn 一个 workstation 子进程并 HTTP 调用它（远端编译/打包/资产生成）。Tauri 化后桌面壳没有 HTTP server，**这条路被切断**。
- **A**：Web 端不再依赖 workstation runtime，自己装齐 LLM/ONNX（要 GPU）。
- **B**：保留 headless 模式——`ats-web` 二进制带 `--mode workstation` 启动只跑 workstation worker，Web 主服务通过 HTTP 调它。
- **C**：把"远端 workstation"概念彻底删掉，所有任务在 Web 服务进程内跑。
- **目前倾向**：B（最小破坏）。

#### Q4 — 前端 zustand store 结构是否重设计？
现 `frontend/src/` store 分散在 features/、shared/。新结构建议集中 `src/stores/` 按 feature 切。决定迁移时 store API 是否完全重设计（一次性痛快）vs 镜像现有形状（迁移轻）。

#### Q5 — `runtime/agentthespire.config.example.json` schema 是否保留？
现配置 schema 与 `Settings.from_dict` 强耦合。Rust 端 figment 可兼容，但字段命名（snake_case vs camelCase）、嵌套深度要决定是否调整。**目前倾向**：保留字段名以便用户原配置文件直接可用。

#### Q6 — CI/CD 改造时机
目前 CI 跑 pytest + frontend vitest + ts type check。Rust 端要加 `cargo check/clippy/test`、`cargo sqlx prepare` 校验、`npm tsc --noEmit`。阶段 1 完成后是否立刻接 CI？建议是。

#### Q7 — 桌面端打包签名与公证
Windows MSI 需代码签名证书（避免 SmartScreen 拦截）。macOS 需 Apple Developer ID + 公证。阶段 8 前评估。若无签名，先发不签名版本可接受。

#### Q8 — 是否保留 Tauri 自动更新
AI-Session-Viewer 用 `tauri-plugin-updater`。阶段 8 后单独评估，目前不接。

---

## 7. 验收与下一步

### 7.1 阶段 0 验收（本次会话产物）

- [x] 建 `rust` 分支
- [x] 设计文档（本文件）
- [x] Cargo workspace 4 个 crate stub
- [x] 前端 Vite + React 骨架 + 双适配 services
- [x] Tauri v2 配置
- [ ] **待用户**：`cd rust && cargo check --workspace` 通过
- [ ] **待用户**：`cd rust && npm install && npm run build` 通过
- [ ] **待用户**：`cd rust && npx tauri dev` 能起窗口看到 health 显示

### 7.2 阶段 1 入口

阶段 1 启动前需先解 Q1、Q3 两个未决问题（数据库形态、远端 workstation 是否保留），其余可在过程中决定。

阶段 1 候选第一个完整模块：`config` + `health`（即把 stage 0 的 stub 完整实现 + 加 figment 多源配置加载）。
