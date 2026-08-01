# ProjectSession 与 Run 并发正确性

> 状态：completed
>
> 优先级：P0
>
> 开发类型：backend/fullstack
>
> 方案来源：[`桌面后端运行时加固与发布收口方案`](../../../docs/03-当前方案/2026-07-31-桌面后端运行时加固与发布收口方案.md) Work Order 3

## 1. 目标

建立长期 `ProjectSession` 作为活动工程运行时的唯一所有者，使工程锁、共享 `RunRepository`、后台 Run task、取消句柄和 session 状态具有一致生命周期。关闭或切换工程必须先拒绝新 Run、取消并等待全部工程 Run 收尾，排空后才释放 OS 锁；异常退出遗留的非终态 Run 在下次打开时确定性标记为 interrupted。

## 2. 已知事实

- `ProjectFolder` 已持有操作系统独占锁，但 Tauri `ActiveProject` 当前只保存 `Mutex<Option<ProjectFolder>>`。
- Tauri commands 会按调用重新构造 `FileRunRepository`，实例内 CAS 锁不能覆盖 cancel 与 handler completion 的竞争。
- RunRecord v2 已将 status、timeline、failure 和 result 收口到一次 repository transition；终态不可二次修改。
- Work Order 2 已提供 `project.closing`、`project.close_timeout`、`run.interrupted` 等结构化失败基础，但没有实现 session 行为。
- 用户已确认：关闭工程后 job/Run 不应继续执行；cancel-and-drain 上限为 30 秒，超时后保持 closing 和工程 OS 锁；不提供强制解锁入口。
- 全局 ML prewarm 不读写工程，不属于 ProjectSession Run，关闭工程后可以继续。

## 3. 已确认设计

- `ProjectSession` 放在 Tauri composition/runtime 层，Core 保留可复用的 Run repository、取消、受控进程和 reconciliation 原语，不让 Core 依赖 Tauri。
- 本任务允许破坏性修改内部 command state 和 service 构造方式，不保留旧 `ActiveProject` 兼容 API。
- `open_project` 和 `create_project` 在已有 session 时采用与关闭相同的 drain 协议；旧 session 完成排空并释放锁后才创建目标 session。
- session lifecycle 由一个 async mutex 串行化，状态只允许 `open -> closing`；close 超时不回到 open。
- 用户取消、工程关闭、工程切换和应用退出都先向 token 发布原因，再由 handler 完成清理，最后通过唯一 repository CAS 写入终态。
- Windows 外部进程使用原生 Job Object 终止整个进程树。具体采用 `process-wrap` 的 Tokio `JobObject + KillOnDrop` 安全封装，避免违反 workspace 的 `unsafe_code = "forbid"`，并避免普通 spawn 后再分配 Job 的竞态。

## 4. 调查结论

- `RunApplicationService` 的九个 submit 入口都直接 `tokio::spawn` 并丢弃 `JoinHandle`，必须统一返回可注册的 `SpawnedRun`。
- 当前 command 每次调用都重新构造 `FileRunRepository`；实例内 `write_lock` 因而不能覆盖 cancel 与 handler completion 的 CAS 竞争。
- 当前 cancel 立即把 RunRecord 写为 `cancelled`，handler 只在部分 LLM stream 每五个事件轮询 repository；无事件的 stalled stream 无法及时退出。
- build/asset compile 使用 `std::process::Command::output + spawn_blocking`，package 在单个 blocking closure 中完成 ZIP，Truth Snapshot 的下载、工具检查、反编译和 finalize 均没有统一 cancellation token。
- `RunTransition::Interrupt` 只接受 Running；恢复 Pending 时还会违反 Failed 必须有 `startedAt` 的当前校验。
- Tauri 当前使用 `.run(...).expect(...)`，没有 `RunEvent::ExitRequested` drain hook；Tauri 2 可通过 `prevent_exit()` 后异步排空，再调用 `app.exit(code)`。
- 全局 ML prewarm 不读写工程目录，保持在 `ProjectSession` 之外。

## 5. 需求（演进中）

- 一个活动工程只有一个 `Arc<ProjectSession>` 和一个共享 `Arc<FileRunRepository>`。
- session 状态至少为 `open | closing`；进入 closing 后所有新 Run 提交确定性失败。
- 每个工程 Run 注册 cancellation handle 和完成通知，直到 handler 与其文件/进程清理完全退出。
- `close_project`、工程切换和正常应用退出复用 cancel-and-drain 核心能力。
- 30 秒超时返回 `project.close_timeout`，保持 session、OS 锁和 closing 状态，并报告阻断 Run。
- 下次打开工程时把遗留 Pending/Running Run 通过唯一 CAS 转为 failed/interrupted。
- 不影响全局 ML prewarm。

## 6. 可执行契约

### 6.1 Core 类型与签名

