# Order 8: Visual Batch/Complex and Final Gates

## Goal

把 Batch/Complex 从可编辑原始 JSON 改造成 Item Library 驱动的可视化工作流，并用同一套 definition、Plan、Single、Run、Resource、Build 和 Package 合同完成本轮最终机器门禁。

## Decisions

- `BatchGenerateRequest` 升级 v4：请求只包含工程 `modId`、已保存的 `StoredItemDefinition`、确定性 `artifactId` 和失败策略；React 不构造或编辑 `PlanItem`。
- Batch Feature 从每个 definition 的 canonical `behaviorIntent` 构造 Plan request，执行并持久化独立 Plan child Run，再把模型返回 Plan 固定到同一 `itemId/itemType` 后复用 Single v3。
- Batch result 升级 v2：每项记录精确 `definitionHash`、Plan Run、可选 Single Run、终态、typed failure 和成功结果。即使全部 Item 失败，Batch 编排本身仍成功并返回完整可重试结果；取消和合同/基础设施失败仍使父 Run 失败或取消。
- `ComplexGenerateRequest` 升级 v3，直接嵌套同一 Batch v4 请求；Complex 不再维护第二份 planning loop。只有全部所选 Item 成功后才执行 Build/Package，避免打包部分生成结果。
- UI 只从 Item Library 选择当前 definition pointer，展示只读 typed request preview；失败重试从父 Run 的持久化 request 取回精确 definition snapshot，不自动替换为当前 pointer。
- child Run 展示只读取持久化 `RunRecord`。progress 事件不是终态权威，React 不伪造 child 状态。
- Build/Package 的现有独立表单保留；Complex 的 package 参数由结构化控件生成。

## Acceptance Criteria

- [x] Batch/Complex 页面不再存在可编辑 JSON request textarea。
- [x] 当前工程的 Item Library 可多选；Pack/Truth 未就绪的 definition 可见但不可提交。
- [x] Batch 与 Complex 使用同一 Batch v4 definition 输入和后端 Plan→Single 路径。
- [x] typed request 以只读 preview 展示，并包含精确 `definitionHash`。
- [x] 每个处理过的 Item 显示持久化 Plan Run、Single Run、状态和 typed failure；不存在 Single Run 时能区分 Plan 失败。
- [x] 可以仅重试失败 Item，且重试复用原父 Run 中固定的 definition snapshot。
- [x] 缺失/无效资源、类型不匹配或未就绪 Truth 在父 Run/模型调用前被后端 preflight 拒绝。
- [x] Complex 只在所有 Batch Item 成功后执行真实 Build/Package。
- [x] Batch/Complex request/result IPC payload 有运行时 guards 和 malformed canary。
- [x] 四种正式 STS2 类型的组合测试覆盖 child Runs、真实 compile、Artifact hash/provenance、Build/Package 和无 staging/transaction residue。
- [x] workspace test/check/clippy、desktop ML/E2E、frontend test/type/build、DAG、CLI、fmt/diff 全部通过。
- [x] 稳定规范、当前事实、父子 Trellis 状态同步；不创建 release candidate。

## Risks And Trade-offs

- Batch v4、Complex v3 和 Batch result v2 是破坏性 schema 升级；项目尚未上线，不保留 v3/v2 双合同。
- definition 的 `behaviorIntent` 成为 Batch planning requirements 的唯一来源；需要在提交前验证非空，避免 Shell 另设临时 requirements 状态。
- Batch 父 Run 的 `succeeded` 表示编排完整结束，不代表所有 Item 成功；调用方必须读取 `failed/items`。Complex 使用更严格的全成功策略。
- fail-fast 后未处理项不创建 child Run；UI 从 request 与 result 差集明确显示未执行，而不是伪造 Run。

## Excluded

- 新 release candidate、安装器、真实外部 Provider 和真实游戏人工验收。
- Character、跨 Item 引用、动画、音频、3D 或第二个 Game Pack。
- 并行 codegen；项目文件事务仍保持串行。

## Verification

- `cargo test --workspace --all-targets`: 132 passed；四类型 Batch 产生 8 个 Plan/Single child Runs、4 次真实 dotnet compile、4 个可复算 Artifact，且无 staging/transaction residue。
- `cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings`、desktop `ml-rembg`/`e2e` feature check 通过。
- `npm run test:frontend`: 23 passed；`npx tsc -b --pretty false` 与 `npm run build` 通过。
- `npm run test:e2e:gui`: 5/5 passed；覆盖隔离工程、Truth 导入、definition、Batch v4、Complex v3、真实 Build/Package 和 Run/Artifact/残留检查。
- Stage 2 DAG self-test/real、CLI catalog、`cargo fmt --all -- --check`、`git diff --check` 通过；CLI Batch contributions 包含 `mod.plan.guidance`。
- 未创建 release candidate；安装器、真实 Provider 和真实游戏行为验收属于后续独立授权门禁。
