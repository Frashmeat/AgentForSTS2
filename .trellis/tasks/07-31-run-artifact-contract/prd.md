# RunRecord v2 与 ArtifactManifest

> 状态：planning
>
> 优先级：P0
>
> 开发类型：fullstack
>
> 方案来源：[`桌面后端运行时加固与发布收口方案`](../../../docs/03-当前方案/2026-07-31-桌面后端运行时加固与发布收口方案.md) Work Order 1
>
> 架构边界：[`通用 Mod 流水线与 Game Pack 边界`](../../../docs/03-当前方案/通用Mod流水线与Game-Pack边界.md)

## 1. 目标

以一次破坏性迁移完成 `Job -> Run` 全链路收口，建立唯一的执行事实 `RunRecord v2` 和唯一的成功产物证据 `ArtifactManifest`：

- Run 状态、timeline、failure 和 result 由同一个 repository CAS 聚合维护。
- 成功产物按 `artifact-id/run-id` 保存不可变快照，Run result 只引用 manifest 及其 SHA-256。
- 删除 Audit 对 Run 生命周期的旁路投影，并停止生成或读取 `evidence.md`。
- 旧 V1 history 整目录备份，新会话只读写 V2 Run history，不保留兼容 API、别名或静默 fallback。

## 2. 范围

### 2.1 纳入

- `ats-core` 的 platform domain、repository、application service、handlers 和持久化模型。
- 所有生产 `Job*` Rust 类型、模块、文件名、公开方法和 ID 前缀迁移为 `Run*`。
- Tauri command、event、state、payload 和 snapshot 的 `job*` 内部契约迁移为 `run*`。
- TypeScript API 类型、hooks、store 和组件内部标识迁移为 `run*`；中文产品文案继续使用“任务”。
- RunRecord v2、timeline、tagged RunResult、V1 history 备份和启动恢复边界。
- Artifact Store、ArtifactManifest、Evidence、文件哈希和事务发布顺序。
- 删除产品链中的 `.ats/audit.log` Run 生命周期投影和 `evidence.md` 读写。
- 同步稳定架构、当前进度和 `.trellis/spec/backend/quality-guidelines.md`。

### 2.2 不纳入

- Work Order 2 的完整 `FailureCategory`、`RecoveryAction` 和所有领域错误归一化；本任务只定义 Run 持久化所需的最小 `ActionableFailure`。
- Work Order 3 的 `ProjectSession`、关闭/切换工程 cancel-and-drain 和 30 秒超时。
- baseline/ML 安装版、BuildInfo、发布候选脚本和依赖升级。
- 旧 history 逐条迁移、兼容解码、隐式双读或恢复旧 Audit 查询。
- 自动清理成功 Artifact Store。
- 修改已通过真实游戏验收的 `game_packs/sts2/` 资源、规则、模板、build recipe 或 package layout。

## 3. 破坏性边界

- Rust、Tauri 和 TypeScript 生产代码不得保留 `Job` 类型别名、兼容 command、兼容 hook 或 `job-*` 新 ID。
- 产品中文仍使用“任务”，不要求把用户可见文案机械改为“运行”。
- 旧序列化枚举只有在 V1 目录识别/备份代码需要时才允许作为私有检测结构存在，不进入 V2 repository 或产品查询。
- `<runtime>/knowledge/jobs` 不再进入产品查询，但不删除目录内容。
- V1 history 备份失败时拒绝进入可写工程会话，不部分迁移、不静默清空。

## 4. 跨层数据流

```text
React/Tauri request
  -> RunApplicationService submit
  -> RunRepository create CAS
  -> handler execution/progress
  -> project file transaction
       -> immutable artifact files
       -> artifact-manifest.json
  -> RunRepository terminal CAS
       -> status/completedAt/failure/result
       -> exactly one terminal timeline event
  -> Tauri run event/snapshot
  -> React task UI
```

任何一步不能提交 manifest 时，Run 不得进入 `succeeded`；任何终态 CAS 失败时，不得通过 Audit、event 或另一份 JSON 声称不同终态。

