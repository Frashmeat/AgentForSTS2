# Compatibility Layer Cleanup Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不打断现有回退能力的前提下，逐步清理迁移期兼容层，收敛到前端 `shared/* + features/*` 与后端 `app/modules/*` 作为主干。

**Architecture:** 采用分阶段收缩策略。先删除纯转发壳，再把仍承载主体逻辑的 legacy page 迁回 feature 层，随后在迁移开关下线前收口 websocket 兼容层，最后再让 router 直接依赖模块化内核并移除后端 facade。

**Tech Stack:** React 18, TypeScript, Vite, Python, FastAPI, pytest

---

## Scope And Decisions

本计划固定采用以下决策，不再在执行中反复讨论：

- 前端 `lib/ws.ts`、`lib/batch_ws.ts` 迁移期先保留，等批量流程收口且迁移开关下线后再删除。
- `pages/*` 只先删除纯壳文件，不把 `BatchMode.tsx` 和纯壳 page 混在一轮处理。
- 后端 `agents/*` 采用分步退出，不做 router、tests、facade 一次性清除。

## File Map

### Frontend Core

- `frontend/src/shared/ws/facade.ts`
  当前 websocket 主干抽象，后续要成为统一入口。
- `frontend/src/shared/api/config.ts`
  迁移开关读取入口，最终要配合下线旧开关。
- `frontend/src/App.tsx`
  单资产装配层，当前仍保留 `use_unified_ws_contract` 回退逻辑。

### Frontend Compatibility Layer

- `frontend/src/lib/ws.ts`
  单资产 websocket 兼容壳，当前仍有回退价值。
- `frontend/src/lib/batch_ws.ts`
  批量 websocket 兼容壳，当前仍被批量流程主逻辑依赖。
- `frontend/src/pages/ModEditor.tsx`
  纯转发 page，优先删除。
- `frontend/src/pages/LogAnalysis.tsx`
  纯转发 page，优先删除。
- `frontend/src/pages/BatchMode.tsx`
  名义上是 page，实际上仍承载批量流程主体逻辑，需先迁移后再处理。

### Frontend Feature Layer

- `frontend/src/features/mod-editor/view.tsx`
  ModEditor 正式 feature 入口。
- `frontend/src/features/log-analysis/view.tsx`
  LogAnalysis 正式 feature 入口。
- `frontend/src/features/batch-generation/view.tsx`
  当前只是反向导出 `pages/BatchMode.tsx`，需要收回主体逻辑。

### Backend Compatibility Layer

- `backend/agents/planner.py`
  规划 facade，仍被 router 与测试依赖。
- `backend/agents/code_agent.py`
  代码生成 facade，仍被多个 router 依赖。

### Backend Core And Callers

- `backend/routers/workflow.py`
  单资产工作流路由，当前仍依赖旧 facade。
- `backend/routers/batch_workflow.py`
  批量工作流路由，当前仍依赖旧 facade。
- `backend/routers/build_deploy.py`
  构建部署路由，当前仍依赖旧 facade。
- `backend/app/modules/planning/*`
  规划主干。
- `backend/app/modules/codegen/*`
  代码生成主干。
- `backend/app/shared/infra/feature_flags.py`
  迁移开关定义位置，最终要清理或收口。

### Tests And Docs

- `frontend/tests/batchSocket.test.ts`
  批量 websocket 兼容相关测试。
- `frontend/tests/workflowSocket.test.ts`
  单资产 websocket 兼容相关测试。
- `backend/tests/test_planner.py`
  规划兼容入口测试。
- `backend/tests/test_review_fixes.py`
  代码生成兼容入口相关测试。
- `docs/02-现状/当前项目结构报告.md`
  需要在最终收口后更新结构描述。

## Execution Order

1. 先删纯壳 page，降低重复层级。
2. 再把 Batch 主体逻辑迁回 feature 层，理顺目录边界。
3. 然后收口前端 websocket 兼容壳。
4. 再让 backend router 改为依赖模块化内核入口。
5. 最后下线迁移开关，删除无人引用 facade，更新文档。

## Chunk 1: Remove Pure Page Shells

