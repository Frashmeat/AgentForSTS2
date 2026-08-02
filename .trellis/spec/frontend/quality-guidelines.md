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

---

## Testing Requirements

- 运行 `npm run test:frontend` 执行 `scripts/frontend/*.test.mjs`。
- 纯 TypeScript 模块由现有 Vite SSR loader 加载，断言使用 Node 内置 `node:test`；需要 DOM 交互测试时再评估专用测试依赖。
- 敏感字段表单至少覆盖：未触碰不写回、输入新值、明确清空三种情况。
- GUI E2E 的 Run 失败路径必须等待列表读到真实 `failed` RunRecord，并核对 typed failure/diagnostic；测试不能直接读 history 文件来绕过陈旧 UI 状态。

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
