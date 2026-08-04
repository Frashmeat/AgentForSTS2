# Order 2: Item Repository And Generic Editor

## Goal

在 Order 1 的 Pack v3 / ItemDefinition v1 合同上建立可版本化的工程内 Item Library、typed Tauri transport 和 Pack-driven 通用编辑器，并把 per-type Truth readiness 放到保存与 Run 创建之前。

## Decisions

- Workspace 只定义 `ItemRepository` port；Filesystem Adapter 使用 `.ats/items-v1/<itemId>/current.json` 与 `definitions/<definitionHash>.json`。
- definition snapshot 不可变；相同 hash 幂等；`current.json` 同目录原子替换；stable itemId 的 itemType 不能改变，崩溃后孤立 snapshot 也保留该约束。
- `ProjectSession` 持有工程唯一的 `FileItemRepository`，使同一工程的并发 Tauri Item 命令共享同步门。
- Tauri 暴露 capability/list/get/save typed commands。Pack drift、缺 Truth/readiness、not found 与 storage failure 保留稳定 typed failure。
- `mod.plan`、Single、Batch、Complex 在 `RunRecord::new` 前检查所有请求类型 readiness；blocked 请求不创建 Run。
- React 类型列表和字段控件只来自 capability descriptor；不维护 `relic/card/potion/power` 平行常量。
- 英文/中文及未来 locale 使用同一 candidate/confirmed/outdated 模型：修改源文本或已确认译文后必须重新确认。
- 当前 Plan/Single 作为明确标注的 generation bridge 保留；它不绑定 definition hash，Order 4 Relic Gate 才完成该纵向切换。
- Resource binding 本 Order 仅保存 typed map；JSON 输入桥由 Order 3/4 的 Resource Workbench 替换。

## Acceptance Criteria

- [x] Item repository 保存不可变 definition snapshots、原子 current pointer、历史 hash 和 deterministic list。
- [x] repository 拒绝 tamper/path/type drift，并在 pointer 丢失恢复场景中仍阻止 itemType 改变。
- [x] capability/list/get/save Tauri commands 与 TypeScript/Web transport 同步。
- [x] save 在 capability blocked 或 Pack drift 时返回 typed failure，不落盘。
- [x] Plan/Single/Batch/Complex readiness 在 Run 创建前阻断。
- [x] Item Library 支持新建、打开 current、保存新版本并显示 definition hash。
- [x] 通用编辑器渲染 text/integer/boolean/choice/string-list 字段，不含 STS2 类型分支。
- [x] locale 候选必须显式确认；源文本或已确认译文改变后状态回到 outdated。
- [x] capability blocked 类型显示安全原因且不能新建或提交生成。
- [x] 前端 runtime guard 使用与 Rust 完全一致的 camelCase wire contract。
- [x] backend/frontend/cross-layer specs 与当前状态文档同步。

## Machine Evidence

2026-08-04 已通过：

```text
cargo test --workspace --all-targets                         117 passed
cargo check --workspace --all-targets                        passed
cargo clippy --workspace --all-targets -- -D warnings        passed
cargo check -p agentthespire-desktop --features ml-rembg     passed
cargo check -p agentthespire-desktop --features e2e          passed
npm run test:frontend                                         15 passed
npx tsc -b --pretty false                                    passed
npm run build                                                 passed
node scripts/check-stage2-dependency-dag.mjs --self-test      passed
node scripts/check-stage2-dependency-dag.mjs                  passed
cargo run -p ats-cli -- features                              9 Features
cargo fmt --all -- --check                                    passed
git diff --check                                              passed
```

未运行新的完整 release candidate、安装器、真实 Provider 或真实游戏验收；这些不属于 Order 2。

## Excluded / Deferred

- 不绑定 ItemDefinition hash 到 Plan/Single/Artifact；Order 4 完成。
- 不实现媒体探测、master/derived/candidate/selected；Order 3 完成。
- 不新增 Card/Potion/Power；Order 5-7 完成。
- 不运行 release candidate、安装器、真实 Provider 或真实游戏验收。

## Completion Gate

完整 workspace test/check/clippy、frontend test/type/build、DAG、fmt 和 diff 门禁通过后归档并自动 Git commit。用户已于 2026-08-04 授权后续每个 Order 自动提交；push、rebase、发布、完整候选及真实环境验收仍需独立授权。
