# CLAUDE.md

本仓库给 Claude / AI 助手的强制约束。`AGENTS.md` 是阅读跳板入口，本文件是**操作规则**。

---

## 1. 推送前必须 docker 模拟 CI（强制）

**Why**：本仓库主开发机是 Windows，但 GitHub Actions 的 Rust CI 跑 Ubuntu。
Tauri 在 Linux 上要装一串 GTK / webkit2gtk / glib 系统包（见 `Dockerfile.ci`），
Windows 本地 `cargo test --workspace` 全过 ≠ Linux CI 通过。

**Rule**：
- 任何 push 到 `rust` 分支之前，**必须**先在本地跑：
  ```powershell
  .\scripts\ci-local.ps1
  ```
- 该脚本用 `Dockerfile.ci`（ubuntu:24.04 + apt deps + rust + node）逐项执行
  `.github/workflows/rust-ci.yml` 中的步骤（cargo check → cargo test → tsc → vite build）。
- 退出码 0 才允许 push；非 0 必须先修，不要"先推到云端看看"。
- 仅改 docs / 仅改 PowerShell 脚本 / 仅改 `.gitignore` 等显然不影响 Rust+前端构建的，可以跳过；
  但**任何**碰到 `crates/`、`src/`、`package*.json`、`Cargo*`、`.github/workflows/`、
  `Dockerfile*`、`rust-toolchain*` 的改动，零例外都要先跑。

**Why not GitHub Actions 直接试**：iterate 一次 push → 等 ~3 分钟 → 看 log → 改 → 再推，
循环 5 次就 15 分钟。本地 docker 缓存层后 iterate < 30s。

**Why not `act`**：act 的 image ~10GB，且与 GitHub Actions runner 行为有细微差异；
Dockerfile.ci 的复杂度只是 actions/setup-* 的简化版，可控性更好。

---

## 2. 通用约束

- 详细任务流 / 项目背景：先读 `AGENTS.md` → `PROJECT_SPEC.md`（若存在）→ `docs/`
- 当前方案：`docs/03-当前方案.md`；具体任务和执行状态读取当前 Trellis `prd.md`
- 测试基线：`cargo test --workspace` 必须 204 unit + 4 integration = 208 全过；
  新增模块带 3+ 单测（TDD）
- 提交消息：中文，标 stage / 测试数量（参照 `git log --oneline -10`）
- ats-core 不引 axum / tauri；含外部 IO 用 async-trait
- 文件 IO 用 tempfile + rename 保原子

---

## 3. 例外清单（写下来避免反复争论）

| 改动类型 | 跑 ci-local 吗 |
| --- | --- |
| 仅改 `docs/**/*.md` | 否 |
| 仅改 `*.ps1` 脚本 | 否 |
| 仅改 `.github/ISSUE_TEMPLATE/` | 否 |
| 改 `crates/**`、`src/**` | **是** |
| 改 `Cargo.toml` / `Cargo.lock` | **是** |
| 改 `package.json` / `package-lock.json` | **是** |
| 改 `.github/workflows/*.yml` | **是**（双向验证：本地 docker 也要相应改） |
| 改 `Dockerfile.ci` 自身 | **是**（重 build image） |
| 改 `rust-toolchain.toml` | **是** |
| 改 `tauri.conf.json` / `src-tauri/**` | **是** |

任何不确定的边界情况，跑就完了（30s 的事，吃不了亏）。