## 5. RunRecord v2 契约

### 5.1 字段

```text
RunRecord
  schemaVersion = 2
  id = run-...
  kind: RunKind
  status: pending | running | succeeded | failed | cancelled
  createdAt
  startedAt?
  completedAt?
  payload
  progress?
  failure: ActionableFailure?
  result: RunResult?
  attempts
  timeline: RunTimelineEvent[]
```

- `id` 必须使用 `run-` 前缀，且可作为安全单段目录名。
- `payload` 按 `RunKind` 进行受控校验，不接受调用方伪造的任意结构作为内部契约。
- `RunResult` 使用 serde tagged enum；产物型结果只保存 `artifactManifestRef` 和 `manifestSha256` 及该 kind 必需的非重复摘要字段。
- `cancelled` 不写 `failure`；`failed` 必须写 `failure`；`succeeded` 必须写与 kind 匹配的 `result`。
- `startedAt` 只在进入 `running` 时写入；`completedAt` 只在终态 CAS 中写入。

### 5.2 最小 ActionableFailure

```text
ActionableFailure
  schemaVersion = 1
  code
  stage
  message
  retryable
  diagnosticRef?
```

- 本任务只产生 `run.interrupted`、`run.storage_failed`、`run.invalid_transition` 和按 handler 收口的 `run.execution_failed`；不得在本任务内预测完整 provider/IO 分类。
- `message` 必须是面向用户的有限长度脱敏摘要；`diagnosticRef` 只能是 `.ats/diagnostics/<run-id>/` 下的工程相对路径。
- Work Order 2 在同一 schema 上补充 `category`、`action`、`retryAfterMs` 和白名单 context，并将领域错误归一化为更具体的 code；不得恢复 `error: String` 第二真相源。

### 5.3 RunResult 变体

```text
RunResult
  serde = { "kind": "<snake_case variant>", ...variant fields }

  text_generation
    model / content / finishReason / usage

  artifact_production
    artifactManifestRef / manifestSha256 / artifactId / entityName
    model? / usage?

  batch_artifact_production
    total / succeeded / failed
    items[] = itemId / artifactManifestRef? / manifestSha256? / diagnosticRef?

  build
    projectRelativeRoot / steps[]
    artifactManifestRef? / manifestSha256?

  package
    artifactManifestRef / manifestSha256 / artifactId
    filesAdded / uncompressedBytes / packageBytes

  plan
    item / itemFileRef? / model / usage

  log_analysis
    model / report / logChars / truncatedChars / usage

  truth_snapshot_refresh
    gamePackId / snapshotId / cacheHit / sourceCount / indexCount
    toolVersions / warnings
```

- `artifact_production` 覆盖 `code_generate` 和 `asset_generate`，只保存 `artifactManifestRef`、`manifestSha256`、`artifactId`、`entityName` 及必要的 token/模型摘要；文件路径、Evidence、compile 输出和图片质量详情进入 manifest 或 diagnostics。
- `batch_artifact_production` 保存每个 item 的成功 manifest 引用或失败 diagnostic 引用及计数，不复制子产物 Evidence。
- `package` 的 ZIP 是正式产物，必须生成 manifest；`build` 只有在声明的构建输出形成可发布快照时才使用 manifest，否则结果只保存已脱敏的步骤摘要。
- `text_generation`、`plan`、`log_analysis` 和 `truth_snapshot_refresh` 保存各自受控字段；所有路径必须是工程相对路径，不得把完整 Prompt、原始上游 body、未标准化绝对路径或任意 `serde_json::Value` 作为 result。
- `RunResult` 与 `RunKind` 必须在 repository 终态校验中匹配；失败和取消不允许保存成功 result。

### 5.4 Timeline

```text
RunTimelineEvent
  kind = created | started | cancel_requested
       | succeeded | failed | cancelled | interrupted
  at
  stage?
  failureCode?
  cancellationReason? = user | project_close | project_switch | app_shutdown
```

