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

---

## Testing Requirements

- 运行 `npm run test:frontend` 执行 `scripts/frontend/*.test.mjs`。
- 纯 TypeScript 模块由现有 Vite SSR loader 加载，断言使用 Node 内置 `node:test`；需要 DOM 交互测试时再评估专用测试依赖。
- 敏感字段表单至少覆盖：未触碰不写回、输入新值、明确清空三种情况。

---

## Code Review Checklist

<!-- What reviewers should check -->

(To be filled by the team)
