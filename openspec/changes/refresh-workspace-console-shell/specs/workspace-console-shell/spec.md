## ADDED Requirements

### Requirement: Workspace Routes MUST Render Inside a Unified Console Shell

The frontend MUST render all workspace routes under `/` inside a shared console-style shell instead of letting each workspace feature own its own page-level outer layout.

#### Scenario: Switching between workspace tabs keeps a consistent shell

- **WHEN** 用户在 `single`、`batch`、`edit`、`log` 之间切换
- **THEN** 左侧导航、顶部标题区和主内容容器必须保持一致
- **AND** 页面切换不得退回为各功能页各自独立的整页外壳

#### Scenario: Workspace shell scope stays limited to root workspace

- **WHEN** 用户访问 `/auth/*`、`/me/*` 或其他非工作区路由
- **THEN** 这些页面不得被工作区控制台壳层包裹

### Requirement: Workspace Shell MUST Use the Approved Purple-Blue Theme Tokens

The workspace shell MUST derive its primary visual language from the approved palette `#13132B`, `#1B1553`, `#241953`, `#724A91`, and `#626B96`, and MUST expose those values through reusable semantic theme tokens.

#### Scenario: Navigation and accent visuals use the new palette

- **WHEN** 用户查看工作区导航、主按钮、标题强调和关键状态装饰
- **THEN** 这些元素必须以紫蓝系主题 token 呈现
- **AND** 不得继续以 `amber-*` 作为工作区主视觉色

#### Scenario: Surface styles stay reusable across workspace features

- **WHEN** `single`、`batch`、`edit`、`log` 四个工作区内容面板渲染
- **THEN** 它们必须复用统一的背景、描边和 surface 语义样式
- **AND** 不得为每个页面重新定义一套互相冲突的外层视觉规则

### Requirement: Workspace Navigation MUST Remain Reachable on Small Screens

The workspace shell MUST provide a responsive fallback so that all workspace tabs remain reachable on narrow screens.

#### Scenario: Narrow screen navigation fallback

- **WHEN** 工作区在窄屏或移动端宽度下渲染
- **THEN** 用户仍必须能够访问 `single`、`batch`、`edit`、`log`
- **AND** 允许侧边导航退化为顶部 tab 或抽屉入口
