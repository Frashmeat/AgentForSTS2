## Why

当前工作流日志在单资产、批量规划、Mod 修改、构建部署、日志分析等入口里分别以 `string[]` 的方式散落保存，前端只能直接渲染原始流文本，既无法稳定显示“当前实际调用的模型”，也无法同时提供“原始输出”和“优化后输出”两种视图。随着 Claude / Codex 与更多工作流节点共存，日志展示已经从“能看见输出”升级为“需要稳定解释输出”，因此需要先建立统一的日志展示能力，再进入实现。

## What Changes

- 为工作流新增统一的日志展示能力，要求整个工作流支持在单面板中切换“优化后输出”和“原始输出”。
- 为工作流新增统一的模型显示能力，要求日志面板固定显示当前实际调用的模型名。
- 为后端流式事件补充稳定元信息，支持前端区分原始日志、阶段日志、stderr、系统提示与模型信息来源。
- 约定本次变更优先覆盖单资产、批量规划、Mod 修改、构建部署和日志分析等工作流主链路，不扩展到用户中心历史任务回放。

## Capabilities

### New Capabilities
- `workflow-log-dual-view`: 定义工作流日志如何同时提供“原始输出”和“优化后输出”，并约束前端默认展示与切换方式。
- `workflow-runtime-model-indicator`: 定义工作流日志面板如何稳定显示当前实际调用的模型名，以及在无模型阶段时的表现。

### Modified Capabilities
- 无

## Impact

- 前端日志展示与状态：
  - `frontend/src/components/AgentLog.tsx`
  - `frontend/src/features/single-asset/*`
  - `frontend/src/features/batch-generation/*`
  - `frontend/src/features/mod-editor/view.tsx`
  - `frontend/src/components/BuildDeploy.tsx`
  - `frontend/src/features/log-analysis/view.tsx`
- 前端工作流 websocket / 事件适配层：
  - `frontend/src/lib/*.ts`
  - `frontend/src/shared/types/workflow.ts`
- 后端流式输出链路：
  - `backend/llm/agent_backends/_runner.py`
  - `backend/llm/agent_backends/claude_cli.py`
  - `backend/llm/agent_backends/codex_cli.py`
  - 相关 workflow / batch / build / log 分析流式事件发送逻辑
