# Stage 2 开工前稳定化与可观测性收口

## Goal

把 Windows 最终候选验收后暴露的零散问题收敛为一个可一次完成的 Stage 2 开工前里程碑：恢复干净、可解释的 Core 基线，补齐 `asset_generate` 编译失败诊断与成功回归，隔离 Truth Snapshot runtime 和 workspace watcher 的长期锁冲突，并形成不依赖 Stage 2 产品目标的 Game Pack readiness gate。完成后再讨论并进入 Stage 2，不携带本轮候选的诊断债务。

## What I already know

- 最终候选 `rc-20260802T064241Z-6e8d291971e8` 已完成机器门禁、安装版启动、真实 Artifact 发布、锁释放/重获取 E2E，并由用户明确确认验收。
- 当前未提交的 `fs_atomic.rs`、Truth Snapshot store、Artifact Store 共享 rename retry 和诊断 example 不能解决 Orca workspace watcher 对大型反编译索引目录的长期锁；它们未进入最终候选。
- 安装版真实 `asset_generate` 已完成 ML 出图和去背景，但在 `asset_bundle.compile` 失败。事务正确回滚，却没有保留足以复现的生成 C# 和 compile stdout/stderr。
- 无副作用 custom-code Run 已证明 final Artifact 原子发布、manifest/hash、本轮 staging 清理和 Run CAS 成功。
- Game Pack Stage 1 已完成 Pack identity、loader/registry、project `game_id`、Truth Snapshot、VerifiedGameContext、guidance/template/resource/build/package 声明化和 STS2 纵向切换。
- 长期架构要求通用 Core 不出现 `game_id == "sts2"` 分支；第二个真实游戏用例应以“只新增 Game Pack，只有可跨游戏复用能力才改 Core”为验收标准。

## Assumptions (temporary)

- 本任务是 Stage 2 的前置稳定化，不直接实现第二个 Game Pack 或动态 Pack 安装。
- Stage 2 的具体产品目标、首个真实游戏和外部 Pack 交付方式留到本任务完成后决定；本任务只固定所有方向都必须满足的 Core/Pack 边界。
- `asset_generate` 的失败证据必须脱敏、run-scoped、可复验，不能把原始绝对路径、provider body 或凭据写入 RunRecord/IPC。
- Truth Snapshot 的正确修复方向是 runtime/workspace 隔离和明确诊断，不用无限或更长 rename retry 掩盖持续占用。
- 已验收的 Artifact retry 契约、同目录原子发布、失败清理和 Run CAS 顺序保持不变。

## Requirements (evolving)

- 处理当前未提交诊断改动：一次性 example 不进入产品；共享 rename retry 只有在规范、测试和实际短暂冲突用例证明必要时才保留。
- 为 structured `asset_generate` compile failure 保存脱敏且完整的 run-scoped diagnostics，包括生成 bundle、执行摘要、退出码和有界 stdout/stderr。
- compile failure 通过稳定 typed failure 和 diagnostic ID 到达 RunRecord/IPC，仍然回滚正式工程文件和 final Artifact。
- 增加 deterministic compile-failure 与 compile-success 回归，成功路径复算 ArtifactManifest、文件哈希、Game Pack/Snapshot identity 和 staging 清理。
- 开发/E2E Truth Snapshot runtime 默认位于 workspace watcher 范围外；长期锁冲突有明确错误和恢复动作，不伪造成瞬态成功。
- 建立目标无关的 Stage 2 readiness 检查：通用 Core 的 STS2 特例清单、Pack-neutral fixture/最小纵向样本、Pack-only 扩展边界、现行 spec 与文档一致性。
- 完成后工作区干净，相关定向测试、workspace test/clippy/check 和必要桌面 E2E 通过；不重建 Windows installer，除非代码变更确实要求新候选且另行授权。

## Acceptance Criteria (evolving)

