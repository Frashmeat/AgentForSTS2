# Order 1: Pack Item Contracts And Readiness

## Goal

建立后续 Item Library、资源工作台和 STS2 类型扩充共同依赖的稳定后端合同，消除 Plan、Single Generate 与 UI 各自维护类型/Truth/Resource 事实的结构性重复。

## Decisions

- Game Pack 破坏性升级为 schema v3；顶层 `itemTypes` 是类型身份、localized display name、受限字段 descriptor、required locale、Evidence Query 和 Resource role 的唯一真源。
- `pack.mod-plan-guidance` v3 只拥有规划指导；`pack.mod-generate-single` v3 只拥有生成指导、validation Primitive 与 exact generated-file roles。
- Kernel 只新增稳定 `ItemId`、`ItemTypeId`、`ItemFieldId`、`LocaleId`，不新增 STS2 类型枚举。
- Workspace 拥有 ItemDefinition v1 与 definition SHA-256；Runtime、Adapter 和 Shell 不拥有该领域定义。
- Game Context 以 pinned Pack + optional current verified Truth 计算 per-type readiness；不创建 Run、不调用模型。
- 不保留 Pack v2 类型列表兼容层；现有 release/Truth/Run/Artifact 证据保留，但旧 Truth identity 不能代表新 Pack。

## Acceptance Criteria

- [x] Pack loader 验证非空唯一类型、localized names、字段约束、Evidence Queries 与 Resource roles。
- [x] Built-in STS2 Pack 使用 v3 pinned hash，并只正式声明当前 `custom_code/relic`。
- [x] Plan 与 Single Generate 复用 Pack 顶层 item catalog，不再复制 Evidence Query/Resource roles。
- [x] Single generation contribution 必须精确覆盖 catalog type IDs。
- [x] Stable Item/Type/Field/Locale IDs 在构造与 Serde 时应用同一验证。
- [x] ItemDefinition v1 覆盖 canonical fields、behavior intent、locale status、Resource version binding，并可确定性计算 hash。
- [x] 无 Truth、部分缺 Evidence、全部满足三种 readiness 结果均有 typed 后端测试。
- [x] Cargo DAG 不变；Runtime 不认识游戏类型；Pack 不携带任意执行代码。
- [x] Active specs/docs 同步到 Pack v3 和当前连续 Orders。

## Machine Evidence

2026-08-04 当前未提交工作树：

```text
cargo test -p ats-kernel -p ats-workspace -p ats-game-context -p ats-features
  48 passed
cargo test --workspace --all-targets
  110 passed
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
node scripts/check-stage2-dependency-dag.mjs --self-test
node scripts/check-stage2-dependency-dag.mjs
cargo fmt --all -- --check
git diff --check
```

未运行新的完整 release candidate、安装器、真实 Provider 或真实游戏验收；这些不属于 Order 1。

## Completion Gate

Order 1 代码和机器门禁完成后，先请求用户授权 Git commit。提交完成才把本任务标记 completed，并切换 Order 2；push/rebase 仍需独立授权。