- 创建 Run 时恰好追加一个 `created`。
- progress 更新不追加 timeline。
- 重复取消请求不重复追加 `cancel_requested`。
- 每个 Run 最多一个 terminal event。
- `interrupted` 是崩溃恢复专用 terminal event，对应 `status = failed`、`failure.code = run.interrupted`。
- terminal CAS 同时提交 `status`、`completedAt`、`failure/result` 和 terminal timeline event。

### 5.5 状态转换

| 当前状态 | 允许目标 | 必需 timeline | failure/result |
| --- | --- | --- | --- |
| 新建 | `pending` | `created` | 均为空 |
| `pending` | `running` | `started` | 均为空 |
| `pending` | `cancelled` | `cancel_requested`、`cancelled` | 均为空 |
| `running` | `succeeded` | `succeeded` | result 必填，failure 为空 |
| `running` | `failed` | `failed` 或 `interrupted` | failure 必填，result 为空 |
| `running` | `cancelled` | `cancel_requested`、`cancelled` | 均为空 |
| 任一终态 | 无 | 不追加 | 拒绝修改 |

## 6. ArtifactManifest 契约

### 6.1 存储布局

```text
artifacts/<artifact-id>/runs/<run-id>/
  artifact-manifest.json
  files/
    <immutable snapshot files>

.ats/diagnostics/<run-id>/
  <failed/cancelled run diagnostics>
```

- `artifact-id` 和 `run-id` 必须是无分隔符、无遍历的安全单段。
- 路径解析必须拒绝 symlink 跳出 Artifact Store。
- 同一 artifact 的后续成功 Run 新建独立目录，不覆盖旧成功快照。
- 失败或取消只写可清理 diagnostics，不生成 ArtifactManifest。

### 6.2 Schema

```text
ArtifactManifest
  schemaVersion = 1
  artifactId
  artifactKind
  producingRunId
  createdAt
  gameContext
    gamePackId
    gamePackSchemaVersion
    gamePackSha256
    snapshotId
    snapshotSchemaVersion
    sources
    indexes
    toolVersions
  evidence[]
    source
    symbol
    purpose
    boundedExcerpt
  generation
    provider
    model
    inputsSha256
  imageProcessing?
    processor
    modelSha256?
    runtimeVersion?
    fallback
  files[]
    role
    snapshotRelativePath
    publishedRelativePath?
    byteLength
    sha256
```

- manifest、Evidence 和路径字段不得包含凭据、完整 Prompt、完整模型输出、URL query/fragment 或未标准化绝对路径。
- `files[].snapshotRelativePath` 必须解析到当前 run 的 `files/` 内。
- 每个文件保存 byte length 和 SHA-256；读取时可复算验证。
- 本任务建立 `imageProcessing` schema 和空/已有 provenance 适配点，实际 ML provenance 完整接入属于 Work Order 4。

### 6.3 发布顺序

1. 在 staging/文件事务中生成正式工程输出、不可变快照和 manifest。
2. 校验 manifest schema、所有相对路径、byte length 和 SHA-256。
3. 原子发布工程输出与完整 run artifact 目录。
4. 计算已发布 `artifact-manifest.json` SHA-256。
5. 通过 terminal CAS 将 Run 置为 `succeeded`，result 只引用 manifest 相对路径与 digest。

步骤 1-4 任一失败必须回滚新正式输出和新 artifact run 目录，Run 进入 `failed`，不得留下部分成功证据。

## 7. V1 History 与 Audit/Evidence 删除

- 在持有工程 OS 锁且进入 V2 可写 session 前检测 V1 `history/`。
- 将整个 V1 目录原子重命名为 `history.v1-backup-<UTC timestamp>`，然后创建空 V2 history。
- 已存在 V2 history 时不得再次备份或混读 V1。
- 备份目录只用于人工追溯，不进入 list/get/cancel/reconciliation。
- 删除 `AuditEntry`、audited repository 和 Run 生命周期 audit 写入/查询入口。
- 删除生产路径中新建、读取或 UI 展示 `evidence.md` 的逻辑；历史文件不主动删除。
- 同一 artifact 首次按新结构成功发布时，事务性清理旧共享产物目录中的陈旧副本；不得删除历史备份或其他 artifact。

