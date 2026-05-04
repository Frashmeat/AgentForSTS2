# Component Guidelines

> How components are built in this project.

---

## Overview

<!--
Document your project's component conventions here.

Questions to answer:
- What component patterns do you use?
- How are props defined?
- How do you handle composition?
- What accessibility standards apply?
-->

(To be filled by the team)

---

## Component Structure

<!-- Standard structure of a component file -->

管理端页面的成功、失败、警告提示统一通过 `frontend/src/pages/admin/AdminLayout.tsx` 注入的 `useAdminLayoutContext()` 触发：

- `onStatusNotice` 是管理端通知的唯一入口，对应应用根部的 `StatusNoticeStack`。
- `onConfirm` 是管理端二次确认的唯一入口，对应应用根部的 `ConfirmDialog`。
- 管理端页面不要再维护一次性 `error/message` 状态条，也不要直接调用 `window.confirm`。
- 页面内的业务状态展示可以保留，例如健康状态徽标、不可用原因提示、接口返回的最后错误字段；这些是数据展示，不是操作通知。

---

## Props Conventions

<!-- How props should be defined and typed -->

(To be filled by the team)

---

## Styling Patterns

<!-- How styles are applied (CSS modules, styled-components, Tailwind, etc.) -->

(To be filled by the team)

---

## Accessibility

<!-- A11y requirements and patterns -->

(To be filled by the team)

---

## Common Mistakes

<!-- Component-related mistakes your team has made -->

(To be filled by the team)
