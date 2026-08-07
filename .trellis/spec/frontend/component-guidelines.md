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

### Pack-Driven Item Editor

`src/pages/ModEditorPage.tsx` renders Item types and canonical fields exclusively from
`ItemCapabilityCatalog`. Game-specific type IDs and option lists are forbidden in React.

```text
getItemCapabilities
  -> ready/blocked descriptor
  -> createItemDraft(descriptor)
  -> generic field controls
  -> saveItemDefinition
  -> StoredItemDefinition(definitionHash + definition)
```

- `text`, `integer`, `boolean`, `choice` and `string_list` are the complete generic canonical-field
  control switch. `localizationFields` independently drives localized inputs, labels, multiline
  behavior and required completeness; React must not assume every type has only name/description.
  Adding a game Item type must not add another component branch.
- `referenceSlots` and `compositionProfiles` are generic typed descriptor contracts. Their first
  editing surface may arrive in Character Studio, but React must render slot/profile metadata and
  parameter bounds rather than branch on `character`, `card` or STS2.
- `resourceProfileField` selects one Pack-declared `resourceProfiles[]` entry from the current
  canonical choice. Resource Workbench receives only that profile's required roles; it cannot use
  the union or a Character-specific list.
- Blocked descriptors remain visible with `capabilityReason`, but new/save/generation actions are
  disabled. The backend repeats the authoritative readiness check.
- Item lists show the current ItemDefinition v2 pointer and definition hash. Historical v2 definitions are loaded by
  `itemId + definitionHash`; React never mutates a previously returned snapshot in place.
- Primary locale edits mark derived translations outdated. Editing an already confirmed secondary
  locale also makes it outdated; `confirmLocalization` is the only transition back to confirmed.
- A translation whose `translatedFrom` locale is absent is a client-visible Draft issue and cannot
  be submitted as a malformed Tauri argument.
- Resource Workbench renders role, media shape, `packDefaultAvailable` and source dependencies from
  `ResourceCatalog`; it must not branch on `relic`, Character or another game type. Upload, Pack
  default and AI create candidates, preview is lazy, and only explicit select may write a role-keyed
  ItemDefinition binding. The Default control submits `{kind:"pack_default"}` without opening a file
  dialog or attaching `sourcePath`.
- Generation accepts only the current saved `StoredItemDefinition`. Unsaved edits, missing roles,
  stale selected pointers and Pack-shape mismatch keep the action disabled; backend preflight is
  authoritative and repeats the same gate before creating a Run.

### Visual Batch And Complex

`src/pages/BatchGenerationPage.tsx` selects current `StoredItemDefinition` pointers from the Item
Library. React contains no editable JSON request state and no game-specific type branches.

- Batch request items use deterministic `artifactId = itemId`; the exact definition hash is visible
  in the selection and read-only typed request preview.
- Pack-declared but blocked Item types remain visible and disabled. Empty behavior intent also keeps
  selection disabled; desktop preflight remains authoritative.
- Complex embeds the same Batch request and derives Package `modId` from the open project.
- Child rows are loaded through `getRun` using IDs from a validated terminal result. Never invent a
  failed/skipped Run from progress or from the request/result difference.
- Retry copies only failed inputs from the previous parent Run request. It must not substitute a
  newer Item Library pointer.

### Pack-Driven Composition Studio

`src/pages/CompositionStudioPage.tsx` renders `compositionProfiles` and Draft nodes without game or
Item-type branches. The Pack default profile is selected initially; Custom starts from the declared
`customBaseProfile` and uses only Pack min/max/constraint metadata.

- Draft list/get/update/delete and confirmation use the ProjectSession-owned repository. React sends
  the exact revision for every mutation and refreshes after a typed conflict.
- Type/status/search filtering, pagination and batch selection are generic Draft projections. Partial
  confirmation is enabled only for a pinned-closed selection; the backend repeats authoritative
  Draft/Pack/readiness/graph validation.
- A model Plan creates Draft state only. Review edits and replacements do not write Item current
  pointers until explicit atomic confirmation.
- Targeted retry sends the exact Draft revision, selected Item ID and bounded user instructions.
  The UI waits for the new persisted `composition.retry-node` Run terminal state, refreshes the
  Draft only on success and never submits a whole-plan response as single-node evidence.
- An empty Pack `compositionProfiles` catalog renders an unavailable state. React must not synthesize
  a Character workflow before the Pack declares one.
- Whole-closure generation lists only confirmed definitions whose type is a Pack-declared
  composition root and whose definition carries composition-profile provenance. The request pins
  the exact root hash plus optional matching Draft revision; React does not enumerate dependencies
  or author a second graph.
- Generation displays the persisted `composition.generate` Run status and safe failure code/stage.
  A returned Run ID or completed polling loop is not success evidence.
- Planning likewise retains and renders the persisted `composition.plan` terminal Run. Safe v1
  failure details may add reason/expected/actual/Item/slot context; malformed or unknown details
  fall back to code/stage and never cause raw Provider output to be rendered.

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
