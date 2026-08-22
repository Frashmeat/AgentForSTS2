# Quality Guidelines

> Code quality standards for frontend development.

---

## Overview

<!--
Document your project's quality standards here.

Questions to answer:
- What patterns are forbidden?
- What linting rules do you enforce?
- What are your testing requirements?
- What code review standards apply?
-->

(To be filled by the team)

---

## Forbidden Patterns

- 不得把后端返回的脱敏 secret/token 字符串装入可编辑表单值；掩码只用于“已配置”提示。
- 不得根据掩码值是否变化判断 secret 是否需要写回，应使用显式 `touched` 状态。

---

## Required Patterns

- secret/token 输入框初始值为空；未触碰表示保留原值，触碰后精确写入用户输入，包括空字符串清空语义。
- 可独立验证的表单转换与 patch 组装逻辑应抽成纯函数。
- `run-progress` 事件只用于提示刷新，不能作为 Run 终态权威；事件可能先于后端 terminal CAS 到达。Runs UI 在列表存在 `pending` / `running` 时必须继续有界轮询 `listRuns()`，读到全部终态后停止。
- Shared polling for one submitted Run must not default to a fixed timeout shorter than the worst-case backend Feature budget. It reads the persisted `RunRecord` until terminal by default; only callers with a separate explicit deadline contract, or tests, pass a polling timeout.
- 选中详情和列表状态必须来自持久化 `RunRecord`。不得因为收到包含 `error` / `failed` 的 progress stage 就在前端自行伪造 failed payload。
- Batch/Complex 不得维护可编辑 JSON request 或 caller-authored Plan。Item 选择、只读 preview、
  child Run 展示和失败重试必须由纯模型函数覆盖；重试复用父 Run request 中的 exact
  definition snapshot。

---

## Testing Requirements

- 运行 `npm run test:frontend` 执行 `scripts/frontend/*.test.mjs`。
- 纯 TypeScript 模块由现有 Vite SSR loader 加载，断言使用 Node 内置 `node:test`；需要 DOM 交互测试时再评估专用测试依赖。
- 敏感字段表单至少覆盖：未触碰不写回、输入新值、明确清空三种情况。
- GUI E2E 的 Run 失败路径必须等待列表读到真实 `failed` RunRecord，并核对 typed failure/diagnostic；测试不能直接读 history 文件来绕过陈旧 UI 状态。
- GUI E2E 使用稳定 `data-testid` 作为工作流合同；Shell route、页面拆分或控件替换时必须在同一 Order 更新 E2E，顺序型 suite 应 fail fast，避免 setup 失败产生级联超时。
- GUI E2E must wait for the semantic ready state it consumes, not only for a control shell to
  exist. For an asynchronously populated select, wait until the exact target option exists and is
  enabled before changing its value; an empty select rendered before capability loading is not ready.
- 重复提交必须同时等待新的 Run ID 和持久化 terminal status；页面上残留的上一条 terminal Run 不能作为本次结果。
- Run polling tests must cover a real terminal state that arrives after the former frontend wait window, plus deterministic failure for an explicit caller deadline. Increasing Provider retries, backoff or external-tool budgets requires checking every frontend consumer in the same change.
- Composition Plan 提交必须保留本次 persisted terminal Run；失败不得因 Draft 未创建而在 Studio 中静默消失，安全 details 展示需有纯函数 canary。
- Composition Generate v7 提交后必须像 Plan 一样轮询对应 `ExecutionGraphView`。通用 Resume
  返回新 Run ID 后，前端以 validated `featureId` 选择 Plan 或 Generate monitor；不得默认按
  Plan 刷新 Draft，也不得通过重新提交原 root request 伪造节点恢复。API canary 必须固定
  generate request schema v6、内部 `until_passed` policy wire、`feedbackPhase`、逐 Item
  `adjustableItems` 和 `submit_composition_item_feedback` / `resume_execution_graph` command 名称。普通 UI
  不得显示 Graph/Run/node/role/compiler code、semantic round 或 repair policy controls。
- GUI E2E must follow the semantic feedback policy. A repairable output-contract failure creates a
  failed child Run but may keep the same parent Run/Graph active until success; the test must assert
  the persisted feedback state and same parent identity instead of waiting for Paused. Only a typed
  no-progress, exhausted-policy or system failure may drive the explicit Resume path.
- 隔离 E2E runner 必须显式准备并校验 pinned fixture、Provider API base path 与响应模式，不得要求生产代码为测试伪造 Truth/Resource。
- The dedicated Tauri E2E window must start hidden, unfocused and absent from the taskbar so a full
  desktop IPC run does not interrupt the interactive user session. These flags belong only to
  `tauri.e2e.conf.json`; the production window must remain visible and focusable by default.
- Batch 模型测试至少覆盖 request 构造、精确 hash 失败重试、fail-fast 未执行差集和 malformed
  schema/counter canary。
- Resource Workbench 模型测试必须证明 required derived roles 自动包含 Pack-owned master，且
  `packDefaultAvailable` 仅控制通用 Default 操作；不得通过 Character/STS2 字面量决定按钮或角色。

## Single Run Polling Contract

`src/services/runPolling.ts::waitForRun(runId, onUpdate?, options?)` is the shared consumer for a
submitted Feature Run. `options.timeoutMs` is optional; omission means polling the validated,
persisted `RunRecord` until `succeeded`, `failed` or `cancelled`. `options.intervalMs` controls only
read frequency. `readRun`, `sleep` and `now` are dependency seams for deterministic tests and do not
change the production Run authority.

| Case | Persisted state / option | Required result |
| --- | --- | --- |
| Good | `running` remains valid beyond an old UI deadline, then becomes terminal | Continue polling and return the exact terminal record; every observed record reaches `onUpdate` |
| Base | No `timeoutMs` is supplied | Use no frontend deadline; backend Run lifecycle, cancellation and reconciliation remain authoritative |
| Bad | Caller supplies an explicit deadline and no terminal record arrives before it | Throw `run polling timed out`; do not fabricate or mutate a terminal Run |
| Error | `get_run` transport or runtime validation fails | Propagate the actionable IPC failure; do not retain it as a fake Run status |

Run `npm run test:frontend` and keep `scripts/frontend/run-polling.test.mjs` assertions for both the
Good/Base terminal path and the explicit Bad deadline. Any backend retry, timeout, build or package
budget change must review this contract and every `waitForRun` caller in the same change.

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
