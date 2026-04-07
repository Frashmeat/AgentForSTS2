## Why

当前工作区前端虽然已经把单资产、批量规划、Mod 修改、日志分析收进同一个 SPA，但视觉上仍是“多个工具页并排存在”的状态：

- `/` 下四个 tab 只有浅层 tab 切换，没有统一的工作台外壳
- 各功能页各自拥有外层卡片、宽度和留白策略，信息层级不一致
- 现有主色仍偏 `amber/slate`，与用户希望的“控制台 / 平台后台”风格不一致

用户已经明确把范围定为 `B`：统一工作区外壳，而不是只改首页，也不是全站换肤。因此需要一次规划型变更，把工作区 `/` 路由改造成统一控制台壳层，并锁定不影响 `/auth/*`、`/me/*` 等平台页面。

## What Changes

- 为工作区 `/` 路由引入统一的控制台壳层：左侧功能轨、顶部操作条、页面标题区、卡片化内容容器。
- 以用户给定配色 `#13132B / #1B1553 / #241953 / #724A91 / #626B96` 建立工作区主题 token，而不是在各页面继续散落使用 `amber-*`。
- 将 `single / batch / edit / log` 四个工作区功能页改为“内容模块”，由共享壳层提供外部导航、标题、基础面板和响应式约束。
- 保持现有路由与查询参数模型不变：仍以 `/?tab=single|batch|edit|log` 驱动页面切换。
- 明确本次边界：
  - 只改工作区 `/` 相关界面
  - 不改认证页、用户中心页、任务详情页
  - 不改后端接口、工作流状态机和平台分流逻辑

## Capabilities

### New Capabilities

- `workspace-console-shell`: 工作区必须以统一控制台壳层呈现导航、标题、操作入口和内容容器。

### Modified Capabilities

- 无

## Impact

- `frontend/src/App.tsx` 中工作区壳层、tab 切换和设置入口布局
- `frontend/src/index.css` 或新增共享样式文件中的主题变量、背景、滚动条和通用 surface token
- `frontend/src/features/single-asset/view.tsx`
- `frontend/src/features/batch-generation/view.tsx`
- `frontend/src/features/mod-editor/view.tsx`
- `frontend/src/features/log-analysis/view.tsx`
- 可能新增工作区共享 UI 组件，例如 `frontend/src/components/workspace/*`
