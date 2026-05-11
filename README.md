# AgentTheSpire — Rust 重写（rust 分支）

[![Rust CI](https://github.com/Frashmeat/AgentForSTS2/actions/workflows/rust-ci.yml/badge.svg?branch=rust)](https://github.com/Frashmeat/AgentForSTS2/actions/workflows/rust-ci.yml)

> 仓库 **rust** 分支根目录即 Rust + Tauri 实现。**main** 分支保留 Python 端
> 历史（`backend/` + React `frontend/` + PowerShell `tools/`）作 reference。
> 双分支长期并存，路线图 stage 8 决定是否合并。

## 文档

- 战略层：[Rust+Tauri 全栈重写计划](./docs/03-方案/全栈重写/进行中/2026-05-11-Rust+Tauri全栈重写计划.md)
- 战术层：[Rust 重写后续执行计划](./docs/03-方案/全栈重写/进行中/2026-05-11-Rust重写后续执行计划.md)
- 决议清单：[Q1-Q10 + N1-N5 决议汇总](./docs/03-方案/全栈重写/进行中/2026-05-11-Q决议汇总.md)
- ADRs：[`docs/04-决策/`](./docs/04-决策/)（含 0001 架构 / 0002 前端双适配）

## 当前进度

| 阶段 | 状态 |
| --- | --- |
| Stage 0 骨架 | ✅ |
| Stage 1 config + health | ✅ |
| Stage 2.1 knowledge 静态 | ✅ |
| Stage 2.2 knowledge 动态（ilspycmd + BaseLib + code_facts + 包导入导出） | ✅ |
| Stage 2.3 planning | ✅ |
| Stage 2.4 codegen prompt assembler | ✅ |
| Stage 3.1b 桌面端工程文件夹 | ✅ |
| Stage 3.5 全 9 个 handler（含 asset_generate） | ✅ |
| Stage 4 LLM（Anthropic + OpenAI 双协议）+ image_gen | ✅ |
| Stage 5 装配收口（桌面侧） | 🟡 进行中 |
| Stage 3.1a / 3.3a / 3.6 Web 轨 | ⏳ 中转站决策后启动 |
| Stage 6 前端业务页 | ✅ 4 个核心页（dashboard / editor / batch / log） |
| Stage 7 deploy CLI | ✅ 4 子命令 |
| Stage 8 切换 main | ⏳ |

## 结构

```
.
├── Cargo.toml              workspace 根
├── package.json            前端依赖 + 构建脚本
├── vite.config.ts          __IS_TAURI__ 编译时变量
├── index.html / src/       React/TS 前端
│   ├── components/         8 个 Card 组件 + Layout
│   ├── pages/              4 个 page（Dashboard/ModEditor/Batch/Log）
│   └── services/           api.ts + tauriApi.ts + webApi.ts 双适配
├── crates/
│   ├── ats-core/           纯业务逻辑（无 axum/无 tauri 依赖）
│   │   ├── platform/       Job/JobKind 状态机 + 9 个 handler
│   │   ├── knowledge/      8 个模板 + ilspycmd + BaseLib + code_facts
│   │   ├── llm/            Anthropic + OpenAI + factory
│   │   ├── image_gen/      OpenAI Images 协议
│   │   ├── image_proc/     启发式背景去除
│   │   ├── planning/       PlanItem + validation + execution_bundles
│   │   ├── codegen/        6 个 assemble_*_prompt
│   │   ├── prompting/      bundle 分段渲染
│   │   ├── audit/          运行时审计 JSONL
│   │   ├── plan_artifact/  PlanItem 状态跟踪
│   │   ├── project/        工程文件夹（ProjectFolder）+ AppDataPaths
│   │   ├── project_utils/  中文路径 + Windows 长路径
│   │   ├── capabilities/   本机环境检测
│   │   ├── mod_analyzer/   已存在 mod 项目分析
│   │   └── config/health/errors 基础设施
│   ├── ats-web/            axum HTTP 服务器（Web 端）
│   └── ats-cli/            部署/打包/日志 CLI（4 子命令）
└── src-tauri/              Tauri v2 桌面壳（Workstation 端）
    └── src/commands/       11 个命令模块对应 ats-core 服务
```

## 前置

- Rust stable（rust-toolchain.toml 已锁定）
- Node.js 20+
- Windows：MSVC build tools + WebView2（Win11 自带，Win10 需要 Edge runtime）
- Linux：见 [Tauri prerequisites](https://tauri.app/start/prerequisites/)
- 可选：[ilspycmd](https://github.com/icsharpcode/ILSpy)（反编译 sts2.dll）
  ```
  dotnet tool install -g ilspycmd
  ```

## 常用命令

### 开发

```bash
# 第一次拉下来
npm install

# 桌面应用开发（Vite HMR + Tauri，自动 reload）
npx tauri dev
# 或者用本目录里的 ps1 脚本：
./dev.ps1

# Web 服务器开发
./dev-web.ps1
# 等价于：
npm run build:web && cargo run -p ats-web

# CLI（部署/打包）
cargo run -p ats-cli -- --help
```

### 生产构建

```bash
# 桌面安装包（MSI / NSIS / DMG / DEB / AppImage）
npx tauri build
# 或：
./build.ps1

# Web 二进制
npm run build:web
cargo build -p ats-web --release
# 或 Docker：
./build-web.ps1 -Docker

# CLI
cargo build -p ats-cli --release
```

### 质量门

```bash
cargo test --workspace               # 200+ unit + integration tests
cargo check --workspace
cargo clippy --workspace             # 当前有 pedantic 警告未清；CI advisory 跑
npx tsc --noEmit
npm run build:web
```

## 配置

主配置文件：`runtime/agentthespire.config.json`（template 见 `runtime/agentthespire.config.example.json`）。

### LLM provider（Q5/Q9 决议）

支持任意第三方代理（OpenAI / Anthropic 协议兼容均可）：

```json
"llm": {
  "provider": "anthropic",       // 或 "openai" / "new_api" / "one_api"
  "model": "claude-sonnet-4-6",
  "api_key": "sk-...",
  "base_url": "https://your-proxy.example"
}
```

provider 字段容错（trim + lowercase + hyphen→underscore）：
- `""` / `"anthropic"` / `"claude"` → AnthropicClient（Messages API）
- `"openai"` / `"openai_compatible"` / `"new_api"` / `"one_api"` → OpenAIClient (Chat Completions)

### image_gen（asset_generate handler 用）

```json
"image_gen": {
  "provider": "openai",
  "model": "dall-e-3",            // 或代理支持的任意 model 名（如火山 doubao-）
  "api_key": "...",
  "base_url": "...",
  "size": "1024x1024"
}
```

## 工程文件夹（桌面端）

Q1 决议：桌面端无 DB，每个 mod 项目是自包含目录：

```
<user-chosen-path>/<project_name>/
├── project.json        # 项目元数据
├── plan.json           # ModPlan
├── items/              # PlanItem 状态 + plan_artifact 跟踪
├── artifacts/          # 生成的 .cs / .png
├── history/            # 任务历史（FileJobRepository）
└── .ats/               # lock + version + audit.log
```

可整体 zip / git / 复制粘贴携带。

## 签名 / 公证（Q7）

当前决议：先发不签名版本。Windows 用户看 SmartScreen 警告时点"更多信息 → 仍要运行"。

未来启用签名时填 `src-tauri/tauri.conf.json` 的 `bundle.windows.certificateThumbprint` 和 `bundle.macOS.signingIdentity`（当前占位字段 `_signing_help` 注释）。

## 路线图剩余

详见 [执行计划文档](../docs/03-方案/全栈重写/进行中/2026-05-11-Rust重写后续执行计划.md)。当前主要剩余项：

- Stage 5 装配收口剩余子项（优雅停机 / image_proc prewarm 钩子）
- Stage 8 切换 main（rust 分支合并到 main；main 当前仍是 Python reference）
- 真正的 ML rembg（ort + u2net.onnx；现是启发式）
- Web 轨（sqlx + auth + admin pages）—— 中转站方案确定后启动

## 工作流偏好

- TDD：每个新模块带 3+ 个 cargo 单测
- 提交前必跑：`cargo test --workspace` + `npx tsc --noEmit` + `npm run build:web`
- commit message 用中文，标 stage 编号 + 测试数量
- ats-core 不引 axum/tauri；ats-web 装配 sqlx；src-tauri 装配 file repo
- LLM/文件等含外部 IO 的 trait 用 async-trait
- 文件 IO 用 tempfile + rename 保原子；进程内 Mutex 串行化写
