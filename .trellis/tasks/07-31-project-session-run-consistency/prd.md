# ProjectSession 与 Run 并发正确性

> 状态：planning
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

## 3. 临时假设

- `ProjectSession` 放在 Tauri composition/runtime 层，Core 保留可复用的 Run repository、取消和 reconciliation 原语，不让 Core 依赖 Tauri。
- 本任务允许破坏性修改内部 command state 和 service 构造方式，不保留旧 `ActiveProject` 兼容 API。
- `open_project` 在已有 session 时采用与关闭相同的 drain 协议，成功后再打开目标工程；失败或超时保持原 session。

## 4. 待调查问题

- 所有后台 Run 的 spawn 入口、取消检查点和 task 退出时机是否能统一注册为 lease。
- build/package 等外部 child process 当前是否保留可终止句柄，Windows 下是否需要进程树终止。
- `ProjectFolder::open` 和 `FileRunRepository` 的恢复入口能否在 session 可写前完成 reconciliation。
- 应用退出 hook 当前能提供何种异步 drain 保证，是否需要与显式 close 分成两个可测试入口。
- 前端当前 close/switch 协议需要哪些 closing、blocked run 和 retry 状态字段。

## 5. 需求（演进中）

- 一个活动工程只有一个 `Arc<ProjectSession>` 和一个共享 `Arc<FileRunRepository>`。
- session 状态至少为 `open | closing`；进入 closing 后所有新 Run 提交确定性失败。
- 每个工程 Run 注册 cancellation handle 和完成通知，直到 handler 与其文件/进程清理完全退出。
- `close_project`、工程切换和正常应用退出复用 cancel-and-drain 核心能力。
- 30 秒超时返回 `project.close_timeout`，保持 session、OS 锁和 closing 状态，并报告阻断 Run。
- 下次打开工程时把遗留 Pending/Running Run 通过唯一 CAS 转为 failed/interrupted。
- 不影响全局 ML prewarm。

## 6. 验收标准（演进中）

- [ ] cancel 与 handler completion 并发时只有一个终态 CAS 成功，terminal timeline 恰好一次。
- [ ] 关闭/切换成功返回后没有工程 Run、文件事务或 child process 继续运行，OS 锁才释放。
- [ ] drain 超时后第二进程仍无法打开工程，新 Run 被拒绝，重试取消仍可执行。
- [ ] 打开含遗留 Pending/Running 的工程后，它们确定性进入 failed + `run.interrupted`。
- [ ] 全局 ML prewarm 不注册到 ProjectSession，也不访问活动工程目录。
- [ ] Core/Desktop/TypeScript 定向测试和 code-spec 同步通过。

## 7. 不纳入

- Work Order 4 的 BuildInfo、ML provenance、baseline/ML installer。
- Work Order 5 的完整 workspace/release candidate 验证。
- 自动清理成功 Artifact Store 或 diagnostics。
- detached session registry、强制释放工程锁或后台继续旧工程 Run。
- 真实游戏 UI 操作。

## 8. 技术备注

- 目标方案与停止条件见运行时收口方案 Work Order 3。
- 相关规范：`.trellis/spec/backend/quality-guidelines.md`、`.trellis/spec/backend/error-handling.md`、`.trellis/spec/guides/cross-layer-thinking-guide.md`。
- 实施前必须补齐具体签名、状态机、关闭/切换序列、错误矩阵、Good/Base/Bad 和定向测试。