## 8. API 与命名迁移清单

- Core：`Job* -> Run*`、`job_application_service -> run_application_service`、repository/service/handler 方法与事件。
- Desktop：command 名、Rust payload/snapshot、managed state、event topic 和 ID 参数统一为 `run`。
- Frontend：`tauriApi.ts`、`webApi.ts`、API selector、hooks、store、类型和组件内部变量统一为 `run`。
- Web/CLI：所有仍编译到生产目标的 Job 契约同步迁移；不保留只为编译通过的兼容层。
- 文件：`job_lifecycle.rs -> run_lifecycle.rs`，相关 fixture 和 snapshot 名称同步更新。
- 文案：中文“任务”保留；代码标识和序列化字段不得继续产生新的 `job`。

代码调查完成后，将实际生产符号和文件清单补充到本 PRD 的“调查结果”。

## 9. 验证矩阵

| 场景 | 预期 |
| --- | --- |
| Good：asset Run 成功 | `pending -> running -> succeeded`，timeline 各事件恰好一次，manifest digest 可复算 |
| Good：同一 artifact 再次成功 | 新建不同 run-id 目录，旧 manifest/files 不变化 |
| Base：只有 V1 Job history | 整目录备份，建立空 V2 history，产品查询不返回旧记录 |
| Base：progress 多次更新 | progress 更新生效，timeline 不增长 |
| Bad：cancel 与 completion 竞争 | 只有一个 terminal CAS 成功，无状态/timeline 分叉 |
| Bad：旧终态再次完成/失败/取消 | 确定性拒绝，不修改文件和 timeline |
| Bad：manifest 或文件写入失败 | 文件事务回滚，Run 不得 succeeded，不产生 `evidence.md` |
| Bad：manifest 路径越界或 symlink | 发布前拒绝，不写 Artifact Store 外部路径 |
| Bad：V1 history 备份失败 | 拒绝 V2 可写 session，原目录保持可恢复 |
| Bad：敏感 canary | RunRecord、manifest、diagnostics 和导出内容不含 canary 原文 |

## 10. 最低定向验证

```text
cargo test -p ats-core --test run_lifecycle
cargo test -p ats-core platform::
cargo test -p ats-core project::
cargo check -p ats-core
cargo check -p agentthespire-desktop
npm run test:frontend
npx tsc -b --pretty false
```

- 只运行与本任务实际影响对应的定向子集。
- 不运行 workspace 全量测试、workspace clippy、完整前端生产构建或 Tauri bundle，除非先说明必要性并获得用户授权。
- 不执行真实游戏 UI；本任务不需要重新验证已完成的 Game Pack Stage 1。

## 11. 文档与规范同步

- 更新 `.trellis/spec/backend/quality-guidelines.md`：以 RunRecord/ArtifactManifest 契约替换 Job 和 `evidence.md` 现行口径。
- 更新 `docs/01-总览/项目架构总览.md` 的执行事实。
- 更新 `docs/02-现状/当前进度说明.md` 的完成项和剩余缺口。
- 更新 `docs/03-当前方案/通用Mod流水线与Game-Pack边界.md` 中 Evidence Record 的物理表达。
- Work Order 1 完成前不得把状态提前写成已完成。

## 12. 验收清单

- [ ] 实际生产符号、文件和调用链调查完成并写入本 PRD。
- [ ] RunRecord v2 字段、serde 形状和校验测试完成。
- [ ] 状态转换、CAS、timeline 去重和终态竞争测试完成。
- [ ] 所有生产 `Job*` 契约破坏性迁移为 `Run*`，无兼容别名或 fallback。
- [ ] Artifact Store 安全路径和不可变 run 快照完成。
- [ ] ArtifactManifest schema、文件哈希、Evidence 和发布顺序完成。
- [ ] 成功 Run result 只引用 manifest；失败/取消不生成 manifest。
- [ ] V1 history 整目录备份和 V2 空 history 完成。
- [ ] 产品不再读写 Audit 生命周期、legacy Job history 或 `evidence.md`。
- [ ] Core、Tauri、TypeScript 和相关目标通过本任务定向检查。
- [ ] 稳定文档与 backend code-spec 同步。