- [x] 当前诊断性源码和 example 已作出明确处置，工作区不再保留无归属实验改动。
- [x] `asset_generate` compile failure 留下可定位但脱敏的 run-scoped diagnostics，Run 为 typed failed，无正式文件、final Artifact 或本轮 staging。
- [x] 同一 deterministic fixture 的成功 Run 生成可复算一致的 final ArtifactManifest 和文件哈希。
- [x] Truth Snapshot 在 workspace 外 runtime 刷新成功；workspace watcher 长期占用不会被更长重试描述为已修复。
- [x] Pack-neutral readiness fixture 不依赖 STS2 ID、C#/Godot/BaseLib 常量或 STS2 package layout。
- [x] Stage 2 开工前的扩展边界和不变量写入 PRD/code-spec；具体入口目标明确留给本任务完成后的产品讨论。
- [x] 相关定向测试与约定全量门禁通过，文档同步，Git 工作区干净。

## Readiness Evidence (2026-08-02)

- GUI E2E `npm run test:e2e:gui` 为 5/5：Truth Snapshot 从 OS app-data 刷新且 `.staging` 为空；成功 asset Run 复算 manifest 和全部文件哈希；失败 asset Run 为 `artifact.compile_failed` 并保留脱敏 compile report/generated bundle；跨路由恢复、build/package 和 Godot 配置失败路径通过。
- Core 定向回归：`asset_bundle` 9/9、`asset_generate` 14/14、`failure` 11/11、`package_project` 7/7；诊断写失败时保留既有图片证据并清理 compile 专属半成品。
- 全量门禁：`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo check --workspace`、`npm run build`、`npm run test:frontend`、`npx tsc -b --pretty false` 和 `git diff --check` 均通过。
- 改动 Rust 文件逐一通过 `rustfmt --check`。仓库级 `cargo fmt --all -- --check` 仍会报告本任务未修改的 `crates/ats-core/src/image_gen/chat_image.rs` 既有格式差异；本任务未借机改动该文件。
- `docs/03-当前方案/通用Mod流水线与Game-Pack边界.md` 记录当前 STS2 workstation/provider/structured-asset/indexer/runner/UI 耦合和 Stage 2 变更门槛，不把 Pack-neutral package fixture 扩大解释为任意游戏已受支持。
- 用户已在 2026-08-02 明确要求完成进入 Stage 2 前的全部收尾，确认 readiness 结果并授权提交/归档。代码、规范、文档和任务归档进入同一个提交；不推送或变基。

## Definition of Done

- Tests added/updated for success, deterministic failure, rollback, redaction, staging cleanup and Pack neutrality.
- Backend executable specs include signatures, payload fields, validation/error matrix, Good/Base/Bad cases and required assertions.
- No secrets, raw provider bodies or absolute private paths enter persisted/user-visible evidence.
- Current-state and current-plan docs identify Stage 2 as the next active development phase.
- Human confirms the readiness result before this task is completed and archived.

## Out of Scope (explicit)

- Windows code signing, SmartScreen reputation, auto-update or public release automation.
- Rebuilding the already accepted release candidate without a separately approved necessity.
- Implementing a full marketplace, remote registry or arbitrary plugin execution model.
- Defining or implementing the concrete Game Pack Stage 2 product target.
- Hiding a permanent watcher lock behind unbounded retry, copy publication or non-atomic activation.
- Broad UI redesign or unrelated dependency upgrades.

## Technical Notes

- Candidate acceptance: `.trellis/tasks/archive/2026-08/08-01-release-candidate-and-backend-docs/`.
- Likely Core files: `crates/ats-core/src/platform/application/handlers/asset_bundle.rs`, `asset_compile.rs`, `asset_generate.rs`, `failure.rs`, `fs_atomic.rs`, `game_pack/truth_snapshot/store.rs`.
- Relevant specs: `.trellis/spec/backend/error-handling.md` and `.trellis/spec/backend/quality-guidelines.md`.
- Architecture references: `docs/03-当前方案/通用Mod流水线与Game-Pack边界.md` and `docs/03-当前方案/2026-07-30-Game-Pack-Stage-1职责清单与迁移设计.md`.
