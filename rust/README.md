# AgentTheSpire — Rust 重写

[![Rust CI](https://github.com/Frashmeat/AgentForSTS2/actions/workflows/rust-ci.yml/badge.svg?branch=rust)](https://github.com/Frashmeat/AgentForSTS2/actions/workflows/rust-ci.yml)

本目录是项目的 Rust + Tauri 重写。与仓库根的 Python `backend/` / React `frontend/` / PowerShell `tools/` 并存，目标在路线图 stage 8 完整接管。

设计文档：[`docs/03-方案/全栈重写/进行中/2026-05-11-Rust+Tauri全栈重写计划.md`](../docs/03-方案/全栈重写/进行中/2026-05-11-Rust+Tauri全栈重写计划.md)。

## 结构

```
rust/
├── Cargo.toml              workspace 根
├── package.json            前端依赖 + 构建脚本
├── vite.config.ts          __IS_TAURI__ 编译时变量
├── index.html / src/       React/TS 前端
├── crates/
│   ├── ats-core/           纯业务逻辑（无 axum/无 tauri 依赖）
│   ├── ats-web/            axum HTTP 服务器（Web 端）
│   └── ats-cli/            部署/打包/日志 CLI
└── src-tauri/              Tauri v2 桌面壳（Workstation 端）
```

## 前置

- Rust stable（rust-toolchain.toml 已锁定）
- Node.js 20+
- Windows：MSVC build tools + WebView2（Win11 自带，Win10 需要 Edge runtime）
- Linux：见 [Tauri prerequisites](https://tauri.app/start/prerequisites/)

## 常用命令

```bash
# 第一次拉下来
npm install

# 桌面应用开发（Vite HMR + Tauri，自动 reload）
npx tauri dev

# Web 服务器开发
npm run build:web           # 构建前端到 dist/
cargo run -p ats-web        # 启动 axum 服务

# CLI（部署/打包）
cargo run -p ats-cli -- --help

# 生产构建
npx tauri build             # 桌面安装包
npm run build:web && cargo build -p ats-web --release   # Web 二进制
cargo build -p ats-cli --release                         # CLI

# 类型与 lint
cargo clippy --workspace -- -D warnings
npx tsc --noEmit
```

## 当前阶段

Stage 0 — 骨架。各 crate 与前端 `src/services/api.ts` 仅含最小 stub（一个 `health` 路由）。后续阶段按设计文档第 5 章「迁移路线图」推进。
