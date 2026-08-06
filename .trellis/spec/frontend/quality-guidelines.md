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
- 隔离 E2E runner 必须显式准备并校验 pinned fixture、Provider API base path 与响应模式，不得要求生产代码为测试伪造 Truth/Resource。
- Batch 模型测试至少覆盖 request 构造、精确 hash 失败重试、fail-fast 未执行差集和 malformed
  schema/counter canary。
- Resource Workbench 模型测试必须证明 required derived roles 自动包含 Pack-owned master，且
  `packDefaultAvailable` 仅控制通用 Default 操作；不得通过 Character/STS2 字面量决定按钮或角色。

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
