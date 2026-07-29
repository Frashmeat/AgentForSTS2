# AgentTheSpire — Rust 重写（rust 分支）

[![Rust CI](https://github.com/Frashmeat/AgentForSTS2/actions/workflows/rust-ci.yml/badge.svg?branch=rust)](https://github.com/Frashmeat/AgentForSTS2/actions/workflows/rust-ci.yml)

> 仓库 **rust** 分支根目录即 Rust + Tauri 实现。**main** 分支保留 Python 端
> 历史（`backend/` + React `frontend/` + PowerShell `tools/`）作 reference。
> 双分支长期并存，路线图 stage 8 决定是否合并。

## 文档

- 总入口：[`docs/README.md`](./docs/README.md)
- 架构：[项目架构总览](./docs/01-总览/项目架构总览.md)
- 当前事实：[当前进度说明](./docs/02-现状/当前进度说明.md)
- 当前方向：[当前方案](./docs/03-当前方案/当前方案.md)
- 历史材料：[`docs/90-归档/`](./docs/90-归档/)

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
| Stage 5 桌面端 MVP 收口 | 🟡 真实链路与安装版待人工验收 |
| Stage 3.1a / 3.3a / 3.6 Web 轨 | ⏳ 中转站决策后启动 |
| Stage 6 前端业务页 | ✅ Dashboard / Editor / Batch / Log / System |
| Stage 7 deploy CLI | ✅ build / deploy / logs / stop / compose template |
| Stage 8 切换 main | ⏳ |

## 结构

```
.
├── Cargo.toml              workspace 根
├── package.json            前端依赖 + 构建脚本
├── vite.config.ts          __IS_TAURI__ 编译时变量
├── index.html / src/       React/TS 前端
│   ├── components/         业务 Card + 通用 UI + Layout
│   ├── pages/              Dashboard/ModEditor/Batch/Log/System
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
│   └── ats-cli/            构建/部署/日志/停止/模板 CLI
└── src-tauri/              Tauri v2 桌面壳（Workstation 端）
    └── src/commands/       Tauri commands 对应 ats-core 服务
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
npm install --include=dev

# 桌面应用开发（Vite HMR + Tauri，自动 reload）
npx tauri dev
# 或者用本目录里的 ps1 脚本：
./dev.ps1

# 如果 node_modules 是从 WSL/Linux/macOS 复制过来的，或安装中断导致
# node_modules/.bin/vite.cmd / Rollup Windows 原生包缺失时，ps1 脚本会
# 自动尝试 `npm install --include=dev` 修复。
# 若本机存在 NODE_ENV=production，手动安装也必须带 `--include=dev`；
# 仍失败时，按提示删除 node_modules 后用同一命令重装。

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

> **⚠️ Web 端部署安全**：`ats-web` 的 `/api/*`（含会消耗 LLM 配额的补全接口）**当前无网络层鉴权**。
> 默认 bind `127.0.0.1` 是安全的；CORS 已收紧为 `web.cors_origins` 白名单（+ `allow_loopback_origins` 时放行本机回环），
> 可挡住浏览器跨源读取响应。但若把端口**对外发布**（如 `0.0.0.0` 或 Docker `-p 7870:7870`），
> 任何能直连该端口的非浏览器客户端仍可直接调用接口盗刷配额。对外发布**必须**置于带鉴权的反向代理之后
> （或仅 `127.0.0.1:7870` bind-publish）。完整的 session 登录鉴权属 Web 轨（sqlx + auth）后续工作。

### 质量门

```bash
cargo test --workspace               # 200+ unit + integration tests
cargo check --workspace
cargo clippy --workspace             # 当前有 pedantic 警告未清；CI advisory 跑
npm run test:frontend                # 前端纯逻辑回归测试
npx tsc --noEmit
npm run build:web
```

## 配置

主配置文件：`runtime/agentthespire.config.json`（template 见 `runtime/agentthespire.config.example.json`）。

### LLM provider

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

桌面端无 DB，每个 mod 项目是自包含目录：

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

## 签名 / 公证

当前决议：先发不签名版本。Windows 用户看 SmartScreen 警告时点"更多信息 → 仍要运行"。

未来启用签名时填 `src-tauri/tauri.conf.json` 的 `bundle.windows.certificateThumbprint` 和 `bundle.macOS.signingIdentity`（当前占位字段 `_signing_help` 注释）。

## 启用 ML 背景去除（可选 feature）

启发式 `SimpleBgRemover` 默认走纯白背景；要让深色 / 复杂背景也能正确出 alpha，
启用 `ml-rembg` feature 启用 u2netp + ort 推理：

```powershell
# 桌面开发（带 ml-rembg）
./dev.ps1 -MlRembg
# 桌面生产打包（带 ml-rembg；首次运行准备 onnxruntime native lib）
./build.ps1 -MlRembg
# Web 服务器开发
cargo run -p ats-web --features ml-rembg
# 直跑 cargo tauri 命令（PS 脚本只是 UX 糖）
cargo tauri build --features ml-rembg
```

- 首次启动会下载 ~5MB 的 `u2netp.onnx` 到 `%APPDATA%/AgentTheSpire/models/`
  并做 SHA-256 校验。下载失败 / 网络断开自动回退到启发式（启发式永远 always-on）。
- **Windows**：使用 `ort` 的 `load-dynamic` 模式。桌面端首次 ML prewarm 会把官方
  ONNX Runtime 1.22.0 的 `onnxruntime.dll` 准备到应用数据目录，再通过
  `ort::init_from` 显式加载；准备或加载失败时回退到启发式背景去除。
- **macOS / Linux**：当前 ort 2.0 走动态链接 `libonnxruntime.{dylib,so}`，build
  完后用 `otool -L` / `ldd` 看下产物，必要时把 lib 放进 bundle 资源（待真有用户跑
  Mac/Linux 时再补具体步骤）。
- 关闭 feature 后整个 ML 代码路径不编译，二进制无 ort / ndarray 开销。CI
  默认 off，保持构建轻量。
- 实时状态在 UI 的 `AuditCard` 上有 chip：Loading / Ready / Failed。

## 路线图剩余

详见[当前方案](./docs/03-当前方案/当前方案.md)；具体任务、状态和验证证据由 Trellis 管理。当前主要剩余方向：

- ~~Stage 5 装配收口剩余子项~~ 大半已完成（audit auto-write + ctrl_c + image_proc
  prewarm + ML rembg + health readiness 字段都在）
- Stage 8 切换 main（rust 分支合并到 main；main 当前仍是 Python reference）
- 真实端到端 ML rembg 复杂背景效果验证（脚手架已就绪，等真生图测）
- Mac/Linux 桌面构建 + onnxruntime 动态库 bundling（按需）
- Web 轨（sqlx + auth + admin pages）—— 中转站方案确定后启动

## 工作流偏好

- TDD：每个新模块带 3+ 个 cargo 单测
- 提交前必跑：`cargo test --workspace` + `npm run test:frontend` + `npx tsc --noEmit` + `npm run build:web`
- commit message 用中文，标 stage 编号 + 测试数量
- ats-core 不引 axum/tauri；ats-web 装配 sqlx；src-tauri 装配 file repo
- LLM/文件等含外部 IO 的 trait 用 async-trait
- 文件 IO 用 tempfile + rename 保原子；进程内 Mutex 串行化写