### Task 1: 清理 ModEditor 与 LogAnalysis 纯转发 page

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/features/mod-editor/view.tsx`
- Modify: `frontend/src/features/log-analysis/view.tsx`
- Delete: `frontend/src/pages/ModEditor.tsx`
- Delete: `frontend/src/pages/LogAnalysis.tsx`

- [ ] **Step 1: 记录当前 page 壳的唯一职责**

确认 `frontend/src/pages/ModEditor.tsx` 与 `frontend/src/pages/LogAnalysis.tsx` 仅执行 feature 转发，不承载状态、路由参数或副作用。

- [ ] **Step 2: 搜索两处 page 壳的所有引用**

Run: `rg -n "pages/ModEditor|pages/LogAnalysis|ModEditor\\(|LogAnalysis\\(" .\frontend\src .\frontend\tests`

Expected: 只有少量装配层或无实际依赖。

- [ ] **Step 3: 让调用方统一依赖 feature 入口**

如有 import 仍指向 `pages/*`，改为 `features/*/view.tsx`。

- [ ] **Step 4: 删除两个纯转发 page 壳**

删除：
- `frontend/src/pages/ModEditor.tsx`
- `frontend/src/pages/LogAnalysis.tsx`

- [ ] **Step 5: 运行前端构建验证**

Run: `npm run build`

Expected: PASS，构建成功且无 page 壳缺失错误。

## Chunk 2: Move Batch Flow Back To Feature Layer

### Task 2: 把批量模式主体从 page 层迁回 feature 层

**Files:**
- Modify: `frontend/src/features/batch-generation/view.tsx`
- Modify: `frontend/src/pages/BatchMode.tsx`
- Test: `frontend/tests/feature-shell.test.ts`
- Test: `frontend/tests/batchSocket.test.ts`

- [ ] **Step 1: 明确最终边界**

目标边界：
- `features/batch-generation/view.tsx` 承载批量流程主体
- `pages/BatchMode.tsx` 如果保留，只作为极薄路由壳

- [ ] **Step 2: 先写或补充边界测试**

至少覆盖：
- `BatchGenerationFeatureView` 可独立渲染
- page 壳不再承载业务逻辑

- [ ] **Step 3: 运行测试确认当前边界问题可被感知**

Run: `npm test -- feature-shell`

Expected: 若当前没有该测试则先补；补后应能约束“feature 不再反向依赖 page”。

- [ ] **Step 4: 搬迁主体逻辑到 feature 文件**

把当前批量流程主状态、事件绑定、渲染主体移入 `frontend/src/features/batch-generation/view.tsx`。

- [ ] **Step 5: 将 page 压缩为薄壳或删除**

若保留 page：

```tsx
import { BatchGenerationFeatureView } from "../features/batch-generation/view";

export default function BatchMode() {
  return <BatchGenerationFeatureView />;
}
```

- [ ] **Step 6: 运行前端构建与相关测试**

Run: `npm run build`

Expected: PASS

如测试命令已配置，再运行：

Run: `npm test -- batchSocket`

Expected: PASS

## Chunk 3: Keep WS Compat Layer Thin Until Flag Removal

### Task 3: 收口前端 websocket 兼容层职责

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/lib/ws.ts`
- Modify: `frontend/src/lib/batch_ws.ts`
- Modify: `frontend/src/shared/ws/facade.ts`
- Test: `frontend/tests/workflowSocket.test.ts`
- Test: `frontend/tests/batchSocket.test.ts`

- [ ] **Step 1: 列出单资产与批量流程对 websocket 的最小需求**

最小职责应只包括：
- `on`
- `send`
- `waitOpen`
- `close`
- 单资产持久错误处理绑定
- 批量事件类型收敛

- [ ] **Step 2: 为兼容层写约束测试**

测试要证明：
- `lib/ws.ts` 只在 facade 之上补单资产错误处理
- `lib/batch_ws.ts` 只在 facade 之上定义 endpoint 与类型

- [ ] **Step 3: 运行测试确认约束生效**

Run: `npm test -- workflowSocket`

Expected: PASS

- [ ] **Step 4: 清理兼容层中的重复逻辑**

禁止在 `lib/*` 中继续复制：
- websocket 创建逻辑
- 通用事件派发逻辑
- 可下沉到 `shared/ws/facade.ts` 的通用行为

- [ ] **Step 5: 保留迁移开关但收窄调用面**

`frontend/src/App.tsx` 仍可保留：
- `use_unified_ws_contract`

但旧分支只能调用薄壳兼容层，不再允许分叉出第二套业务逻辑。

- [ ] **Step 6: 运行前端构建**

Run: `npm run build`

Expected: PASS

## Chunk 4: Retire Backend Facades In Two Steps

### Task 4: 先让 router 依赖稳定的模块入口