```text
CancellationToken
  cancel(reason: CancellationReason) -> bool
  reason() -> Option<CancellationReason>
  cancelled() -> Future<CancellationReason>

SpawnedRun
  run_id: RunId
  cancellation: CancellationToken
  task: JoinHandle<()>

RunApplicationService::submit_*(..., cancellation: CancellationToken)
  -> RunRepositoryResult<SpawnedRun>

RunApplicationService::request_cancel(id, reason)
  -> RunRepositoryResult<RunRecord>

FileRunRepository::reconcile_interrupted(failure_factory)
  -> RunRepositoryResult<Vec<RunId>>
```

- `CancellationToken` 首次取消原因获胜，后续取消调用只负责重复唤醒，不覆盖来源。
- handler 观察 token 后先停止网络/文件/child process，并等待清理完成，再尝试 `RunTransition::Cancel`；若成功/失败 CAS 已获胜则保持既有终态。
- reconciliation 对 Pending/Running 都执行 `Interrupt`。Pending interrupted 保持 `startedAt = None`，校验允许且只允许 `failure.code = run.interrupted` 的这种 Failed 记录。

### 6.2 Desktop 所有权

```text
ActiveProject
  lifecycle: tokio::sync::Mutex<()>
  current: RwLock<Option<Arc<ProjectSession>>>

ProjectSession
  project: ProjectFolder
  repository: Arc<FileRunRepository>
  tasks: RunTaskScope
  state: Atomic(Open | Closing)
  snapshot: ProjectSnapshot

RunTaskScope
  accepting: bool
  tasks: BTreeMap<RunId, { cancellation, JoinHandle }>
```

- 所有 command 先获取 `Arc<ProjectSession>` 快照，禁止跨 `await` 持有 active 锁。
- submit 在同一 task-scope 临界区内检查 accepting 并注册 `SpawnedRun`，不能出现“已创建 Pending 但 session 不知道 task”的窗口。
- `get/list/cancel` 和所有提交共享 session 内唯一 repository 实例。

### 6.3 Close / Switch / Exit 序列

```text
lock lifecycle
  -> current session enter closing
  -> task scope reject submit
  -> publish cancellation reason to every task
  -> wait all JoinHandle until cleanup complete (30s)
  -> success: remove current session, drop ProjectFolder, emit project-changed
  -> timeout: keep current session + closing + OS lock, return project.close_timeout
```

- switch 使用 `ProjectSwitch`，显式 close 使用 `ProjectClose`，应用退出使用 `AppShutdown`。
- timeout context 只包含受白名单约束的阻断 `runId`；不提供 force unlock。
- retry close 对 closing session 再次 cancel-and-drain。
- 应用退出 hook 只拦截第一次退出；排空成功后由内部标记放行 `app.exit(code)`，避免递归阻止。

### 6.4 外部进程与 blocking work

- Windows `dotnet/ilspycmd` 使用 `process-wrap` Tokio Job Object；取消分支调用 `kill()` 并 `wait()`，只有 wait 完成才算 handler 退出。
- 非 Windows 使用 process group/session wrapper 提供等价的进程树终止语义。
- package ZIP 在 entry 和固定大小复制块边界检查 token；取消时删除临时 ZIP，不发布 output。
- LLM request 建立和每次 `stream.next()` 都与 `token.cancelled()` 做 `tokio::select!`，drop response stream 后退出。
- `spawn_blocking` 任务必须接收 token 并在可回滚边界检查；不能用 abort `JoinHandle` 伪装底层工作已停止。

### 6.5 错误矩阵

| 场景 | 稳定结果 |
| --- | --- |
| closing 后提交 | `project.closing`，不创建 RunRecord |
| 用户取消活动 Run | 清理完成后 `cancelled`，timeline 原因为 `user` |
| close/switch/shutdown 取消 | 清理完成后 `cancelled`，timeline 保留对应原因 |
| close 30 秒未排空 | `project.close_timeout`，session/锁保持，context.runId 指向阻断 Run |
| 打开遗留 Pending/Running | `failed + run.interrupted`，唯一 interrupted event |
| cancel 与 success/failure 同时完成 | 仅一个 terminal CAS 成功，另一方识别既有终态 |
| child kill/wait 失败 | task 不得报告已排空；close 最终超时并持锁 |
| repository 写盘失败 | `run.storage_failed`，session 不释放锁直至 task 实际退出 |

## 7. 验收标准

- [x] cancel 与 handler completion 并发时只有一个终态 CAS 成功，terminal timeline 恰好一次。
- [x] 关闭/切换成功返回后没有工程 Run、文件事务或 child process 继续运行，OS 锁才释放。
- [x] drain 超时后第二进程仍无法打开工程，新 Run 被拒绝，重试取消仍可执行。
- [x] 打开含遗留 Pending/Running 的工程后，它们确定性进入 failed + `run.interrupted`。
- [x] 全局 ML prewarm 不注册到 ProjectSession，也不访问活动工程目录。
- [x] Core/Desktop/TypeScript 定向测试和 code-spec 同步通过。