## 13. 停止条件

- Run 状态和 timeline 无法在同一次 repository CAS 中提交。
- 产物与 manifest 无法在 Run 成功前完成一致发布和校验。
- 任一生产路径必须依赖旧 Job history、Audit 生命周期投影或 `evidence.md` 才能工作。
- V1 history 无法在不丢失原目录的前提下完成可恢复备份。
- 实际代码证明范围必须扩展到 Work Order 2/3 的核心职责，且不能通过本任务内的最小边界解决。

触发停止条件时先汇报证据、影响范围和备选方案，不继续堆叠兼容层。

## 14. 调查结果

### 14.1 Relevant Specs

- `.trellis/spec/backend/quality-guidelines.md`：当前唯一包含 Rust 桌面生成、Evidence、图片和工具链可执行契约的后端规范；本任务必须把其中 `Job`、`evidence.md` 和任意 JSON result 口径改成 Run/Manifest。
- `.trellis/spec/guides/cross-layer-thinking-guide.md`：用于核对 Core -> Tauri -> TypeScript -> React 的字段、事件和错误边界。
- `docs/03-当前方案/2026-07-31-桌面后端运行时加固与发布收口方案.md`：Work Order 1、停止条件和后续任务边界。
- `docs/03-当前方案/通用Mod流水线与Game-Pack边界.md`：Evidence 必须绑定 Game Pack/Truth Snapshot，不得把 STS2 特例写入通用 Run/Artifact 层。
- frontend hook/state/type 规范仍是骨架，backend `type-safety.md` 不存在；本 PRD 的 schema、矩阵和跨层命名清单是本任务主要的类型安全门禁。

### 14.2 Existing Patterns

- `JobRepository::modify` 与 `handlers/common.rs` 已提供单次读改写和“取消不被成功收尾覆盖”的局部 CAS 模式，可演化为状态、timeline、failure/result 同提交的 Run transition API。
- `fs_atomic::{write_atomic, write_atomic_sync}` 提供同目录临时文件 + rename；Truth Snapshot store 已有 staging 目录、内容哈希、不可变目录发布和 pointer digest 模式，可复用于 Artifact Store。
- Truth Snapshot 的 `digest_tree` 和 package layout 已有拒绝 symlink、限制 root、规范化相对路径和 SHA-256 的实现模式。
- `VerifiedGameContext::evidence()` 已提供 Game Pack、Snapshot、source、index 和 tool version 的结构化事实；`PromptAssembler` 目前又把它和代码事实渲染成 Markdown，需拆成可序列化 Evidence 数据后供 prompt 与 manifest 两个消费者使用。
- `asset_bundle.rs::FileTransaction` 已能回滚正式工程文件，但诊断文件在事务外先写入；Artifact Store 发布需要独立 staging/commit 协调器，不能仅在现有 helper 上追加 manifest 写入。

### 14.3 Production Job/Run Symbol Inventory

Core 生产链：

- `platform/domain/{models,repository,errors}.rs`：`JobId/Kind/Status/Progress/Job/Summary/Error/Result/Repository`。
- `platform/application/job_application_service.rs`、`application/mod.rs`、`platform/mod.rs`、`contracts.rs`：service、submit/get/list/cancel、`SubmitJobAck` 和公开 re-export。
- 九个 handler：`text_generate`、`code_generate`、`asset_generate`、`batch_custom_code`、`build_project`、`package_project`、`single_asset_plan`、`log_analysis`、`truth_snapshot_refresh`；其中 build/package/plan/log/batch 仍手写 `get -> update` 终态。
- `platform/infra/file_job_repository.rs` 与 `audited_repository.rs`：V1 文件仓库和 Audit 装饰器。
- `audit.rs`：独立 Audit 类型、sink 和 `.ats/audit.log` 读写；`plan_artifact.rs` 是另一套功能，不随 Audit 删除。
- `project/folder.rs`：创建 `history/` 并持有工程 OS 锁，当前 open 没有 V1 history 识别或备份。