**Files:**
- Modify: `backend/routers/workflow.py`
- Modify: `backend/routers/batch_workflow.py`
- Modify: `backend/routers/build_deploy.py`
- Modify: `backend/app/modules/planning/application/services.py`
- Modify: `backend/app/modules/codegen/application/services.py`
- Test: `backend/tests/test_planner.py`
- Test: `backend/tests/test_review_fixes.py`
- Test: `backend/tests/test_codegen_module.py`

- [ ] **Step 1: 定义 router 需要的正式入口**

推荐提供明确入口函数或 service provider，避免 router 直接拼装模块对象。

- [ ] **Step 2: 先为新入口补测试**

至少覆盖：
- 规划入口可返回 `ModPlan`
- 代码生成入口可触发 `create_asset`
- 构建入口可调用 `build_and_fix`

- [ ] **Step 3: 运行对应后端测试，确认新入口测试先失败或缺失**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_planner.py -v`

Expected: 若补充新入口测试，应先失败或暴露未接线问题。

- [ ] **Step 4: 让 router 改依赖新入口**

把 `workflow.py`、`batch_workflow.py`、`build_deploy.py` 从直接 import `agents/*` 改为依赖模块化正式入口。

- [ ] **Step 5: 保留 `backend/agents/*` 作为临时 facade**

此阶段不删除：
- `backend/agents/planner.py`
- `backend/agents/code_agent.py`

只要求它们不再是 router 的主依赖。

- [ ] **Step 6: 运行后端相关测试**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests\test_planner.py .\backend\tests\test_codegen_module.py .\backend\tests\test_review_fixes.py -v`

Expected: PASS

### Task 5: 再删除后端无人引用 facade

**Files:**
- Modify: `backend/tests/test_planner.py`
- Modify: `backend/tests/test_review_fixes.py`
- Delete: `backend/agents/planner.py`
- Delete: `backend/agents/code_agent.py`

- [ ] **Step 1: 搜索 `backend/agents/planner.py` 与 `backend/agents/code_agent.py` 的剩余引用**

Run: `rg -n "agents\\.planner|agents\\.code_agent|from agents import sts2_guidance" .\backend`

Expected: 只剩测试或明确待迁移点。

- [ ] **Step 2: 把剩余测试切到新入口**

不再让测试固化旧 facade 名称。

- [ ] **Step 3: 删除无人引用的 facade**

仅在 `rg` 结果确认无运行时调用后执行删除。

- [ ] **Step 4: 运行后端全量测试子集**

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests -v`

Expected: PASS

## Chunk 5: Remove Migration Flags And Update Docs

### Task 6: 下线迁移开关并更新结构文档

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/shared/api/config.ts`
- Modify: `backend/app/shared/infra/feature_flags.py`
- Modify: `docs/02-现状/当前项目结构报告.md`
- Modify: `docs/03-方案/重构/未开始/2026-03-24-模块化解耦重构计划书.md`

- [ ] **Step 1: 确认前端旧 websocket 路径已不再承担回退职责**

只有在 Chunk 2 和 Chunk 3 完成后，才允许执行此步。

- [ ] **Step 2: 删除 `use_unified_ws_contract` 等迁移开关**

要求：
- 不保留死分支
- 不保留注释式“以后再删”

- [ ] **Step 3: 更新文档中的结构描述**

把以下结论同步到文档：
- `shared/*` 与 `features/*` 是前端主干
- `app/modules/*` 是后端主干
- 已移除的 compat/page/facade 不再列为现状

- [ ] **Step 4: 运行最终验证**

Run: `npm run build`

Expected: PASS

Run: `.\backend\.venv\Scripts\python.exe -m pytest .\backend\tests -v`

Expected: PASS

## Follow-Up Checklist

以下事项明确属于“后续处理”，不要混入第一轮低风险清理：

- [ ] `BatchMode.tsx` 主体逻辑迁回 `features/batch-generation/view.tsx`
- [ ] `lib/ws.ts` 与 `lib/batch_ws.ts` 收敛为真正薄壳
- [ ] router 停止直接依赖 `backend/agents/*`
- [ ] 测试不再固化旧 facade 路径
- [ ] 下线 websocket 迁移开关
- [ ] 删除无人引用 facade
- [ ] 更新 `docs/02-现状/当前项目结构报告.md`

## Definition Of Done

满足以下条件才算兼容层清理完成：

- 前端不存在纯转发 page 壳
- `BatchMode` 主体逻辑回到 feature 层
- websocket 只有一套主干实现，兼容壳仅在迁移期存在且极薄
- backend router 不再依赖 `backend/agents/*`
- 迁移开关已下线
- 文档结构描述与代码现状一致