### Good / Base / Bad

- Good：运行中的 LLM、ZIP 和 build Run 在 close 时收到对应 reason，回滚/终止并退出，随后工程锁释放。
- Base：无活动 Run 的工程 close 立即排空；只有全局 ML prewarm 时不等待它。
- Base：遗留 Pending 和 Running fixture 在 session 发布前各追加一次 interrupted terminal event。
- Bad：在 closing 与 submit 的 barrier 竞态中，不得出现未注册的 Pending Run。
- Bad：cancel 与 handler completion 同时过 barrier，终态 event 总数必须恰好为一。
- Bad：受控 handler 超过 30 秒不退出，close 返回 timeout，第二个 opener 仍得到 project locked。

## 8. 不纳入

- Work Order 4 的 BuildInfo、ML provenance、baseline/ML installer。
- Work Order 5 的完整 workspace/release candidate 验证。
- 自动清理成功 Artifact Store 或 diagnostics。
- detached session registry、强制释放工程锁或后台继续旧工程 Run。
- 真实游戏 UI 操作。

## 9. 实施顺序

1. Run domain：Pending interrupt 规则、CancellationToken、reconciliation 与定向测试。
2. Run application：九个 submit 返回 SpawnedRun；common finalize 使用 token/CAS。
3. Desktop composition：ProjectSession、RunTaskScope、共享 repository 和 command getter 迁移。
4. close/create/open/switch：30 秒 cancel-and-drain、closing 拒绝与 OS 锁测试。
5. LLM/ZIP/Truth Snapshot 取消检查点和受控 child process abstraction。
6. Tauri exit hook、前端 close 状态兼容、code-spec 和稳定文档同步。

每一步保持 `ats-core` 与 desktop 可编译；不保留旧 `ActiveProject` 字段或每 command repository fallback。

## 10. 最低定向验证

```text
cargo test -p ats-core platform::domain::models::tests
cargo test -p ats-core platform::infra::file_run_repository::tests
cargo test -p ats-core platform::application::handlers::<affected>::tests
cargo test -p agentthespire-desktop commands::project::tests
cargo test -p agentthespire-desktop project_session::tests
cargo check -p ats-core
cargo check -p agentthespire-desktop
npx tsc -b --pretty false          # 仅当前端 command 签名发生变化时
```

不运行 workspace 全量测试、全量 clippy、Tauri bundle 或真实游戏 UI。

## 11. 技术备注

- 目标方案与停止条件见运行时收口方案 Work Order 3。
- 相关规范：`.trellis/spec/backend/quality-guidelines.md`、`.trellis/spec/backend/error-handling.md`、`.trellis/spec/guides/cross-layer-thinking-guide.md`。
- 相关稳定文档：`docs/01-总览/项目架构总览.md`、`docs/02-现状/当前进度说明.md` 和运行时收口方案。

## 12. 完成证据（2026-08-01）

- Core/Desktop 编译：`ats-core --no-default-features`、`agentthespire-desktop --no-default-features`、`ats-core --features ml-rembg` 均通过。
- 取消与终态：CancellationToken 2 项、cancel/success 并发 CAS 1 项通过。
- Truth Snapshot：refresh 15 项、application handler 2 项通过；覆盖 stalled HTTP body 取消、旧 current 保留和 worker 退出后才写 Cancelled。
- 图片处理：`image_proc` 25 项及阻塞背景处理取消 1 项通过；取消等待真实 worker 退出后清理 partial diagnostics。
- 进程树：Windows Job Object 父子进程树取消测试 1 项通过。
- Desktop 生命周期：ProjectSession 4 项、AppShutdown 4 项、工程切换 command 1 项通过；覆盖 submit/closing barrier、timeout 持锁、退出防重入、成功排空释放锁和 `ProjectSwitch` reason。
- code-spec、架构总览、当前进度、当前方案和历史生命周期 ADR 已同步。TypeScript command 签名未变化，因此本任务没有新增 TypeScript 编译边界。
- 本轮 20 个非模块入口 Rust 文件的独立 rustfmt 检查和 `git diff --check` 通过；模块入口由三组 `cargo check` 覆盖。全仓 `cargo fmt --check` 仍被未改动的 `crates/ats-core/src/image_gen/chat_image.rs` 既有格式漂移阻断，本任务未越界格式化该文件。

未执行：完整 workspace 测试、全量 clippy、完整 Tauri build、双桌面进程 UI 冒烟和真实窗口退出冒烟。前四类重型/人工门禁不作为本 Work Order 自动完成的替代证据；窗口和双进程行为保留给后续人工/候选验收。