Desktop/Tauri：

- `src-tauri/src/commands/platform.rs`：9 个 `submit_*_job`、`get_job/list_jobs/cancel_job`、`job-progress`，并在每次 command 中新建 repository/Audit wrapper。
- `src-tauri/src/commands/audit.rs`：`audit_append/audit_read_recent` 与 plan artifact command 混放；只删除前两者并保留/重命名后者所属模块。
- `src-tauri/src/lib.rs`：全部 command 注册。

TypeScript/React：

- `src/services/tauriApi.ts` 与 `webApi.ts`：Job 类型、ack、commands 和 desktop-only stubs。
- `src/hooks/useJobProgress.ts`、`src/stores/workflow.ts`：event topic、ID 和跨页恢复状态。
- `JobsCard/JobsList/JobsSubmitForm`、`SingleAssetWorkflowCard`、`KnowledgeCard`、`BatchGenerationPage`、`LogAnalysisPage`、`SystemPage`：生产 Job API 消费者；产品中文“任务”保留。
- `AuditCard` 当前直接读取 `.ats/audit.log`，应删除独立 Audit 视图或改为直接渲染 Run timeline/ArtifactManifest，不能保留旧 command fallback。

### 14.4 Files to Modify

- 新建/重命名：`run_application_service.rs`、`file_run_repository.rs`、`tests/run_lifecycle.rs`，并在 platform domain 中定义 RunRecord v2、timeline、typed payload/result、最小 ActionableFailure。
- 新建 Artifact Store 模块，负责安全 ID/相对路径、staging、文件 digest、manifest 校验和不可变发布；复用通用哈希/路径规则，不把 STS2 逻辑放入该模块。
- 修改上述九个 handler，使全部状态迁移经过同一 Run transition API；产物型 handler 在成功 CAS 前完成 manifest 发布，失败诊断写入 `.ats/diagnostics/<run-id>/`。
- 修改 `PromptAssembler`/Evidence 数据模型和 asset bundle transaction，停止创建、返回和测试 `evidence.md`。
- 修改 `ProjectFolder::create/open` 及测试，建立 V2 history 标记和 V1 整目录备份。
- 删除 `audited_repository.rs` 和 Core Audit 生命周期实现；拆分 Tauri `commands/audit.rs`，保留独立的 plan artifact command。
- 全量迁移 Tauri commands/events、TypeScript service/hook/store/component 内部标识，并更新 command 注册和定向 frontend tests/E2E selectors。
- 同步 backend quality spec、架构总览、当前进度和 Game Pack 边界文档。

### 14.5 Contract Gaps And Implementation Order

1. 先定义纯领域 schema、合法转换和 `RunKind <-> payload/result` 校验，再替换 repository；禁止在 handler 中继续直接修改公开字段。
2. 先实现 V1 识别/可恢复目录备份，再允许 V2 repository 初始化；当前 JSON 没有 `schemaVersion`，因此不能与 V2 混读。
3. 先实现结构化 Evidence 和 Artifact Store staging，再迁移 asset/code/package 成功结果；当前 `artifacts/<entity>/` 会覆盖同名产物且把 raw/evidence/正式文件混放。
4. 再迁移所有 handler 到统一 terminal CAS；当前五个 handler 的 `get -> update` 会覆盖并发取消，build 还在失败状态中写成功形状的 result。
5. 最后破坏性切换 Tauri/TypeScript 命名并删除 Audit/evidence 入口，使用定向 compile/typecheck 暴露漏网符号。

已知边界：Tauri 当前每个 command 都新建 `FileJobRepository`，实例内 mutex 不能形成跨 command 锁域。Work Order 1 负责让单个 repository 的 Run 聚合和终态转换正确；共享 `Arc<RunRepository>`、关闭工程 cancel-and-drain 和全局竞争收口仍由 Work Order 3 完成。本任务验收不得声称已解决工程 session 级并发。
