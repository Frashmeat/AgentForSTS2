# AgentTheSpire - Rust / Tauri

[![Rust CI](https://github.com/Frashmeat/AgentForSTS2/actions/workflows/rust-ci.yml/badge.svg?branch=rust)](https://github.com/Frashmeat/AgentForSTS2/actions/workflows/rust-ci.yml)

`rust` 分支是当前 Rust + Tauri 实现；`main` 保留旧 Python 实现作为历史参考。

## 当前架构

AgentTheSpire 是编译期分层的模块化单体。桌面壳不拥有游戏规则、Prompt 或生成事务：

```text
React / Tauri / Web / CLI
-> Feature catalog + Stage2Composition
-> typed Feature service
-> Pack Contribution + Truth Evidence + selected Resources
-> Runtime ports / registered Adapters
-> RunRecord v3 + ArtifactManifest v3
```

```text
crates/
  ats-kernel/        稳定 ID、schema、失败和 BuildInfo 合同
  ats-runtime/       Run/Artifact envelope、取消和模型/执行端口
  ats-game-context/  Game Pack、Contribution、Truth/Evidence、工程模板
  ats-workspace/     工程锁、recents、资源版本和工程文件夹
  ats-features/      12 个 typed Feature、Recipe 和组合服务
  ats-adapters/      HTTP、文件仓储、dotnet、ZIP 等基础设施实现
  ats-web/           health、Feature catalog 和静态 SPA
  ats-cli/           deploy 工具与共享 Feature catalog
src-tauri/           桌面 composition root、ProjectSession 和 IPC
src/                 React/TypeScript 产品界面与 v3 transport guards
game_packs/sts2/     STS2 contribution、资源规格和工程模板
```

旧 `ats-core`、Run/Artifact v2、raw LLM、Prompt preview 和旧 handler API 已从当前生产路径删除。

## 产品 Feature

共享 catalog 当前包含：

- `project.create`
- `mod.plan`
- `composition.plan`
- `composition.retry-node`
- `composition.generate`
- `resource.prepare`
- `mod.generate.single`
- `mod.generate.batch`
- `mod.generate.complex`
- `log.analyze`
- `project.build`
- `project.package`

桌面端支持工程创建/打开/关闭、Truth 导入、Feature 提交、v3 Run 查询/取消和设置。Web 当前只暴露 health/catalog；CLI 可用 `cargo run -p ats-cli -- features` 输出相同 catalog。

`resource.prepare` 已统一支持用户文件、Pack 默认资源和 AI 媒体。AI Adapter 支持 OpenAI Images 与 Chat Completions 图片协议，生成 bytes 进入同一版本化 Resource Workspace；health 的 `mediaGenerationRegistered` 为 `true`。`ml-rembg` 是独立的候选构建/图片后处理身份，不等于媒体 provider 配置或连通性。

## Prompt 所有权

一次模型请求由以下可复验输入装配：

```text
Feature Recipe
+ Pack Contribution
+ Truth Evidence
+ Selected Resources
+ Project Context
+ Runtime Custom Instructions
+ typed output contract
= ModelRequestSnapshot v1
```

自然语言任务结构位于 `crates/ats-features/recipes/`；游戏内容位于 `game_packs/<id>/stage2-game-pack.json`；当前事实来自 verified Truth；用户补充指令来自 `llm.custom_prompt`。代码只保留 role、schema、section ID、转义、截断、安全和 JSON 输出等协议约束。

## 开发

前置：Rust stable、Node.js 20+、Windows MSVC Build Tools 和 WebView2。运行真实 STS2 build/package 还需要游戏程序集、Godot 和 Pack 声明的工具链。

```powershell
npm install --include=dev
npx tauri dev

# Web health/catalog + SPA
npm run build:web
cargo run -p ats-web

# Feature catalog
cargo run -p ats-cli -- features
```

## 质量门

```powershell
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
npm run test:frontend
npx tsc -b --pretty false
npm run build
```

任何 push 前还必须按 `CLAUDE.md` 执行 `scripts/ci-local.ps1`。完整候选构建和安装版人工验收是独立门禁，不由局部测试替代。

## 配置

默认配置：`runtime/agentthespire.config.json`。LLM 支持 Anthropic 与 OpenAI-compatible HTTP provider，并允许 `llm.custom_prompt` 作为本次运行的附加指令；密钥不会进入 Run、Artifact、IPC 错误或共享验证文件。

桌面工程是自包含目录：

```text
<project>/
  project.json
  items/
  Generated/
  artifacts/<artifact-id>/runs/<run-id>/artifact-manifest.json
  .ats/lock
  .ats/version
  .ats/runs-v3/<run-id>.json
```

工程使用 OS 文件锁。关闭、切换和退出先取消并排空 Run，再释放锁；失败不得伪造成功或留下 final `.staging-*`。

## 文档

- [文档索引](./docs/README.md)
- [项目架构总览](./docs/01-总览/项目架构总览.md)
- [Prompt 与文本资源总览](./docs/01-总览/Prompt-文本资源总览.md)
- [当前进度](./docs/02-现状/当前进度说明.md)
- [当前方案](./docs/03-当前方案/当前方案.md)

Windows 当前发行仍未签名；签名、SmartScreen 信誉、自动发布、Web auth/SQLx、第二个真实游戏和动态插件均不属于当前 Stage 2 迁移。
